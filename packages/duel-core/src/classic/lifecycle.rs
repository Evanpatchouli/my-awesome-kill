use super::skills::{HeroAdjustment, HeroMoment, HeroTurn};
use super::MatchResult;
use crate::classic_data::{self, Role};
use crate::scheduler::{EventRef, Outcome};
use crate::{
    Answer, CardId, CardKind, ChoiceBranch, ChoiceKind, ChoiceRequest, ChoiceView, Decision,
    DrawReason, EventKind, Game, Phase, Program, Resolution, SeatId, TurnFlow, TurnStage, TurnView,
    Zone,
};

impl Game {
    pub(crate) fn start_next_turn(&mut self) -> bool {
        let Some(state) = self.classic.as_ref() else {
            return false;
        };
        if let Some(result) = state.result {
            if state.result_announced {
                return false;
            }
            self.classic
                .as_mut()
                .expect("the table is classic")
                .result_announced = true;
            self.scheduler.start(Program::event(EventKind::GameOver {
                winner: result.winner(),
            }));
            return true;
        }

        let turn = if let Some(current) = state.turn {
            let seat = self
                .living_order_from(current.seat)
                .into_iter()
                .find(|seat| *seat != current.seat)
                .expect("an ongoing table has another living seat");
            let relative = |seat: SeatId| {
                (seat as usize + self.seats.len() - state.lord as usize) % self.seats.len()
            };
            let round = current.round + u64::from(relative(seat) <= relative(current.seat));
            TurnView {
                seat,
                number: current.number + 1,
                round,
                phase: Phase::Preparation,
            }
        } else {
            TurnView {
                seat: state.lord,
                number: 1,
                round: 1,
                phase: Phase::Preparation,
            }
        };
        let round_start = state
            .turn
            .is_none_or(|previous| previous.round != turn.round);
        let state = self.classic.as_mut().expect("the table is classic");
        state.turn = Some(turn);
        state.sha_played = false;
        state.hero_turn = HeroTurn::default();
        state.skip_play = false;
        self.active_seat = turn.seat;
        self.scheduler.start(Program::Turn(TurnFlow {
            seat: turn.seat,
            number: turn.number,
            round: turn.round,
            stage: TurnStage::RoundStart,
            round_start,
        }));
        true
    }

    pub(crate) fn resolve_turn_flow(
        &mut self,
        mut flow: TurnFlow,
        child: Option<Outcome<Resolution>>,
    ) {
        let seat = flow.seat;
        if self.match_result().is_some() || !self.seat(seat).expect("the turn seat exists").alive {
            self.finish();
            return;
        }
        // Cancelling a phase finishes that child. Cancelling a timing on the turn itself ends the turn.
        let child = match child {
            Some(Outcome::Cancelled) if matches!(flow.stage, TurnStage::Phases) => {
                let phase = self
                    .classic
                    .as_ref()
                    .and_then(|state| state.turn)
                    .expect("active turn")
                    .phase;
                Some(Outcome::Finished(Resolution::PhaseFinished(phase)))
            }
            Some(Outcome::Cancelled) => {
                self.scheduler.end(Outcome::Cancelled);
                return;
            }
            outcome => outcome,
        };
        loop {
            let timing = match flow.stage {
                TurnStage::RoundStart => {
                    flow.stage = TurnStage::BeforeStart;
                    flow.round_start.then_some("roundStart")
                }
                TurnStage::BeforeStart => {
                    flow.stage = TurnStage::StartPhases;
                    Some("phaseBeforeStart")
                }
                TurnStage::StartPhases => {
                    if self.seat(seat).is_ok_and(|player| player.turned_over) {
                        if let Some(flip) =
                            self.seat_change(seat, super::elemental::Change::TurnOver(false))
                        {
                            self.scheduler.next(flip);
                        }
                        self.scheduler.end(Outcome::Cancelled);
                        return;
                    }
                    flow.stage = TurnStage::Phases;
                    self.scheduler.replace_current(Program::Turn(flow));
                    self.next(EventKind::Phase {
                        seat,
                        phase: Phase::Preparation,
                    });
                    return;
                }
                TurnStage::Phases => {
                    if matches!(
                        child,
                        Some(Outcome::Finished(Resolution::PhaseFinished(Phase::End)))
                    ) {
                        flow.stage = TurnStage::End;
                        continue;
                    }
                    let phase = match child {
                        Some(Outcome::Finished(Resolution::PhaseFinished(Phase::Preparation))) => {
                            Phase::Judgment
                        }
                        Some(Outcome::Finished(Resolution::PhaseFinished(Phase::Judgment))) => {
                            Phase::Draw
                        }
                        Some(Outcome::Finished(Resolution::PhaseFinished(Phase::Draw))) => {
                            Phase::Play
                        }
                        Some(Outcome::Finished(Resolution::PhaseFinished(Phase::Play))) => {
                            Phase::Discard
                        }
                        Some(Outcome::Finished(Resolution::PhaseFinished(Phase::Discard))) => {
                            Phase::End
                        }
                        _ => unreachable!("a turn advances after a phase finishes"),
                    };
                    self.next(EventKind::Phase { seat, phase });
                    return;
                }
                TurnStage::End => {
                    flow.stage = TurnStage::After;
                    Some("phaseEnd")
                }
                TurnStage::After => {
                    flow.stage = TurnStage::Finish;
                    Some("phaseAfter")
                }
                TurnStage::Finish => {
                    self.finish();
                    return;
                }
            };
            if let Some(timing) = timing {
                if self.queue_timing(timing, Some(seat), None, None, 0, &[]) {
                    self.scheduler.replace_current(Program::Turn(flow));
                    return;
                }
            }
        }
    }

