use crate::scheduler::{EventRef, Outcome};
use crate::{EventId, EventKind, Game, Program, Resolution, RuleError, SeatId};

#[cfg(test)]
mod regressions;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaxHpKind {
    Gain,
    Lose,
}

#[derive(Clone, Debug)]
pub struct MaxHpMetrics {
    pub original_hp: i32,
    pub original_max_hp: i32,
    pub changed_hp: i32,
    pub changed_max_hp: i64,
    pub lose_hp: Option<i64>,
    pub max_hp_after: i32,
}

#[derive(Clone, Debug)]
pub struct MaxHpChange {
    pub event: EventId,
    pub seat: SeatId,
    pub source: Option<SeatId>,
    pub kind: MaxHpKind,
    pub num: i32,
    pub metrics: Option<MaxHpMetrics>,
    pub cancelled: bool,
}

#[derive(Clone, Copy)]
enum Stage {
    Before,
    Begin,
    Apply,
    Death,
    End,
    After,
    Cancel,
    Finish,
}

#[derive(Clone)]
pub(crate) struct Flow {
    change: MaxHpChange,
    stage: Stage,
    awaiting_timing: bool,
}

impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        let change = &self.change;
        let before = change.metrics.as_ref().map(|m| m.original_max_hp);
        let after = change.metrics.as_ref().map(|m| m.max_hp_after);
        match change.kind {
            MaxHpKind::Gain => EventKind::GainMaxHp {
                seat: change.seat,
                source: change.source,
                num: change.num,
                before,
                after,
            },
            MaxHpKind::Lose => EventKind::LoseMaxHp {
                seat: change.seat,
                source: change.source,
                num: change.num,
                before,
                after,
            },
        }
    }
}

impl MaxHpKind {
    fn value_after(self, current: i32, num: i32) -> Option<i32> {
        match self {
            Self::Gain => current.checked_add(num),
            Self::Lose => current.checked_sub(num),
        }
    }

    fn timing(self, stage: Stage) -> &'static str {
        let names = match self {
            Self::Gain => [
                "gainMaxHpBefore",
                "gainMaxHpBegin",
                "gainMaxHpEnd",
                "gainMaxHpAfter",
                "gainMaxHpCancelled",
            ],
            Self::Lose => [
                "loseMaxHpBefore",
                "loseMaxHpBegin",
                "loseMaxHpEnd",
                "loseMaxHpAfter",
                "loseMaxHpCancelled",
            ],
        };
        names[match stage {
            Stage::Before => 0,
            Stage::Begin => 1,
            Stage::End => 2,
            Stage::After => 3,
            Stage::Cancel => 4,
            _ => unreachable!("only lifecycle stages have timing names"),
        }]
    }
}

impl Game {
    pub(crate) fn max_hp_program(
        &self,
        seat: SeatId,
        source: Option<SeatId>,
        kind: MaxHpKind,
        num: i32,
    ) -> Result<Program, RuleError> {
        let player = self.seat(seat)?;
        if let Some(source) = source {
            self.seat(source)?;
        }
        if !player.alive {
            return Err(RuleError::InactiveSeat { seat });
        }
        if self.match_result().is_some() {
            return Err(RuleError::GameOver);
        }
        kind.value_after(player.max_hp, num)
            .ok_or(RuleError::InvalidSelection)?;
        Ok(Program::MaxHp(Flow {
            change: MaxHpChange {
                event: 0,
                seat,
                source,
                kind,
                num,
                metrics: None,
                cancelled: false,
            },
            stage: Stage::Before,
            awaiting_timing: false,
        }))
    }

    pub fn max_hp_change(&self, event: EventId) -> Option<&MaxHpChange> {
        match self.scheduler.active_frame(event)? {
            (_, Program::MaxHp(flow)) => Some(&flow.change),
            _ => None,
        }
    }

    pub(crate) fn resolve_max_hp(
        &mut self,
        event: EventRef,
        mut flow: Flow,
        child: Option<Outcome<Resolution>>,
    ) {
        flow.change.event = event.id;
        if flow.awaiting_timing
            && matches!(child, Some(Outcome::Cancelled))
            && !flow.change.cancelled
        {
            flow.change.cancelled = true;
            flow.stage = Stage::Cancel;
        }
        flow.awaiting_timing = false;
        loop {
            let stage = flow.stage;
            match stage {
                Stage::Before => flow.stage = Stage::Begin,
                Stage::Begin => flow.stage = Stage::Apply,
                Stage::Apply => {
                    let seat = flow.change.seat;
                    let next = self
                        .seat(seat)
                        .ok()
                        .filter(|player| player.alive && self.match_result().is_none())
                        .and_then(|player| {
                            flow.change.kind.value_after(player.max_hp, flow.change.num)
                        });
                    let Some(after) = next else {
                        flow.change.cancelled = true;
                        flow.stage = Stage::Cancel;
                        continue;
                    };
                    let player = self.seat_mut(seat);
                    let original_hp = player.hp.max(0);
                    let original_max_hp = player.max_hp;
                    let raw_hp = player.hp;
                    player.max_hp = after;
                    player.hp = raw_hp.min(after);
                    let changed_hp = match flow.change.kind {
                        MaxHpKind::Gain => 0,
                        MaxHpKind::Lose => player.hp.max(0) - original_hp,
                    };
                    let lose_hp = match flow.change.kind {
                        MaxHpKind::Gain => None,
                        MaxHpKind::Lose => Some((i64::from(raw_hp) - i64::from(after)).max(0)),
                    };
                    flow.change.metrics = Some(MaxHpMetrics {
                        original_hp,
                        original_max_hp,
                        changed_hp,
                        changed_max_hp: i64::from(after) - i64::from(original_max_hp),
                        lose_hp,
                        max_hp_after: after,
                    });
                    flow.stage = if flow.change.kind == MaxHpKind::Lose && after <= 0 {
                        Stage::Death
                    } else {
                        Stage::End
                    };
                    continue;
                }
                Stage::Death => {
                    flow.stage = Stage::End;
                    let death = self.death_program(flow.change.seat, flow.change.source);
                    self.scheduler.replace_current(Program::MaxHp(flow));
                    self.scheduler.next(death);
                    return;
                }
                Stage::End => flow.stage = Stage::After,
                Stage::After | Stage::Cancel => flow.stage = Stage::Finish,
                Stage::Finish => {
                    let cancelled = flow.change.cancelled;
                    self.scheduler.replace_current(Program::MaxHp(flow));
                    self.scheduler
                        .end(Outcome::Finished(Resolution::MaxHpChanged { cancelled }));
                    return;
                }
            };
            let timing = flow.change.kind.timing(stage);
            if self.wants_timing(timing) {
                let seat = flow.change.seat;
                let source = flow.change.source;
                // Publish the live values before the request and its callbacks can read them.
                flow.awaiting_timing = true;
                self.scheduler.replace_current(Program::MaxHp(flow));
                self.queue_timing(timing, Some(seat), source, None, 0, &[]);
                return;
            }
        }
    }
}
