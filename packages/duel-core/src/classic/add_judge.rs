use crate::scheduler::Outcome;
use crate::{Card, CardId, CardKind, EventKind, Game, Program, Resolution, SeatId, Zone};

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
    fn find_held_judge_material(&self, id: CardId) -> Option<(SeatId, Zone, Card)> {
        self.seats.iter().find_map(|player| {
            [Zone::Hand, Zone::Equipment, Zone::Judgment]
                .into_iter()
                .find_map(|zone| {
                    player
                        .zone(zone)
                        .iter()
                        .find(|card| card.id == id)
                        .map(|&card| (player.id, zone, card))
                })
        })
    }

    pub(crate) fn add_judge_material(&self, id: CardId) -> Option<Card> {
        self.find_held_judge_material(id)
            .map(|(_, _, card)| card)
            .or_else(|| self.resolving.iter().find(|card| card.id == id).copied())
    }

    pub(crate) fn resolve_delayed_placement(&mut self, placement: DelayedPlacement) {
        if self.resolving.contains(&placement.material)
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

    fn take_loose_judge_material(&mut self, card: Card) -> Option<Card> {
        if self.resolving.contains(&card) {
            return Some(self.take_resolving(card.id));
        }
        [&mut self.transferring, &mut self.discard]
            .into_iter()
            .find(|pile| pile.contains(&card))
            .map(|pile| Self::remove_card(pile, card.id))
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
                    if let Some((owner, zone, card)) =
                        self.find_held_judge_material(flow.material.id)
                    {
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
                    if let Some(card) = self.take_loose_judge_material(flow.material) {
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