    pub(crate) fn resolve_classic(
        &mut self,
        event: EventRef,
        kind: EventKind,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        match kind {
            EventKind::Opening { lord } => {
                for seat in self.living_order_from(lord) {
                    self.next(EventKind::Draw {
                        seat,
                        amount: 4,
                        reason: DrawReason::Opening,
                    });
                }
                self.finish();
            }
            EventKind::Phase { seat, phase } => {
                self.resolve_phase(event, seat, phase, child, answer)
            }
            EventKind::Draw { seat, amount, .. } => self.resolve_draw(seat, amount),
            EventKind::Dying { source, target } => self.resolve_dying(source, target, child),
            EventKind::ChooseSave { seat, target } => {
                self.resolve_save(event, seat, target, child, answer)
            }
            EventKind::Death {
                source,
                target,
                role,
            } => self.resolve_death(source, target, role),
            EventKind::Penalty { lord } => {
                self.queue_zone_discards(lord, &[Zone::Hand, Zone::Equipment]);
                self.finish();
            }
            EventKind::GameOver { .. } => self.finish(),
            _ => unreachable!("the event cannot receive this child result"),
        }
    }

    fn resolve_phase(
        &mut self,
        event: EventRef,
        seat: SeatId,
        phase: Phase,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        let state = self.classic.as_mut().expect("the table is classic");
        state.turn.as_mut().expect("the turn has started").phase = phase;
        if phase == Phase::Play && state.skip_play {
            self.finish_phase(phase);
            return;
        }
        if self.match_result().is_some() || !self.seat(seat).expect("the turn seat exists").alive {
            self.finish_phase(phase);
            return;
        }
        match phase {
            Phase::Preparation
                if child.is_none() && self.queue_hero_moment(HeroMoment::Preparation { seat }) => {}
            Phase::End if child.is_none() && self.queue_hero_moment(HeroMoment::End { seat }) => {}
            Phase::Judgment => {
                if let Some(card) = self
                    .seat(seat)
                    .expect("the turn seat exists")
                    .judges
                    .last()
                    .copied()
                {
                    self.scheduler.next(self.trick_judgment(seat, card));
                } else {
                    self.finish_phase(phase);
                }
            }
            Phase::Draw => match child {
                None => {
                    if !self.queue_hero_moment(HeroMoment::Draw { seat, amount: 2 }) {
                        self.queue_draw(seat, 2, DrawReason::Phase);
                    }
                }
                Some(Outcome::Finished(Resolution::Hero(HeroAdjustment::Draw(amount))))
                    if amount > 0 =>
                {
                    self.queue_draw(seat, amount, DrawReason::Phase);
                }
                Some(_) => self.finish_phase(phase),
            },
            Phase::Play
                if child.is_none()
                    && answer.is_none()
                    && self.queue_timing("phaseUseBegin", Some(seat), None, None, 0, &[]) =>
            {
                self.scheduler
                    .replace_current(Program::PlayAfterBegin(seat));
            }
            Phase::Play => self.resolve_play(seat, answer),
            Phase::Discard => {
                if matches!(
                    child,
                    Some(Outcome::Finished(Resolution::Hero(HeroAdjustment::Skip)))
                ) {
                    self.finish_phase(phase);
                } else if child.is_some()
                    || answer.is_some()
                    || !self.queue_hero_moment(HeroMoment::Discard { seat })
                {
                    self.resolve_discard_phase(event, seat, answer);
                }
            }
            _ => self.finish_phase(phase),
        }
    }

