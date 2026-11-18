use super::equipment;
use crate::scheduler::Outcome;
use crate::{CounterContext, EventKind, Game, Program, Resolution};

/// One complete counterspell chain for one affected target.
#[derive(Clone)]
pub(crate) struct Flow {
    context: CounterContext,
    index: usize,
    passed: usize,
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
            index: 0,
            passed: 0,
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
                    flow.step = Step::Payment;
                    self.scheduler.replace_current(Program::Counter(flow));
                    self.scheduler.next(self.counter_payment(seat, cards));
                    return;
                }
                flow.index += 1;
                flow.passed += 1;
            }
            Step::Payment => {
                let responded = matches!(child, Some(Outcome::Finished(_)));
                flow.index += 1;
                flow.passed = if responded { 0 } else { flow.passed + 1 };
                if responded {
                    flow.context.negated = !flow.context.negated;
                }
            }
            Step::Next => {}
        }
        let order = self.living_order_from(self.active_seat);
        if order.is_empty() || flow.passed >= order.len() || !self.is_alive(flow.context.target) {
            self.scheduler.end(Outcome::Finished(Resolution::Countered(
                flow.context.negated,
            )));
            return;
        }
        let actor = order[flow.index % order.len()];
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
