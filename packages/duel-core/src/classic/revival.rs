//! Immediate and event-based revival share the same state transition.
use crate::scheduler::Outcome;
use crate::{EventKind, Game, Program, Resolution, RuleError, SeatId};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
enum Stage {
    Before,
    Begin,
    Apply,
    End,
    After,
    Cancel,
    Finish,
}

#[derive(Clone)]
pub(crate) struct Flow {
    seat: SeatId,
    hp: i32,
    stage: Stage,
    awaiting_cancellable: bool,
    cancelled: bool,
}

impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        EventKind::Revive {
            seat: self.seat,
            hp: self.hp,
        }
    }
}

impl Game {
    pub(crate) fn revive_now(&mut self, seat: SeatId, hp: Option<i32>) -> Result<(), RuleError> {
        self.seat(seat)?;
        let player = self.seat_mut(seat);
        let was_dead = !player.alive;
        player.max_hp = player.max_hp.max(1);
        player.hp = hp.filter(|&n| n != 0).unwrap_or(1).min(player.max_hp);
        player.alive = true;
        if was_dead {
            self.dead_seats.retain(|&id| id != seat);
            self.living_seats.push(seat);
        }
        Ok(())
    }

    pub(crate) fn revive_program(&self, seat: SeatId, hp: Option<i32>) -> Program {
        Program::Revive(Flow {
            seat,
            hp: hp.filter(|&n| n != 0).unwrap_or(1),
            stage: Stage::Before,
            awaiting_cancellable: false,
            cancelled: false,
        })
    }

    pub(crate) fn resolve_revive(&mut self, mut flow: Flow, child: Option<Outcome<Resolution>>) {
        if flow.awaiting_cancellable && matches!(child, Some(Outcome::Cancelled)) {
            flow.cancelled = true;
            flow.stage = Stage::Cancel;
        }
        flow.awaiting_cancellable = false;
        loop {
            let (timing, cancellable) = match flow.stage {
                Stage::Before => {
                    flow.stage = Stage::Begin;
                    ("reviveBefore", true)
                }
                Stage::Begin => {
                    flow.stage = Stage::Apply;
                    ("reviveBegin", true)
                }
                Stage::Apply => {
                    self.revive_now(flow.seat, Some(flow.hp))
                        .expect("revive seat was validated");
                    flow.stage = Stage::End;
                    continue;
                }
                Stage::End => {
                    flow.stage = Stage::After;
                    ("reviveEnd", false)
                }
                Stage::After => {
                    flow.stage = Stage::Finish;
                    ("reviveAfter", false)
                }
                Stage::Cancel => {
                    flow.stage = Stage::Finish;
                    ("reviveCancelled", false)
                }
                Stage::Finish => {
                    let cancelled = flow.cancelled;
                    self.scheduler.replace_current(Program::Revive(flow));
                    self.scheduler.end(Outcome::Finished(if cancelled {
                        Resolution::ReviveCancelled
                    } else {
                        Resolution::Complete
                    }));
                    return;
                }
            };
            if self.wants_timing(timing) {
                let seat = flow.seat;
                flow.awaiting_cancellable = cancellable;
                self.scheduler.replace_current(Program::Revive(flow));
                self.queue_timing(timing, Some(seat), None, None, 0, &[]);
                return;
            }
        }
    }
}