    fn finish_phase(&mut self, phase: Phase) {
        self.scheduler
            .end(Outcome::Finished(Resolution::PhaseFinished(phase)));
    }

    pub(crate) fn resume_play_begin(
        &mut self,
        event: EventRef,
        seat: SeatId,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        self.scheduler
            .replace_current(Program::event(EventKind::Phase {
                seat,
                phase: Phase::Play,
            }));
        if matches!(child, Some(Outcome::Cancelled)) {
            self.finish_phase(Phase::Play);
        } else {
            self.resolve_phase(event, seat, Phase::Play, child, answer);
        }
    }

    fn resolve_play(&mut self, seat: SeatId, answer: &mut Option<Answer>) {
        match answer.take() {
            None => self.decision = Some(Decision::Play(seat)),
            Some(Answer::EndPlay) => {
                if self.queue_timing("phaseUseEnd", Some(seat), None, None, 0, &[]) {
                    self.scheduler.replace_current(Program::PlayAfterEnd(seat));
                } else {
                    self.finish_phase(Phase::Play);
                }
            }
            Some(Answer::Play(program)) => {
                if matches!(program.kind(), EventKind::Recast { .. }) {
                    self.classic
                        .as_mut()
                        .expect("the table is classic")
                        .hero_turn
                        .recast_attempted = true;
                }
                if !matches!(
                    program.kind(),
                    EventKind::Skill {
                        skill: crate::SkillId::Jijiang,
                        ..
                    }
                ) {
                    self.note_hero_non_jijiang_play(seat);
                }
                if matches!(
                    program.kind(),
                    EventKind::UseSha { .. }
                        | EventKind::UseShaTargets { .. }
                        | EventKind::UseVirtual {
                            kind: CardKind::Sha,
                            ..
                        }
                ) {
                    self.classic
                        .as_mut()
                        .expect("the table is classic")
                        .sha_played = true;
                }
                self.scheduler.next(program);
            }
            _ => unreachable!("the play phase receives a play action or ends"),
        }
    }

    fn resolve_discard_phase(
        &mut self,
        event: EventRef,
        seat: SeatId,
        answer: &mut Option<Answer>,
    ) {
        if let Some(Answer::Discard(cards)) = answer.take() {
            self.queue_hand_discards(seat, &cards);
            self.finish_phase(Phase::Discard);
            return;
        }
        let player = self.seat(seat).expect("the turn seat exists");
        let limit = usize::try_from(player.hp).unwrap_or_default();
        let required = player.hand.len().saturating_sub(limit);
        if required == 0 {
            self.finish_phase(Phase::Discard);
        } else {
            let cards = player.hand.iter().map(|card| card.id).collect();
            self.decision = Some(Decision::Cards(ChoiceView {
                event: event.id,
                seat,
                kind: ChoiceKind::Discard,
                target: None,
                min: required,
                max: required,
                cards,
                branches: vec![],
                counter: None,
                compare: None,
            }));
        }
    }

    fn queue_hand_discards(&mut self, seat: SeatId, ids: &[CardId]) {
        let cards = self
            .hand_cards_by_id(seat, ids)
            .into_iter()
            .map(|card| (Zone::Hand, card))
            .collect();
        self.queue_discard_batch(seat, cards);
    }

    fn queue_zone_discards(&mut self, seat: SeatId, zones: &[Zone]) {
        let player = self
            .seats
            .iter()
            .find(|player| player.id == seat)
            .expect("the seat exists");
        let cards = zones
            .iter()
            .copied()
            .flat_map(|zone| {
                let cards = match zone {
                    Zone::Hand => &player.hand,
                    Zone::Equipment => &player.equipment,
                    Zone::Judgment => &player.judges,
                };
                cards.iter().copied().map(move |card| (zone, card))
            })
            .collect();
        self.queue_discard_batch(seat, cards);
    }

    fn resolve_draw(&mut self, seat: SeatId, amount: u16) {
        let Some(cards) = self.draw_from_piles(usize::from(amount)) else {
            self.scheduler.end(Outcome::Cancelled);
            return;
        };
        self.seat_mut(seat).hand.extend(cards);
        self.finish();
    }

    pub(crate) fn queue_draw(&mut self, seat: SeatId, amount: u16, reason: DrawReason) {
        self.next(EventKind::Draw {
            seat,
            amount,
            reason,
        });
    }

