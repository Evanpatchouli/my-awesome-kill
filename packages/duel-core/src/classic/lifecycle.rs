use super::skills::{HeroAdjustment, HeroMoment, HeroTurn};
use super::MatchResult;
use crate::classic_data::{self, Role};
use crate::scheduler::{EventRef, Outcome};
use crate::{
    Answer, CardId, CardKind, ChoiceBranch, ChoiceKind, ChoiceRequest, ChoiceView, Decision,
    DrawReason, EventKind, Game, Phase, Program, Resolution, SeatId, TurnView, Zone,
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
        let state = self.classic.as_mut().expect("the table is classic");
        state.turn = Some(turn);
        state.sha_played = false;
        state.hero_turn = HeroTurn::default();
        state.skip_play = false;
        self.active_seat = turn.seat;
        self.scheduler.start(Program::event(EventKind::Turn {
            seat: turn.seat,
            number: turn.number,
            round: turn.round,
        }));
        true
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
            EventKind::Turn { seat, .. } => self.resolve_turn(seat, child),
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

    fn resolve_turn(&mut self, seat: SeatId, child: Option<Outcome<Resolution>>) {
        if self.match_result().is_some() || !self.seat(seat).expect("the turn seat exists").alive {
            self.finish();
            return;
        }
        if child.is_none() && self.seat(seat).is_ok_and(|player| player.turned_over) {
            if let Some(flip) = self.seat_change(seat, super::elemental::Change::TurnOver(false)) {
                self.scheduler.next(flip);
            }
            self.scheduler.end(Outcome::Cancelled);
            return;
        }
        let phase = match child {
            None => Phase::Preparation,
            Some(Outcome::Finished(Resolution::PhaseFinished(Phase::Preparation))) => {
                Phase::Judgment
            }
            Some(Outcome::Finished(Resolution::PhaseFinished(Phase::Judgment))) => Phase::Draw,
            Some(Outcome::Finished(Resolution::PhaseFinished(Phase::Draw))) => Phase::Play,
            Some(Outcome::Finished(Resolution::PhaseFinished(Phase::Play))) => Phase::Discard,
            Some(Outcome::Finished(Resolution::PhaseFinished(Phase::Discard))) => Phase::End,
            Some(Outcome::Finished(Resolution::PhaseFinished(Phase::End))) => {
                self.finish();
                return;
            }
            _ => unreachable!("a turn advances after a phase finishes"),
        };
        self.next(EventKind::Phase { seat, phase });
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
            Some(Answer::EndPlay) => self.finish_phase(Phase::Play),
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
                        .next(Program::VirtualTao { seat, target, card });
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
