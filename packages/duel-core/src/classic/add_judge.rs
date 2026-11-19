use crate::scheduler::Outcome;
use crate::{Card, CardId, CardKind, EventKind, Game, Program, Resolution, SeatId};

#[derive(Clone, Copy)]
enum Stage {
    Before,
    Begin,
    Apply,
    AfterLoss,
    End,
    After,
    Cancel,
    Finish,
    FinishCancelled,
}

#[derive(Clone, Copy)]
pub(crate) struct Flow {
    seat: SeatId,
    material: Card,
    kind: CardKind,
    stage: Stage,
}

impl Flow {
    pub(crate) fn new(seat: SeatId, material: Card, kind: CardKind) -> Self {
        Self {
            seat,
            material,
            kind,
            stage: Stage::Before,
        }
    }

    pub(crate) fn kind(self) -> EventKind {
        EventKind::AddJudge { seat: self.seat }
    }

    pub(crate) fn card_kind(self) -> CardKind {
        self.kind
    }
}

/// The ordinary use's effect chooses whether to create an addJudge child.
#[derive(Clone, Copy)]
pub(crate) struct DelayedPlacement {
    pub(crate) source: SeatId,
    pub(crate) target: SeatId,
    pub(crate) material: Card,
    pub(crate) kind: CardKind,
}

impl DelayedPlacement {
    pub(crate) fn kind(self) -> EventKind {
        EventKind::TrickEffect {
            seat: self.source,
            target: self.target,
            kind: self.kind,
        }
    }
}

impl Game {
    pub(crate) fn add_judge_material(&self, id: CardId) -> Option<Card> {
        self.held_card_location(id)
            .map(|(_, _, card)| card)
            .or_else(|| {
                self.resolving
                    .iter()
                    .chain(&self.transferring)
                    .chain(&self.discard)
                    .chain(self.classic.iter().flat_map(|table| &table.deck))
                    .find(|card| card.id == id)
                    .copied()
            })
    }

    pub(crate) fn resolve_delayed_placement(&mut self, placement: DelayedPlacement) {
        if self.resolving.contains(&placement.material)
            && self.is_in(placement.target)
            && self.can_add_judge(placement.target, placement.kind)
        {
            self.scheduler.next(Program::AddJudge(Flow::new(
                placement.target,
                placement.material,
                placement.kind,
            )));
        }
        self.finish();
    }

    pub(crate) fn resolve_add_judge(&mut self, mut flow: Flow, child: Option<Outcome<Resolution>>) {
        use Stage::*;

        // A cancelled loss leaves its own state settled. The placement follows the
        // material's actual location, including an earlier move to the discard pile.
        if matches!(child, Some(Outcome::Cancelled))
            && !matches!(flow.stage, AfterLoss | FinishCancelled)
        {
            flow.stage = Cancel;
        }
        loop {
            let (timing, next) = match flow.stage {
                Before => ("addJudgeBefore", Begin),
                Begin => ("addJudgeBegin", Apply),
                Apply => {
                    if !self.is_in(flow.seat) {
                        flow.stage = End;
                        continue;
                    }
                    flow.stage = AfterLoss;
                    if let Some((owner, zone, card)) = self.held_card_location(flow.material.id) {
                        self.known_to_all.insert(card.id);
                        self.scheduler
                            .next(self.lose_to_judge_special(owner, zone, card));
                        self.scheduler.replace_current(Program::AddJudge(flow));
                        return;
                    }
                    continue;
                }
                AfterLoss => {
                    // canAddJudge was the caller's check. The addJudge body can
                    // place into an area disabled while that child was pending.
                    if let Some(card) = self.take_unowned_card(flow.material) {
                        self.put_in_judgment(flow.seat, card, flow.kind);
                    }
                    flow.stage = End;
                    continue;
                }
                End => ("addJudgeEnd", After),
                After => ("addJudgeAfter", Finish),
                Cancel => ("addJudgeCancelled", FinishCancelled),
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
            if self.queue_timing(timing, Some(flow.seat), None, None, 1, &[flow.material]) {
                self.scheduler.replace_current(Program::AddJudge(flow));
                return;
            }
        }
    }
}