    fn resolve_dying(
        &mut self,
        source: Option<SeatId>,
        target: SeatId,
        child: Option<Outcome<Resolution>>,
    ) {
        if self.match_result().is_some()
            || self.seat(target).expect("the dying seat exists").hp > 0
            || matches!(child, Some(Outcome::Finished(Resolution::Complete)))
        {
            self.finish();
            return;
        }

        let order = self.living_order_from(self.active_seat);
        let next = match child {
            None => order.first(),
            Some(Outcome::Finished(Resolution::SavePassed(previous))) => {
                let index = order
                    .iter()
                    .position(|id| *id == previous)
                    .expect("the save actor lives");
                order.get(index + 1)
            }
            _ => unreachable!("the dying event only waits for save decisions and death"),
        };
        if let Some(seat) = next {
            self.next(EventKind::ChooseSave {
                seat: *seat,
                target,
            });
        } else {
            let role = self
                .seat(target)
                .expect("the dying seat exists")
                .role
                .expect("the seat has a role");
            self.next(EventKind::Death {
                source,
                target,
                role,
            });
        }
    }

    fn resolve_save(
        &mut self,
        event: EventRef,
        seat: SeatId,
        target: SeatId,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        if self.seat(target).expect("the dying seat exists").hp > 0 {
            self.scheduler
                .end(Outcome::Finished(Resolution::SavePassed(seat)));
            return;
        }
        if let Some(Outcome::Finished(Resolution::Selected(selection))) = child {
            if selection.branch == 0 {
                self.scheduler
                    .end(Outcome::Finished(Resolution::SavePassed(seat)));
            } else {
                let card = self.hero_cards_by_id(seat, &selection.cards)[0];
                if selection.branch == 1 {
                    self.next(EventKind::UseTao { seat, target, card });
                } else {
                    self.scheduler
                        .next(Program::BasicUse(super::basic_use::Flow::tao(
                            seat, target, card, true,
                        )));
                }
            }
            return;
        }
        match answer.take() {
            None => {
                let conversions = self.hero_response_conversions(seat, CardKind::Tao);
                if conversions.is_empty() {
                    self.ask_for_card(event, seat, ChoiceKind::Save, Some(target), CardKind::Tao);
                } else {
                    let mut branches = vec![save_branch(0, "放弃救援", vec![], 0)];
                    let cards: Vec<_> = self
                        .seat(seat)
                        .expect("the saving actor exists")
                        .hand
                        .iter()
                        .filter(|card| card.kind == CardKind::Tao)
                        .map(|card| card.id)
                        .collect();
                    if !cards.is_empty() {
                        branches.push(save_branch(1, "使用桃救援", cards, 1));
                    }
                    branches.extend(conversions);
                    self.scheduler.next(Program::Choose(ChoiceRequest {
                        seat,
                        target: Some(target),
                        branches,
                    }));
                }
            }
            Some(Answer::Pass) => self
                .scheduler
                .end(Outcome::Finished(Resolution::SavePassed(seat))),
            Some(Answer::Card(card)) => self.next(EventKind::UseTao { seat, target, card }),
            _ => unreachable!("a save choice receives one card or a pass"),
        }
    }

    fn resolve_death(&mut self, source: Option<SeatId>, target: SeatId, role: Role) {
        let player = self.seat_mut(target);
        player.alive = false;
        player.hp = 0;
        let alive: Vec<_> = self
            .seats
            .iter()
            .filter(|seat| seat.alive)
            .filter_map(|seat| seat.role)
            .collect();
        let winner = classic_data::winning_side(&alive);
        self.queue_zone_discards(target, &[Zone::Hand, Zone::Equipment, Zone::Judgment]);

        if let Some(winner) = winner {
            self.classic.as_mut().expect("the table is classic").result =
                Some(MatchResult::Winner(winner));
        } else if let Some(killer) = source
            .and_then(|id| self.seat(id).ok())
            .filter(|seat| seat.alive)
        {
            match role {
                Role::Rebel => self.next(EventKind::Draw {
                    seat: killer.id,
                    amount: 3,
                    reason: DrawReason::RebelReward,
                }),
                Role::Loyalist if killer.role == Some(Role::Lord) => {
                    self.next(EventKind::Penalty { lord: killer.id })
                }
                _ => {}
            }
        }
        self.finish();
    }
}

