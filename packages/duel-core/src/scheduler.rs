use std::collections::VecDeque;

use crate::EventId;

#[derive(Clone, Copy)]
pub(crate) struct EventRef {
    pub id: EventId,
    pub parent: Option<EventId>,
}

#[derive(Clone, Copy)]
pub(crate) enum Outcome<R> {
    Finished(R),
    Cancelled,
}

pub(crate) enum Work<P, R> {
    Started(EventRef, P),
    Run(EventRef, P, Option<Outcome<R>>),
    Ended(EventRef, P, Outcome<R>),
}

#[derive(Clone, Copy)]
enum State<R> {
    Ready,
    Running,
    Ending(Outcome<R>),
    After(Outcome<R>),
}

struct Frame<P, R> {
    event: EventRef,
    program: P,
    state: State<R>,
    child_result: Option<Outcome<R>>,
    next: VecDeque<P>,
    after: VecDeque<P>,
}

pub(crate) struct Scheduler<P, R> {
    stack: Vec<Frame<P, R>>,
    last_id: EventId,
}

impl<P: Copy, R: Copy> Scheduler<P, R> {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            last_id: 0,
        }
    }

    pub fn start(&mut self, program: P) {
        assert!(self.stack.is_empty());
        self.push(program, None);
    }

    pub fn next(&mut self, program: P) {
        let frame = self.current();
        assert!(matches!(frame.state, State::Running));
        frame.next.push_back(program);
    }

    pub fn after(&mut self, program: P) {
        let frame = self.current();
        assert!(matches!(frame.state, State::Running));
        frame.after.push_back(program);
    }

    pub fn end(&mut self, outcome: Outcome<R>) {
        let frame = self.current();
        assert!(matches!(frame.state, State::Running));
        frame.state = State::Ending(outcome);
    }

    pub fn poll(&mut self) -> Option<Work<P, R>> {
        loop {
            let frame = self.stack.last_mut()?;
            let event = frame.event;
            if matches!(frame.state, State::Ready) {
                frame.state = State::Running;
                return Some(Work::Started(event, frame.program));
            }
            // Queued children still run when the parent requests its own end.
            if let Some(next) = frame.next.pop_front() {
                self.push(next, Some(event.id));
                continue;
            }
            let outcome = match frame.state {
                State::Ready => unreachable!(),
                State::Running => {
                    return Some(Work::Run(event, frame.program, frame.child_result.take()))
                }
                State::Ending(outcome) => {
                    frame.state = State::After(outcome);
                    return Some(Work::Ended(event, frame.program, outcome));
                }
                State::After(outcome) => outcome,
            };
            if let Some(after) = frame.after.pop_front() {
                self.push(after, Some(event.id));
                continue;
            }
            self.stack.pop();
            if let Some(parent) = self.stack.last_mut() {
                // Cleanup never replaces the result the parent already announced.
                if matches!(parent.state, State::Running) {
                    parent.child_result = Some(outcome);
                }
            }
        }
    }

    fn current(&mut self) -> &mut Frame<P, R> {
        self.stack.last_mut().expect("an event is running")
    }

    fn push(&mut self, program: P, parent: Option<EventId>) {
        self.last_id += 1;
        self.stack.push(Frame {
            event: EventRef {
                id: self.last_id,
                parent,
            },
            program,
            state: State::Ready,
            child_result: None,
            next: VecDeque::new(),
            after: VecDeque::new(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{Outcome, Scheduler, Work};

    #[test]
    fn cancelled_parent_drains_existing_children_then_runs_fifo_cleanup() {
        let mut scheduler = Scheduler::<_, ()>::new();
        scheduler.start("parent");
        let mut lifecycle = Vec::new();
        while let Some(work) = scheduler.poll() {
            match work {
                Work::Started(event, program) => {
                    lifecycle.push((program, event.parent, "start"));
                }
                Work::Run(_, "parent", None) => {
                    scheduler.next("first");
                    scheduler.next("second");
                    scheduler.after("cleanup first");
                    scheduler.after("cleanup second");
                    scheduler.end(Outcome::Cancelled);
                }
                Work::Run(_, _, None) => scheduler.end(Outcome::Finished(())),
                Work::Run(_, _, Some(_)) => panic!("no event resumes after it ends"),
                Work::Ended(event, program, Outcome::Finished(())) => {
                    lifecycle.push((program, event.parent, "finish"));
                }
                Work::Ended(event, program, Outcome::Cancelled) => {
                    lifecycle.push((program, event.parent, "cancel"));
                }
            }
        }

        assert_eq!(
            lifecycle,
            [
                ("parent", None, "start"),
                ("first", Some(1), "start"),
                ("first", Some(1), "finish"),
                ("second", Some(1), "start"),
                ("second", Some(1), "finish"),
                ("parent", None, "cancel"),
                ("cleanup first", Some(1), "start"),
                ("cleanup first", Some(1), "finish"),
                ("cleanup second", Some(1), "start"),
                ("cleanup second", Some(1), "finish"),
            ]
        );
    }
}
