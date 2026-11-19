use super::skills::HeroMoment;
use crate::scheduler::{EventRef, Outcome};
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
    cancelling: bool,
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
            Self::UseCardToExcluded => "useCardToExcluded",
        }
    }
}

impl Game {
    pub(crate) fn queue_card_target_excluded(
        &mut self,
        seat: SeatId,
        target: SeatId,
        cards: &[Card],
    ) {
        self.scheduler.next(Program::TargetTiming(TargetTiming {
            timing: CardTargetTiming::UseCardToExcluded,
            seat,
            target,
            card_kind: CardKind::Sha,
            cards: cards.to_vec(),
            started: false,
            cancelling: false,
        }));
    }
    /// Return false without allocating a frame when nobody observes this timing.
    pub(crate) fn queue_card_target_timing(
        &mut self,
        timing: CardTargetTiming,
        seat: SeatId,
        target: SeatId,
        card_kind: CardKind,
        cards: &[Card],
    ) -> bool {
        let native_sha = timing == CardTargetTiming::UseCardToPlayered
            && card_kind == CardKind::Sha
            && self.has_hero_triggers(&HeroMoment::ShaTarget {
                seat,
                target,
                attack: self.scheduler.current_id(),
            });
        if (!self.wants_timing(timing.name()) && !native_sha) || !self.is_in(target) {
            return false;
        }
        self.scheduler.next(Program::TargetTiming(TargetTiming {
            timing,
            seat,
            target,
            card_kind,
            cards: cards.to_vec(),
            started: false,
            cancelling: false,
        }));
        true
    }

    pub(crate) fn resolve_card_target_timing(
        &mut self,
        frame: EventRef,
        mut flow: TargetTiming,
        child: Option<Outcome<Resolution>>,
    ) {
        if flow.started {
            if flow.cancelling {
                self.scheduler.end(Outcome::Cancelled);
            } else if matches!(child, Some(Outcome::Cancelled)) {
                let cancelled_timing = match flow.timing {
                    CardTargetTiming::UseCardToPlayered => Some("useCardToPlayeredCancelled"),
                    CardTargetTiming::UseCardToExcluded => Some("useCardToExcludedCancelled"),
                    _ => None,
                };
                if let Some(timing) = cancelled_timing {
                    flow.cancelling = true;
                    let queued = self.queue_timing(
                        timing,
                        Some(flow.seat),
                        None,
                        Some(flow.target),
                        0,
                        &flow.cards,
                    );
                    if queued {
                        self.scheduler.replace_current(Program::TargetTiming(flow));
                    } else {
                        self.scheduler.end(Outcome::Cancelled);
                    }
                } else {
                    self.scheduler.end(Outcome::Cancelled);
                }
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
        if flow.timing == CardTargetTiming::UseCardToExcluded {
            let Some(parent) = frame.parent else {
                self.finish();
                return;
            };
            if let Some(queue) = self.prepare_qinggang_timing(
                "useCardToExcluded",
                Some(flow.seat),
                None,
                Some(flow.target),
                0,
                &flow.cards,
                Some(parent),
                Some(&[flow.target]),
            ) {
                flow.started = true;
                self.scheduler.replace_current(Program::TargetTiming(flow));
                self.scheduler.next(Program::Dispatch(queue));
            } else {
                self.finish();
            }
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
        let moment = if flow.timing == CardTargetTiming::UseCardToPlayered
            && flow.card_kind == CardKind::Sha
        {
            Some(HeroMoment::ShaTarget {
                seat: flow.seat,
                target: flow.target,
                attack: frame
                    .parent
                    .expect("Sha target timing has an attack parent"),
            })
        } else {
            None
        };
        let registered = self.begin_timing_request(&event);
        if registered || moment.as_ref().is_some_and(|m| self.has_hero_triggers(m)) {
            flow.started = true;
            self.scheduler.replace_current(Program::TargetTiming(flow));
            let queue = if let Some(moment) = moment {
                self.dispatch_hero_timing(event, moment)
            } else {
                self.dispatch_for_timing(event)
            };
            self.scheduler.next(Program::Dispatch(queue));
        } else {
            self.finish();
        }
    }
}

#[cfg(test)]
#[path = "target_timing_tests.rs"]
mod tests;