fn save_branch(id: u32, label: &str, cards: Vec<CardId>, count: usize) -> ChoiceBranch {
    ChoiceBranch {
        id,
        label: label.into(),
        cards,
        min_cards: count,
        max_cards: count,
        targets: vec![],
        min_targets: 0,
        max_targets: 0,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Action, ChoiceKind, Command, Game, TriggerOrder, TriggerRegistration, TriggerRole,
        TriggerStep,
    };
    use std::sync::{Arc, Mutex};

    #[test]
    fn turn_timings_wait_for_callbacks_across_rounds_and_face_down_turn() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut registrations = Vec::new();
        for timing in [
            "roundStart",
            "phaseBeforeStart",
            "phaseUseEnd",
            "phaseEnd",
            "phaseAfter",
        ] {
            let observed = log.clone();
            registrations.push(TriggerRegistration {
                owner: 0,
                skill: format!("probe_{timing}"),
                timing: timing.into(),
                role: TriggerRole::Global,
                order: TriggerOrder::First,
                priority: 0,
                label: None,
                silent: true,
                filter: Arc::new(|_, _, _| true),
                effect: Arc::new(move |ctx, _, _| {
                    let turn = ctx.game().classic.as_ref().unwrap().turn.unwrap();
                    observed.lock().unwrap().push((
                        turn.number,
                        turn.round,
                        ctx.event().timing.clone(),
                        "callback",
                        ctx.game().seat(turn.seat).unwrap().turned_over,
                    ));
                    TriggerStep::Done
                }),
            });
        }
        let mut game = Game::classic_with_triggers(5, 0, "guanyu", 4, registrations).unwrap();
        let first = game.classic.as_ref().unwrap().turn.unwrap();
        let flipped = game.living_order_from(first.seat)[1];
        game.seat_mut(flipped).turned_over = true;
        let observed = log.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if [
                "roundStart",
                "phaseBeforeStart",
                "phaseUseEnd",
                "phaseEnd",
                "phaseAfter",
            ]
            .contains(&event.timing.as_str())
            {
                let turn = game.classic.as_ref().unwrap().turn.unwrap();
                observed.lock().unwrap().push((
                    turn.number,
                    turn.round,
                    event.timing.clone(),
                    "request",
                    game.seat(turn.seat).unwrap().turned_over,
                ));
            }
        }));
        for _ in 0..100 {
            if game.classic.as_ref().unwrap().turn.unwrap().round == 2 {
                break;
            }
            let public = game.view(None).unwrap();
            let seat = public.waiting_for.unwrap();
            let action = match game.view(Some(seat)).unwrap().choice {
                None => Action::EndPlay,
                Some(choice) => match choice.kind {
                    ChoiceKind::Discard => Action::Discard {
                        event: choice.event,
                        cards: choice.cards[..choice.min].to_vec(),
                    },
                    ChoiceKind::Shan | ChoiceKind::Save => Action::Choose {
                        event: choice.event,
                        card: None,
                    },
                    ChoiceKind::Content => {
                        let branch = choice
                            .branches
                            .iter()
                            .find(|b| b.id == 0)
                            .unwrap_or(&choice.branches[0]);
                        Action::Select {
                            event: choice.event,
                            branch: branch.id,
                            cards: branch.cards[..branch.min_cards].to_vec(),
                            targets: branch.targets[..branch.min_targets].to_vec(),
                        }
                    }
                },
            };
            game.submit(Command {
                version: public.version,
                seat,
                action,
            })
            .unwrap();
        }
        let log = log.lock().unwrap();
        assert_eq!(log[0].2, "roundStart");
        assert_eq!(log[1].2, "phaseBeforeStart");
        assert!(log
            .iter()
            .any(|row| row.0 == 2 && row.2 == "phaseBeforeStart" && row.3 == "callback" && row.4));
        assert!(!log
            .iter()
            .any(|row| row.0 == 2
                && ["phaseUseEnd", "phaseEnd", "phaseAfter"].contains(&row.2.as_str())));
        let second_round: Vec<_> = log
            .iter()
            .filter(|row| row.1 == 2)
            .map(|row| (row.2.as_str(), row.3))
            .collect();
        assert_eq!(
            second_round,
            [
                ("roundStart", "request"),
                ("roundStart", "callback"),
                ("phaseBeforeStart", "request"),
                ("phaseBeforeStart", "callback")
            ]
        );
        for pair in log.iter().filter(|row| row.3 == "request") {
            let index = log.iter().position(|row| std::ptr::eq(row, pair)).unwrap();
            assert_eq!((&log[index + 1].2, log[index + 1].3), (&pair.2, "callback"));
        }
    }
}
