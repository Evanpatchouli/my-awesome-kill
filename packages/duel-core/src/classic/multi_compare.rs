//! Multi-seat comparison: shared sealed bids, paired or simultaneous scoring.
use super::compare::bids::BidGroup;
use crate::scheduler::{EventRef, Outcome};
use crate::{
    Card, CompareGroup, CompareManyFrame, CompareManyResult, CompareMode, EventId, EventKind, Game,
    GroupAnnouncement, Program, Resolution, RevealedBid, RuleError, SeatId, TriggerEffect,
    TriggerEvent,
};
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(test)]
#[path = "multi_compare_tests.rs"]
mod tests;

#[derive(Default)]
pub(crate) struct State {
    live: HashMap<EventId, CompareManyFrame>,
    history: Vec<CompareManyFrame>,
    forced: HashMap<EventId, SeatId>,
}
#[derive(Clone, Copy)]
enum Stage {
    Before,
    Begin,
    Collect,
    Collected,
    Paid,
    Reveal,
    ShowOwner,
    PairStart,
    PairCompare,
    PairFix,
    PairSettle,
    PairNext,
    SimShow,
    SimOwnerCompare,
    SimTarget,
    SimTargetStore,
    SimFixOwner,
    SimFixTarget,
    SimFixNext,
    End,
    After,
    Finish,
    CancelNotify,
    CancelFinish,
}
#[derive(Clone)]
pub(crate) struct Flow {
    owner: SeatId,
    targets: Vec<SeatId>,
    mode: CompareMode,
    callback: Option<TriggerEffect>,
    cards: Vec<Card>,
    index: usize,
    best: Option<i32>,
    winner: Option<SeatId>,
    stage: Stage,
}
impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        EventKind::CompareMany {
            seat: self.owner,
            count: self.targets.len(),
            mode: self.mode,
        }
    }
}
impl Game {
    pub(crate) fn compare_many_program(
        &self,
        owner: SeatId,
        targets: Vec<SeatId>,
        mode: CompareMode,
        callback: Option<TriggerEffect>,
    ) -> Result<Program, RuleError> {
        if self.match_result().is_some() {
            return Err(RuleError::GameOver);
        }
        if self.classic.is_none() {
            return Err(RuleError::UnsupportedCard);
        }
        if targets.is_empty()
            || targets.contains(&owner)
            || targets
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != targets.len()
        {
            return Err(RuleError::InvalidTarget);
        }
        for seat in std::iter::once(owner).chain(targets.iter().copied()) {
            let player = self.seat(seat)?;
            if !player.alive {
                return Err(RuleError::InactiveSeat { seat });
            }
        }
        Ok(Program::GroupCompare(Flow {
            owner,
            targets,
            mode,
            callback,
            cards: Vec::new(),
            index: 0,
            best: None,
            winner: None,
            stage: Stage::Before,
        }))
    }
    pub fn group_comparison(&self, id: EventId) -> Option<&CompareManyFrame> {
        self.group_compares.live.get(&id).or_else(|| {
            self.group_compares
                .history
                .iter()
                .rev()
                .find(|frame| frame.event == id)
        })
    }
    pub fn group_compare_history(&self) -> &[CompareManyFrame] {
        &self.group_compares.history
    }
    pub(crate) fn set_group_number(
        &mut self,
        event: &TriggerEvent,
        seat: SeatId,
        number: i32,
    ) -> Result<(), RuleError> {
        if !["compare", "compareFixing"].contains(&event.timing.as_str()) {
            return Err(RuleError::WrongEvent);
        }
        let frame = self
            .group_compares
            .live
            .get_mut(&event.subject)
            .ok_or(RuleError::WrongEvent)?;
        if seat == frame.group.initiator && frame.numbers[0].is_some() {
            frame.numbers[0] = Some(number);
        } else if frame.target == Some(seat) && frame.numbers[1].is_some() {
            frame.numbers[1] = Some(number);
        } else {
            return Err(RuleError::InvalidTarget);
        }
        Ok(())
    }
    pub(crate) fn force_group_winner(
        &mut self,
        event: &TriggerEvent,
        winner: Option<SeatId>,
    ) -> Result<(), RuleError> {
        if !["compare", "compareFixing"].contains(&event.timing.as_str()) {
            return Err(RuleError::WrongEvent);
        }
        let frame = self
            .group_compares
            .live
            .get(&event.subject)
            .ok_or(RuleError::WrongEvent)?;
        if winner.is_some_and(|seat| {
            if frame.mode == CompareMode::Pairwise {
                seat != frame.group.initiator && Some(seat) != frame.target
            } else {
                seat != frame.group.initiator && !frame.group.targets.contains(&seat)
            }
        }) {
            return Err(RuleError::InvalidTarget);
        }
        if let Some(winner) = winner {
            self.group_compares.forced.insert(event.subject, winner);
        } else {
            self.group_compares.forced.remove(&event.subject);
        }
        Ok(())
    }
    fn group_hook(&mut self, id: EventId, timing: &'static str) {
        let frame = &self.group_compares.live[&id];
        self.queue_timing(timing, frame.player, None, frame.target, 0, &[]);
    }
    fn group_frame(&mut self, id: EventId) -> &mut CompareManyFrame {
        self.group_compares
            .live
            .get_mut(&id)
            .expect("live comparison")
    }
    fn announce_group_reveal(
        &mut self,
        event: EventId,
        mode: CompareMode,
        seats: &[SeatId],
        cards: &[Card],
    ) {
        self.reveal_compare_bids(event, cards);
        self.group_updates.push(GroupAnnouncement::Reveal {
            comparison: event,
            mode,
            bids: seats
                .iter()
                .copied()
                .zip(cards.iter().copied())
                .map(|(seat, card)| RevealedBid { seat, card })
                .collect(),
        });
    }
    pub(crate) fn resolve_group_compare(
        &mut self,
        event: EventRef,
        mut flow: Flow,
        child: Option<Outcome<Resolution>>,
    ) {
        use Stage::*;
        let id = event.id;
        if !self.group_compares.live.contains_key(&id) {
            self.group_compares.live.insert(
                id,
                CompareManyFrame {
                    event: id,
                    parent: event.parent,
                    turn: self
                        .classic
                        .as_ref()
                        .and_then(|state| state.turn)
                        .map(|turn| turn.number),
                    mode: flow.mode,
                    group: CompareGroup {
                        initiator: flow.owner,
                        targets: flow.targets.clone(),
                    },
                    index: 0,
                    player: Some(flow.owner),
                    target: None,
                    cards: [None, None],
                    numbers: [None, None],
                    winner: None,
                    result: None,
                    cancelled: false,
                },
            );
        }
        if matches!(child, Some(Outcome::Cancelled))
            && !matches!(flow.stage, Paid | CancelFinish | CancelNotify)
        {
            self.group_frame(id).cancelled = true;
            flow.stage = CancelNotify;
        }
        if self.match_result().is_some()
            && !matches!(flow.stage, CancelNotify | CancelFinish | Finish)
        {
            self.group_frame(id).cancelled = true;
            flow.stage = CancelNotify;
        }
        match flow.stage {
            Before => {
                flow.stage = Begin;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "chooseToCompareBefore");
            }
            Begin => {
                flow.stage = Collect;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "chooseToCompareBegin");
            }
            Collect => {
                if std::iter::once(flow.owner)
                    .chain(flow.targets.iter().copied())
                    .any(|seat| {
                        !self
                            .seat(seat)
                            .is_ok_and(|player| player.alive && !player.hand.is_empty())
                    })
                {
                    self.group_frame(id).result = Some(CompareManyResult {
                        cards: Vec::new(),
                        numbers: Vec::new(),
                        winner: None,
                        selection_cancelled: true,
                    });
                    flow.stage = End;
                    self.scheduler.replace_current(Program::GroupCompare(flow));
                    return;
                }
                let order = self.seat_order_from(flow.owner).unwrap();
                flow.targets.sort_by_key(|seat| {
                    order.iter().position(|id| id == seat).unwrap_or(usize::MAX)
                });
                self.group_frame(id).group.targets = flow.targets.clone();
                let group = BidGroup::Group {
                    initiator: flow.owner,
                    targets: Arc::from(flow.targets.clone()),
                };
                let collect = self.collect_bids(group);
                flow.stage = Collected;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.scheduler.next(collect);
            }
            Collected => {
                let Some(Outcome::Finished(Resolution::LockedBids(cards))) = child else {
                    self.group_frame(id).cancelled = true;
                    flow.stage = CancelNotify;
                    self.scheduler.replace_current(Program::GroupCompare(flow));
                    return;
                };
                if cards.len() != flow.targets.len() + 1
                    || cards.iter().any(|card| self.face(card.id).is_none())
                {
                    self.group_frame(id).cancelled = true;
                    flow.stage = CancelNotify;
                    self.scheduler.replace_current(Program::GroupCompare(flow));
                    return;
                }
                flow.cards = cards;
                let rank = i32::from(self.face(flow.cards[0].id).unwrap().rank);
                let frame = self.group_frame(id);
                frame.cards = [Some(flow.cards[0]), None];
                frame.numbers = [Some(rank), None];
                frame.result = Some(CompareManyResult {
                    cards: flow.cards.clone(),
                    numbers: Vec::new(),
                    winner: None,
                    selection_cancelled: false,
                });
                let group = BidGroup::Group {
                    initiator: flow.owner,
                    targets: Arc::from(flow.targets.clone()),
                };
                let payment = self.pay_bids(group, flow.cards.clone());
                let cleanup = self.cleanup_bids(flow.cards.clone());
                flow.stage = Paid;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.scheduler.next(payment);
                self.scheduler.after(cleanup);
            }
            Paid => {
                flow.stage = Reveal;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "compareCardShowBefore");
            }
            Reveal => {
                if flow.mode == CompareMode::Simultaneous {
                    let seats: Vec<_> = std::iter::once(flow.owner)
                        .chain(flow.targets.iter().copied())
                        .collect();
                    self.announce_group_reveal(id, flow.mode, &seats, &flow.cards);
                }
                let show = self.show_bid(flow.owner, flow.cards[0], false);
                flow.stage = ShowOwner;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.scheduler.next(show);
            }
            ShowOwner => {
                flow.stage = if flow.mode == CompareMode::Pairwise {
                    PairStart
                } else {
                    SimShow
                };
                self.scheduler.replace_current(Program::GroupCompare(flow));
            }
            PairStart => {
                let target = flow.targets[flow.index];
                let cards = [flow.cards[0], flow.cards[flow.index + 1]];
                let rank = i32::from(self.face(cards[1].id).unwrap().rank);
                let frame = self.group_frame(id);
                frame.index = flow.index;
                frame.player = Some(flow.owner);
                frame.target = Some(target);
                frame.cards = [Some(cards[0]), Some(cards[1])];
                frame.numbers[1] = Some(rank);
                frame.winner = None;
                self.announce_group_reveal(id, flow.mode, &[flow.owner, target], &cards);
                let show = self.show_bid(target, cards[1], false);
                flow.stage = PairCompare;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.scheduler.next(show);
            }
            PairCompare => {
                flow.stage = PairFix;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "compare");
            }
            PairFix => {
                flow.stage = PairSettle;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "compareFixing");
            }
            PairSettle => {
                let frame = self.group_frame(id);
                let numbers = [frame.numbers[0].unwrap(), frame.numbers[1].unwrap()];
                let target = flow.targets[flow.index];
                let natural = if numbers[0] > numbers[1] {
                    Some(flow.owner)
                } else if numbers[0] < numbers[1] {
                    Some(target)
                } else {
                    None
                };
                let winner = self
                    .group_compares
                    .forced
                    .remove(&id)
                    .filter(|seat| *seat == flow.owner || *seat == target)
                    .or(natural);
                let frame = self.group_frame(id);
                frame.winner = winner;
                frame.result.as_mut().unwrap().numbers.push(numbers);
                self.group_updates.push(GroupAnnouncement::Pair {
                    comparison: id,
                    initiator: flow.owner,
                    target,
                    numbers,
                    winner,
                });
                flow.stage = PairNext;
                if let Some(callback) = flow.callback.clone().filter(|_| self.is_alive(flow.owner))
                {
                    let event = self.subject_timing(
                        "compareMultiple",
                        Some(flow.owner),
                        None,
                        Some(target),
                        0,
                        &[],
                    );
                    let owner = flow.owner;
                    self.scheduler.replace_current(Program::GroupCompare(flow));
                    self.scheduler
                        .next(Program::Registered(crate::triggers::Registered::pair(
                            owner, target, event, callback,
                        )));
                } else {
                    self.scheduler.replace_current(Program::GroupCompare(flow));
                }
            }
            PairNext => {
                self.group_frame(id).winner = None;
                flow.index += 1;
                flow.stage = if flow.index < flow.targets.len() {
                    PairStart
                } else {
                    End
                };
                self.scheduler.replace_current(Program::GroupCompare(flow));
            }
            SimShow => {
                if flow.index < flow.targets.len() {
                    let show =
                        self.show_bid(flow.targets[flow.index], flow.cards[flow.index + 1], false);
                    flow.index += 1;
                    self.scheduler.replace_current(Program::GroupCompare(flow));
                    self.scheduler.next(show);
                } else {
                    flow.index = 0;
                    flow.stage = SimOwnerCompare;
                    self.scheduler.replace_current(Program::GroupCompare(flow));
                }
            }
            SimOwnerCompare => {
                flow.stage = SimTarget;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "compare");
            }
            SimTarget => {
                if flow.index == 0 {
                    flow.best = Some(-1);
                    flow.winner = None;
                }
                let target = flow.targets[flow.index];
                let card = flow.cards[flow.index + 1];
                let rank = i32::from(self.face(card.id).unwrap().rank);
                let frame = self.group_frame(id);
                frame.index = flow.index;
                frame.player = None;
                frame.target = Some(target);
                frame.cards[1] = Some(card);
                frame.numbers[1] = Some(rank);
                frame.winner = flow.winner;
                flow.stage = SimTargetStore;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "compare");
            }
            SimTargetStore => {
                let frame = self.group_frame(id);
                let numbers = [frame.numbers[0].unwrap(), frame.numbers[1].unwrap()];
                frame.result.as_mut().unwrap().numbers.push(numbers);
                for (seat, number) in [
                    (flow.owner, numbers[0]),
                    (flow.targets[flow.index], numbers[1]),
                ] {
                    if number > flow.best.unwrap() {
                        flow.best = Some(number);
                        flow.winner = Some(seat);
                    } else if number == flow.best.unwrap() && flow.winner != Some(seat) {
                        flow.winner = None;
                    }
                }
                flow.index += 1;
                if flow.index < flow.targets.len() {
                    flow.stage = SimTarget;
                } else {
                    self.group_frame(id).winner = flow.winner;
                    flow.index = 0;
                    flow.stage = SimFixOwner;
                }
                self.scheduler.replace_current(Program::GroupCompare(flow));
            }
            SimFixOwner => {
                let frame = self.group_frame(id);
                frame.index = 0;
                frame.player = Some(flow.owner);
                flow.stage = SimFixTarget;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "compareFixing");
            }
            SimFixTarget => {
                let target = flow.targets[flow.index];
                let frame = self.group_frame(id);
                frame.index = flow.index;
                frame.player = None;
                frame.target = Some(target);
                frame.cards[1] = Some(flow.cards[flow.index + 1]);
                frame.numbers[1] = Some(frame.result.as_ref().unwrap().numbers[flow.index][1]);
                flow.stage = SimFixNext;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "compareFixing");
            }
            SimFixNext => {
                flow.index += 1;
                if flow.index < flow.targets.len() {
                    flow.stage = SimFixTarget;
                } else {
                    let winner = self.group_compares.forced.remove(&id).or(flow.winner);
                    let frame = self.group_frame(id);
                    frame.player = Some(flow.owner);
                    frame.result.as_mut().unwrap().winner = winner;
                    flow.stage = End;
                }
                self.scheduler.replace_current(Program::GroupCompare(flow));
            }
            End => {
                let frame = &self.group_compares.live[&id];
                if let Some(result) = frame
                    .result
                    .as_ref()
                    .filter(|result| !result.selection_cancelled)
                {
                    self.group_updates.push(GroupAnnouncement::Result {
                        comparison: id,
                        mode: flow.mode,
                        group: frame.group.clone(),
                        numbers: result.numbers.clone(),
                        winner: result.winner,
                    });
                }
                flow.stage = After;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "chooseToCompareEnd");
            }
            After => {
                flow.stage = Finish;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "chooseToCompareAfter");
            }
            Finish | CancelFinish => {
                let frame = self.group_compares.live.remove(&id).unwrap();
                let cancelled = frame.cancelled;
                self.group_compares.forced.remove(&id);
                self.group_compares.history.push(frame);
                self.scheduler
                    .end(Outcome::Finished(Resolution::ComparedMany {
                        event: id,
                        cancelled,
                    }));
            }
            CancelNotify => {
                flow.stage = CancelFinish;
                self.scheduler.replace_current(Program::GroupCompare(flow));
                self.group_hook(id, "chooseToCompareCancelled");
            }
        }
    }
}
