use crate::scheduler::Outcome;
use crate::{Card, CardKind, CardTargetTiming, EventKind, Game, Program, Resolution, SeatId};

/// One target timing is a child of the use, with its own cancellation boundary.
#[derive(Clone)]
pub(crate) struct TargetTiming {
    pub(crate) timing: CardTargetTiming,
    pub(crate) seat: SeatId,
    pub(crate) target: SeatId,
    pub(crate) card_kind: CardKind,
    pub(crate) cards: Vec<Card>,
    started: bool,
}

impl TargetTiming {
    pub(crate) fn kind(&self) -> EventKind {
        EventKind::CardTargetTiming {
            timing: self.timing,
            seat: self.seat,
            target: self.target,
            card_kind: self.card_kind,
        }
    }
}

impl CardTargetTiming {
    pub(crate) const ALL: [Self; 4] = [
        Self::UseCardToPlayer,
        Self::UseCardToTarget,
        Self::UseCardToPlayered,
        Self::UseCardToTargeted,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::UseCardToPlayer => "useCardToPlayer",
            Self::UseCardToTarget => "useCardToTarget",
            Self::UseCardToPlayered => "useCardToPlayered",
            Self::UseCardToTargeted => "useCardToTargeted",
        }
    }
}

impl Game {
    /// Return false without allocating a frame when nobody observes this timing.
    pub(crate) fn queue_card_target_timing(
        &mut self,
        timing: CardTargetTiming,
        seat: SeatId,
        target: SeatId,
        card_kind: CardKind,
        cards: &[Card],
    ) -> bool {
        if !self.wants_timing(timing.name()) || !self.is_in(target) {
            return false;
        }
        self.scheduler.next(Program::TargetTiming(TargetTiming {
            timing,
            seat,
            target,
            card_kind,
            cards: cards.to_vec(),
            started: false,
        }));
        true
    }

    pub(crate) fn resolve_card_target_timing(
        &mut self,
        mut flow: TargetTiming,
        child: Option<Outcome<Resolution>>,
    ) {
        if flow.started {
            if matches!(child, Some(Outcome::Cancelled)) {
                self.scheduler.end(Outcome::Cancelled);
            } else {
                self.finish();
            }
            return;
        }
        // A preceding target's callback can remove this target before its turn.
        if !self.is_in(flow.target) {
            self.finish();
            return;
        }
        let mut event = self.subject_timing(
            flow.timing.name(),
            Some(flow.seat),
            None,
            Some(flow.target),
            0,
            &flow.cards,
        );
        event.card_kind = Some(flow.card_kind);
        if self.begin_timing_request(&event) {
            flow.started = true;
            self.scheduler.replace_current(Program::TargetTiming(flow));
            self.scheduler
                .next(Program::Dispatch(self.dispatch_for_timing(event)));
        } else {
            self.finish();
        }
    }
}

#[cfg(test)]
#[path = "target_timing_tests.rs"]
mod tests;
