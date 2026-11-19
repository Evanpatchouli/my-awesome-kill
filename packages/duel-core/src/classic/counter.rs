use super::equipment;
use crate::scheduler::Outcome;
use crate::{CounterContext, EventKind, Game, Program, Resolution, SeatId};

/// One complete counterspell chain for one affected target.
#[derive(Clone)]
pub(crate) struct Flow {
    context: CounterContext,
    actor: Option<SeatId>,
    passed: Vec<SeatId>,
    step: Step,
}

#[derive(Clone, Copy)]
enum Step {
    Next,
    Choice,
    Payment,
}

impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        EventKind::CounterWindow {
            seat: self.context.target,
            kind: self.context.card_kind,
        }
    }
}

impl Game {
    pub(crate) fn counter_window(&self, context: CounterContext) -> Program {
        Program::Counter(Flow {
            context,
            actor: None,
            passed: Vec::new(),
            step: Step::Next,
        })
    }

    pub(crate) fn resolve_counter_window(
        &mut self,
        mut flow: Flow,
        child: Option<Outcome<Resolution>>,
    ) {
        if self.match_result().is_some() {
            self.scheduler.end(Outcome::Cancelled);
            return;
        }
        match flow.step {
            Step::Choice => {
                if let Some(Outcome::Finished(Resolution::CounterUse { seat, cards })) = child {
                    if self.is_in(seat) && self.is_in(flow.context.target) {
                        flow.step = Step::Payment;
                        self.scheduler.replace_current(Program::Counter(flow));
                        self.scheduler.next(self.counter_payment(seat, cards));
                        return;
                    }
                }
                flow.passed
                    .push(flow.actor.expect("the responder was selected"));
            }
            Step::Payment => {
                if matches!(child, Some(Outcome::Finished(_))) {
                    flow.passed.clear();
                    flow.context.negated = !flow.context.negated;
                } else {
                    flow.passed
                        .push(flow.actor.expect("the responder was selected"));
                }
            }
            Step::Next => {}
        }
        // Resume from the last responder's seat even if that responder left play.
        let mut order = self.in_order_from(flow.actor.unwrap_or(self.active_seat));
        if !order.is_empty() && order.first().copied() == flow.actor {
            order.rotate_left(1);
        }
        let actor = order.into_iter().find(|seat| !flow.passed.contains(seat));
        let Some(actor) = actor.filter(|_| self.is_in(flow.context.target)) else {
            self.scheduler.end(Outcome::Finished(Resolution::Countered(
                flow.context.negated,
            )));
            return;
        };
        flow.actor = Some(actor);
        let context = flow.context;
        flow.step = Step::Choice;
        self.scheduler.replace_current(Program::Counter(flow));
        self.scheduler
            .next(Program::Equipment(equipment::Flow::counter_response(
                actor, context,
            )));
    }
}

#[cfg(test)]
mod tests;
