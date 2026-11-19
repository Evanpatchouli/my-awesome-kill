use crate::scheduler::Outcome;
use crate::{CardKind, CardType, EventKind, Game, Program, Resolution, SeatId};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
enum Stage {
    Before,
    Begin,
    Apply,
    Publish,
    End,
    After,
    Cancel,
    Finish,
    FinishCancelled,
}

#[derive(Clone)]
pub(crate) struct Flow {
    seat: SeatId,
    source: Option<SeatId>,
    disable: bool,
    stage: Stage,
}

impl Flow {
    pub(crate) fn new(seat: SeatId, source: Option<SeatId>, disable: bool) -> Self {
        Self {
            seat,
            source,
            disable,
            stage: Stage::Before,
        }
    }

    pub(crate) fn kind(&self) -> EventKind {
        let Self { seat, source, .. } = *self;
        if self.disable {
            EventKind::DisableJudge { seat, source }
        } else {
            EventKind::EnableJudge { seat, source }
        }
    }
}

impl Game {
    /// Whether a seat's judgment area currently rejects new delayed tricks.
    pub fn is_judge_disabled(&self, seat: SeatId) -> bool {
        self.seat(seat).is_ok_and(|player| player.judge_disabled)
    }

    /// Check the area itself. Card-specific targeting restrictions also apply to use.
    pub fn can_add_judge(&self, seat: SeatId, kind: CardKind) -> bool {
        kind.card_type() == CardType::Delay
            && self
                .seat(seat)
                .is_ok_and(|player| !player.out && !player.judge_disabled)
            && !self.judgments(seat).any(|entry| entry.kind == kind)
    }

    pub(crate) fn resolve_judge_area(
        &mut self,
        mut flow: Flow,
        child: Option<Outcome<Resolution>>,
    ) {
        use Stage::*;
        // The disable operation completes even when its discard child is cancelled.
        if matches!(child, Some(Outcome::Cancelled))
            && !matches!(flow.stage, Publish | FinishCancelled)
        {
            flow.stage = Cancel;
        }

        loop {
            let (disable, enable, next) = match flow.stage {
                Before => ("disableJudgeBefore", "enableJudgeBefore", Begin),
                Begin => ("disableJudgeBegin", "enableJudgeBegin", Apply),
                Apply => {
                    if !self.is_in(flow.seat) {
                        flow.stage = End;
                        continue;
                    }
                    let player = self.seat_mut(flow.seat);
                    if flow.disable {
                        player.judge_disabled = true;
                        flow.stage = Publish;
                        let entries = self.judgments(flow.seat).rev().collect::<Vec<_>>();
                        if !entries.is_empty() {
                            self.scheduler
                                .next(self.judgment_discard_program(flow.seat, entries));
                            self.scheduler.replace_current(Program::JudgeArea(flow));
                            return;
                        }
                    } else {
                        if player.judge_disabled {
                            player.judge_disabled = false;
                            player.judge_disabled_marker = false;
                        }
                        flow.stage = End;
                    }
                    continue;
                }
                Publish => {
                    let player = self.seat_mut(flow.seat);
                    player.judge_disabled = true;
                    player.judge_disabled_marker = true;
                    flow.stage = End;
                    continue;
                }
                End => ("disableJudgeEnd", "enableJudgeEnd", After),
                After => ("disableJudgeAfter", "enableJudgeAfter", Finish),
                Cancel => (
                    "disableJudgeCancelled",
                    "enableJudgeCancelled",
                    FinishCancelled,
                ),
                Finish => {
                    self.finish();
                    return;
                }
                FinishCancelled => {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
            };
            flow.stage = next;
            if self.queue_timing(
                if flow.disable { disable } else { enable },
                Some(flow.seat),
                flow.source,
                None,
                0,
                &[],
            ) {
                self.scheduler.replace_current(Program::JudgeArea(flow));
                return;
            }
        }
    }
}
