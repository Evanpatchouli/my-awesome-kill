use super::MatchResult;
use crate::classic_data::{self, Role};
use crate::scheduler::{EventRef, Outcome};
use crate::{
    Answer, CardId, CardKind, ChoiceBranch, ChoiceKind, ChoiceRequest, ChoiceView, Decision,
    DrawReason, EventKind, Game, Program, Resolution, SeatId, Zone,
};

impl Game {
    /// Change membership without moving the seat in the physical table order.
    pub(crate) fn set_seat_alive(&mut self, seat: SeatId, alive: bool) {
        let player = self.seat_mut(seat);
        if player.alive == alive {
            return;
        }
        player.alive = alive;
        let (leave, enter) = if alive {
            (&mut self.dead_seats, &mut self.living_seats)
        } else {
            (&mut self.living_seats, &mut self.dead_seats)
        };
        leave.retain(|&id| id != seat);
        enter.push(seat);
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
            EventKind::Draw { seat, amount, .. } => self.resolve_draw(seat, amount),
            EventKind::Dying { source, target } => self.resolve_dying(source, target, child),
            EventKind::ChooseSave { seat, target } => {
                self.resolve_save(event, seat, target, child, answer)
            }
            EventKind::Penalty { lord } => {
                self.queue_zone_discards(lord, &[Zone::Hand, Zone::Equipment]);
                self.finish();
            }
            EventKind::GameOver { .. } => self.finish(),
            _ => unreachable!("the event cannot receive this child result"),
        }
    }

    pub(crate) fn resolve_phase_play(&mut self, seat: SeatId, answer: &mut Option<Answer>) -> bool {
        match answer.take() {
            None => self.decision = Some(Decision::Play(seat)),
            Some(Answer::EndPlay) => return true,
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
        false
    }

    pub(crate) fn resolve_discard_phase(
        &mut self,
        event: EventRef,
        seat: SeatId,
        fixed_required: usize,
        answer: &mut Option<Answer>,
    ) -> bool {
        if let Some(Answer::Discard(cards)) = answer.take() {
            self.queue_hand_discards(seat, &cards);
            return true;
        }
        let player = self.seat(seat).expect("the turn seat exists");
        let required = fixed_required.min(player.hand.len());
        if required == 0 {
            return true;
        } else {
            let cards = player.hand.iter().map(|card| card.id).collect();
            self.decision = Some(Decision::Cards(ChoiceView {
                equipment_buttons: None,
                move_cards: None,
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
                compare_group: None,
            }));
        }
        false
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
        let virtual_judgments = if zones.contains(&Zone::Judgment) {
            self.judgments(seat)
                .rev()
                .filter(|entry| entry.is_virtual())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let player = self
            .seats
            .iter()
            .find(|player| player.id == seat)
            .expect("the seat exists");
        let cards = zones
            .iter()
            .copied()
            .flat_map(|zone| {
                let cards: &[crate::Card] = match zone {
                    Zone::Hand => &player.hand,
                    Zone::Equipment => &player.equipment,
                    Zone::Judgment => &player.judges,
                    Zone::Expansion => &player.expansions,
                };
                let virtual_judgments = &virtual_judgments;
                cards.iter().copied().filter_map(move |card| {
                    (zone != Zone::Judgment
                        || !virtual_judgments
                            .iter()
                            .any(|entry| entry.is_attached() && entry.material == card))
                    .then_some((zone, card))
                })
            })
            .collect::<Vec<_>>();
        if virtual_judgments.is_empty() {
            self.queue_discard_batch(seat, cards);
        } else {
            let held = cards.into_iter().map(|(_, card)| card).collect();
            self.scheduler
                .next(self.death_judgment_discard_program(seat, virtual_judgments, held));
        }
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
            || !self
                .seat(target)
                .is_ok_and(|player| player.is_in() && player.hp <= 0)
            || matches!(
                child,
                Some(Outcome::Finished(
                    Resolution::Complete | Resolution::DeathCancelled
                ))
            )
        {
            self.finish();
            return;
        }

        let order = self
            .seat_order_from(self.active_seat)
            .expect("the turn seat exists");
        let remaining = match child {
            None => order.as_slice(),
            Some(Outcome::Finished(Resolution::SavePassed(previous))) => {
                let index = order
                    .iter()
                    .position(|id| *id == previous)
                    .expect("the save actor has a seat");
                &order[index + 1..]
            }
            _ => unreachable!("the dying event only waits for save decisions and death"),
        };
        if let Some(&seat) = remaining.iter().find(|&&seat| self.is_in(seat)) {
            self.next(EventKind::ChooseSave { seat, target });
        } else {
            self.scheduler.next(self.death_program(target, source));
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
        if self.match_result().is_some()
            || !self.is_in(seat)
            || !self
                .seat(target)
                .is_ok_and(|player| player.is_in() && player.hp <= 0)
        {
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

    /// Whether the active death event is reserved for an out-of-play return.
    pub fn death_reserved(&self, event: crate::EventId) -> bool {
        matches!(
            self.scheduler.active_frame(event),
            Some((_, Program::Death(flow))) if flow.reserved
        )
    }

    pub(crate) fn death_program(&self, target: SeatId, source: Option<SeatId>) -> Program {
        Program::event(EventKind::Death {
            source,
            target,
            role: self.seat(target).expect("the death target exists").role,
        })
    }

    pub(crate) fn resolve_death(
        &mut self,
        mut flow: crate::DeathFlow,
        child: Option<Outcome<Resolution>>,
    ) {
        use crate::DeathStage;
        let source = flow.source;
        let target = flow.target;
        let role = flow.role;
        let stage = flow.stage;
        if matches!(child, Some(Outcome::Cancelled))
            && matches!(
                stage,
                DeathStage::Begin
                    | DeathStage::Apply
                    | DeathStage::Cleanup
                    | DeathStage::After
                    | DeathStage::Finish
            )
        {
            flow.stage = DeathStage::FinishCancelled;
            self.scheduler.replace_current(Program::Death(flow));
            self.queue_timing("dieCancelled", Some(target), source, None, 0, &[]);
            return;
        }
        match stage {
            DeathStage::Before => {
                flow.stage = DeathStage::Begin;
                self.scheduler.replace_current(Program::Death(flow));
                self.queue_timing("dieBefore", Some(target), source, None, 0, &[]);
            }
            DeathStage::Begin => {
                flow.stage = DeathStage::Apply;
                self.scheduler.replace_current(Program::Death(flow));
                self.queue_timing("dieBegin", Some(target), source, None, 0, &[]);
            }
            DeathStage::Apply => {
                if !self.is_alive(target) {
                    self.finish();
                    return;
                }
                if self.is_out(target) && !flow.include_out {
                    flow.body_skipped = true;
                    flow.stage = DeathStage::End;
                    self.scheduler.replace_current(Program::Death(flow));
                    return;
                }
                if let Some(source) = source {
                    let killer = self.seat_mut(source);
                    killer.turn_kills = killer.turn_kills.saturating_add(1);
                }
                self.set_seat_alive(target, false);
                let player = self.seat_mut(target);
                player.out = false;
                player.linked = false;
                player.turned_over = false;
                let hp = player.hp;
                flow.stage = DeathStage::Announce;
                self.scheduler.replace_current(Program::Death(flow));
                if hp != 0 {
                    self.queue_health_change(target, -i64::from(hp), None);
                }
            }
            DeathStage::Announce => {
                if !flow.reserved {
                    self.seat_mut(target).role_shown = true;
                    if let Some(table) = self.classic.as_mut() {
                        let alive: Vec<_> = self
                            .seats
                            .iter()
                            .filter(|s| s.alive)
                            .filter_map(|s| s.role)
                            .collect();
                        if table.result.is_none() {
                            table.result =
                                classic_data::winning_side(&alive).map(MatchResult::Winner);
                        }
                    }
                }
                flow.stage = DeathStage::Cleanup;
                self.scheduler.replace_current(Program::Death(flow));
                self.queue_timing("die", Some(target), source, None, 0, &[]);
            }
            DeathStage::Cleanup => {
                if !self.is_alive(target) {
                    self.clear_marks_on_death_except(target, &flow.excluded_marks);
                    if !flow
                        .excluded_marks
                        .iter()
                        .any(|mark| mark == "_rest_return")
                    {
                        self.seat_mut(target).rest_mark_shown = false;
                    }
                    self.remove_temporary_for_seat(target);
                    self.queue_zone_discards(
                        target,
                        &[Zone::Hand, Zone::Equipment, Zone::Judgment],
                    );
                    if let Some(discard) = self.discard_all_expansions_program(target) {
                        self.scheduler.next(discard);
                    }
                }
                flow.stage = DeathStage::Reward;
                self.scheduler.replace_current(Program::Death(flow));
                return;
            }
            DeathStage::Reward => {
                if !flow.reserved && self.match_result().is_none() {
                    if let Some(role) = role {
                        self.queue_death_reward(source, role);
                    }
                }
                flow.stage = DeathStage::End;
                self.scheduler.replace_current(Program::Death(flow));
            }
            DeathStage::End => {
                flow.stage = DeathStage::After;
                self.scheduler.replace_current(Program::Death(flow));
                self.queue_timing("dieEnd", Some(target), source, None, 0, &[]);
            }
            DeathStage::After => {
                flow.stage = DeathStage::Finish;
                self.scheduler.replace_current(Program::Death(flow));
                self.queue_timing("dieAfter", Some(target), source, None, 0, &[]);
                return;
            }
            DeathStage::Finish if flow.body_skipped => {
                self.scheduler
                    .end(Outcome::Finished(Resolution::BodySkipped));
            }
            DeathStage::Finish => self.finish(),
            DeathStage::FinishCancelled => self
                .scheduler
                .end(Outcome::Finished(Resolution::DeathCancelled)),
        }
    }

    fn queue_death_reward(&mut self, source: Option<SeatId>, role: Role) {
        if let Some(killer) = source
            .and_then(|id| self.seat(id).ok())
            .filter(|seat| seat.is_in())
        {
            match role {
                Role::Rebel => {
                    let draw = self.draw_many_program(killer.id, 3, DrawReason::RebelReward);
                    self.scheduler.next(draw);
                }
                Role::Loyalist if killer.role == Some(Role::Lord) => {
                    self.next(EventKind::Penalty { lord: killer.id })
                }
                _ => {}
            }
        }
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

#[cfg(test)]
#[path = "lifecycle/reserved_death_tests.rs"]
mod reserved_death_tests;
