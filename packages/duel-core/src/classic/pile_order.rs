//! Private ordering and pile placement for ordered deck previews.
use std::sync::Arc;

use crate::move_choice::{MoveChoiceRequest, MoveRow, MoveStep};
use crate::scheduler::{EventRef, Outcome};
use crate::{Card, EventId, EventKind, Game, Program, Resolution, SeatId};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PileOrderResult {
    pub event: EventId,
    pub rows: [Vec<Card>; 2],
    pub cancelled: bool,
}

#[derive(Clone, Copy)]
enum RootStage {
    Before,
    Begin,
    Draw,
    Ordered,
    Chosen,
    Piled,
    End,
    After,
    Finish,
    CancelNotify,
    CancelFinish,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum MoveStage {
    Before,
    Begin,
    Move,
    End,
    After,
    Finish,
    CancelFinish,
}
#[derive(Clone, Copy)]
enum MoveKind {
    Ordering,
    Pile,
    Discard,
}

impl MoveKind {
    fn hook(self, stage: MoveStage) -> &'static str {
        let timings = match self {
            Self::Ordering => [
                "cardsGotoOrderingBefore",
                "cardsGotoOrderingBegin",
                "cardsGotoOrderingEnd",
                "cardsGotoOrderingAfter",
                "cardsGotoOrderingCancelled",
            ],
            Self::Pile => [
                "cardsGotoPileBefore",
                "cardsGotoPileBegin",
                "cardsGotoPileEnd",
                "cardsGotoPileAfter",
                "cardsGotoPileCancelled",
            ],
            Self::Discard => [
                "cardsDiscardBefore",
                "cardsDiscardBegin",
                "cardsDiscardEnd",
                "cardsDiscardAfter",
                "cardsDiscardCancelled",
            ],
        };
        timings[match stage {
            MoveStage::Before => 0,
            MoveStage::Begin => 1,
            MoveStage::End => 2,
            MoveStage::After => 3,
            MoveStage::CancelFinish => 4,
            _ => unreachable!("only timing stages have a hook"),
        }]
    }
}

#[derive(Clone)]
pub(crate) struct Root {
    seat: SeatId,
    amount: u16,
    prompt: String,
    stage: RootStage,
    result: Option<PileOrderResult>,
    cards: Vec<Card>,
}
#[derive(Clone)]
pub(crate) struct Movement {
    owner: EventId,
    seat: SeatId,
    kind: MoveKind,
    stage: MoveStage,
    rows: [Vec<Card>; 2],
    moved: bool,
}
#[derive(Clone)]
pub(crate) enum Flow {
    Root(Root),
    Movement(Movement),
    Cleanup {
        owner: EventId,
        seat: SeatId,
        cards: Vec<Card>,
        after_discard: bool,
    },
}

impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::Root(root) => EventKind::ChoosePileOrder {
                seat: root.seat,
                amount: root.amount,
            },
            Self::Movement(m) => {
                let amount = m.rows.iter().map(Vec::len).sum();
                match m.kind {
                    MoveKind::Ordering => EventKind::CardsGotoOrdering {
                        seat: m.seat,
                        amount,
                    },
                    MoveKind::Pile => EventKind::CardsGotoPile {
                        seat: m.seat,
                        amount,
                    },
                    MoveKind::Discard => EventKind::CardsDiscard { amount },
                }
            }
            Self::Cleanup { .. } => EventKind::OrderingDiscard,
        }
    }
}

impl Game {
    pub(crate) fn pile_order_program(&self, seat: SeatId, amount: u16, prompt: &str) -> Program {
        Program::PileOrder(Flow::Root(Root {
            seat,
            amount,
            prompt: prompt.into(),
            stage: RootStage::Before,
            result: None,
            cards: vec![],
        }))
    }
    pub fn pile_order_result(&self, event: EventId) -> Option<&PileOrderResult> {
        match self.scheduler.active_frame(event)?.1 {
            Program::PileOrder(Flow::Root(root)) => root.result.as_ref(),
            _ => None,
        }
    }
    fn pile_order_owned(&self, owner: EventId, card: Card) -> bool {
        self.ordering_owners.get(&card.id) == Some(&owner) && self.resolving.contains(&card)
    }
    fn pile_order_movement(
        &self,
        owner: EventId,
        seat: SeatId,
        kind: MoveKind,
        rows: [Vec<Card>; 2],
    ) -> Program {
        Program::PileOrder(Flow::Movement(Movement {
            owner,
            seat,
            kind,
            stage: MoveStage::Before,
            rows,
            moved: false,
        }))
    }
    pub(crate) fn resolve_pile_order(
        &mut self,
        event: EventRef,
        flow: Flow,
        child: Option<Outcome<Resolution>>,
    ) {
        match flow {
            Flow::Root(root) => self.resolve_pile_order_root(event, root, child),
            Flow::Movement(movement) => self.resolve_pile_order_movement(movement, child),
            Flow::Cleanup {
                owner,
                seat,
                cards,
                after_discard,
            } => {
                let cards: Vec<_> = cards
                    .into_iter()
                    .filter(|&card| self.pile_order_owned(owner, card))
                    .collect();
                if !after_discard && !cards.is_empty() {
                    let movement = self.pile_order_movement(
                        owner,
                        seat,
                        MoveKind::Discard,
                        [cards.clone(), vec![]],
                    );
                    self.scheduler
                        .replace_current(Program::PileOrder(Flow::Cleanup {
                            owner,
                            seat,
                            cards,
                            after_discard: true,
                        }));
                    self.scheduler.next(movement);
                    return;
                }
                // If the discard was prevented, release any still-owned preview cards
                // back to the top in their original physical order.
                for card in cards.iter().rev() {
                    let taken = self.take_resolving(card.id);
                    self.classic.as_mut().unwrap().deck.push(taken);
                }
                self.private_ordering.remove(&owner);
                self.finish();
            }
        }
    }
    fn resolve_pile_order_root(
        &mut self,
        event: EventRef,
        mut root: Root,
        child: Option<Outcome<Resolution>>,
    ) {
        use RootStage::*;
        if root.result.is_none() {
            root.result = Some(PileOrderResult {
                event: event.id,
                rows: [vec![], vec![]],
                cancelled: false,
            });
        }
        // Cancellation of a root timing cancels the root; movement and choice cancellations
        // are handled as unsuccessful children instead.
        if matches!(child, Some(Outcome::Cancelled))
            && matches!(root.stage, Begin | Draw | After | Finish)
        {
            root.result.as_mut().unwrap().cancelled = true;
            root.stage = CancelNotify;
        }
        let seat = root.seat;
        match root.stage {
            Before | Begin | End | After => {
                let (timing, next) = match root.stage {
                    Before => ("choosePileOrderBefore", Begin),
                    Begin => ("choosePileOrderBegin", Draw),
                    End => ("choosePileOrderEnd", After),
                    After => ("choosePileOrderAfter", Finish),
                    _ => unreachable!(),
                };
                root.stage = next;
                self.scheduler
                    .replace_current(Program::PileOrder(Flow::Root(root)));
                self.queue_timing(timing, Some(seat), None, None, 0, &[]);
            }
            Draw => {
                let Some(cards) = self.draw_from_piles(usize::from(root.amount)) else {
                    root.stage = End;
                    self.scheduler
                        .replace_current(Program::PileOrder(Flow::Root(root)));
                    return;
                };
                self.reserved_draws.extend(cards.iter().copied());
                root.cards = cards.clone();
                // Register cleanup before starting the movement child.
                self.scheduler.after(Program::PileOrder(Flow::Cleanup {
                    owner: event.id,
                    seat,
                    cards: cards.clone(),
                    after_discard: false,
                }));
                root.stage = Ordered;
                self.scheduler
                    .replace_current(Program::PileOrder(Flow::Root(root)));
                let movement =
                    self.pile_order_movement(event.id, seat, MoveKind::Ordering, [cards, vec![]]);
                self.scheduler.next(movement);
            }
            Ordered => {
                if matches!(child, Some(Outcome::Cancelled)) {
                    root.stage = End;
                    self.scheduler
                        .replace_current(Program::PileOrder(Flow::Root(root)));
                    return;
                }
                let cards: Vec<_> = root
                    .cards
                    .iter()
                    .copied()
                    .filter(|&card| self.pile_order_owned(event.id, card))
                    .collect();
                let n = cards.len();
                let request = MoveChoiceRequest {
                    seat,
                    prompt: root.prompt.clone(),
                    rows: vec![
                        MoveRow {
                            label: "牌堆顶".into(),
                            cards,
                            min: 0,
                            max: n,
                        },
                        MoveRow {
                            label: "牌堆底".into(),
                            cards: vec![],
                            min: 0,
                            max: n,
                        },
                    ],
                    allow_decline: false,
                    filter_move: None,
                    filter_ok: None,
                    bot: Some(Arc::new(|_, rows| {
                        Some(
                            rows[0]
                                .iter()
                                .enumerate()
                                .map(|(index, card)| MoveStep {
                                    card: card.id,
                                    row: 1,
                                    index,
                                })
                                .collect(),
                        )
                    })),
                };
                let choice = self.plan_move_choice(request);
                root.stage = Chosen;
                self.scheduler
                    .replace_current(Program::PileOrder(Flow::Root(root)));
                if let Ok(choice) = choice {
                    self.scheduler.next(choice);
                }
            }
            Chosen => {
                let rows = match child {
                    Some(Outcome::Finished(Resolution::MovedCards(result)))
                        if !result.cancelled =>
                    {
                        result.rows
                    }
                    _ => None,
                };
                if let Some(rows) = rows.filter(|rows| rows.len() == 2) {
                    let mut iter = rows.into_iter();
                    let rows = [iter.next().unwrap(), iter.next().unwrap()];
                    let movement = self.pile_order_movement(event.id, seat, MoveKind::Pile, rows);
                    root.stage = Piled;
                    self.scheduler
                        .replace_current(Program::PileOrder(Flow::Root(root)));
                    self.scheduler.next(movement);
                } else {
                    root.stage = End;
                    self.scheduler
                        .replace_current(Program::PileOrder(Flow::Root(root)));
                }
            }
            Piled => {
                if let Some(Outcome::Finished(Resolution::PileOrdered(result))) = child {
                    root.result.as_mut().unwrap().rows = result.rows;
                }
                root.stage = End;
                self.scheduler
                    .replace_current(Program::PileOrder(Flow::Root(root)));
            }
            CancelNotify => {
                root.stage = CancelFinish;
                self.scheduler
                    .replace_current(Program::PileOrder(Flow::Root(root)));
                self.queue_timing("choosePileOrderCancelled", Some(seat), None, None, 0, &[]);
            }
            Finish | CancelFinish => {
                self.scheduler
                    .end(Outcome::Finished(Resolution::PileOrdered(
                        root.result.unwrap(),
                    )))
            }
        }
    }
    fn resolve_pile_order_movement(&mut self, mut m: Movement, child: Option<Outcome<Resolution>>) {
        use MoveStage::*;
        if matches!(child, Some(Outcome::Cancelled)) && m.stage != CancelFinish {
            if matches!(m.kind, MoveKind::Ordering) && !m.moved {
                for card in m.rows[0].iter().rev() {
                    if let Some(index) = self.reserved_draws.iter().position(|c| c.id == card.id) {
                        let card = self.reserved_draws.remove(index);
                        self.classic.as_mut().unwrap().deck.push(card);
                    }
                }
            }
            m.stage = CancelFinish;
            self.queue_pile_order_hook(m, CancelFinish);
            return;
        }
        match m.stage {
            Before | Begin | End | After => {
                let stage = m.stage;
                m.stage = match stage {
                    Before => Begin,
                    Begin => Move,
                    End => After,
                    After => Finish,
                    _ => unreachable!(),
                };
                self.queue_pile_order_hook(m, stage);
            }
            Move => {
                match m.kind {
                    MoveKind::Ordering => {
                        for card in &m.rows[0] {
                            if let Some(index) =
                                self.reserved_draws.iter().position(|c| c.id == card.id)
                            {
                                let card = self.reserved_draws.remove(index);
                                self.resolving.push(card);
                                self.ordering_owners.insert(card.id, m.owner);
                            }
                        }
                        self.private_ordering.insert(m.owner);
                    }
                    MoveKind::Pile | MoveKind::Discard => {
                        for row in &mut m.rows {
                            row.retain(|&card| self.pile_order_owned(m.owner, card));
                            for card in row.iter() {
                                self.take_resolving(card.id);
                            }
                        }
                        match m.kind {
                            MoveKind::Pile => {
                                for card in m.rows.iter().flatten() {
                                    self.mark_known_to(card.id, m.seat);
                                }
                                let deck = &mut self.classic.as_mut().unwrap().deck;
                                deck.extend(m.rows[0].iter().rev().copied());
                                deck.splice(..0, m.rows[1].iter().rev().copied());
                            }
                            MoveKind::Discard => {
                                self.discard.extend(m.rows.iter().flatten().copied())
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                m.moved = true;
                m.stage = End;
                self.scheduler
                    .replace_current(Program::PileOrder(Flow::Movement(m)));
            }
            Finish | CancelFinish => {
                if matches!(m.kind, MoveKind::Pile) && m.moved {
                    let mut top = m.rows[0].clone();
                    top.reverse();
                    self.scheduler
                        .end(Outcome::Finished(Resolution::PileOrdered(
                            PileOrderResult {
                                event: m.owner,
                                rows: [top, m.rows[1].clone()],
                                cancelled: m.stage == CancelFinish,
                            },
                        )));
                } else if m.stage == CancelFinish {
                    self.scheduler.end(Outcome::Cancelled);
                } else {
                    self.finish();
                }
            }
        }
    }

    fn queue_pile_order_hook(&mut self, movement: Movement, stage: MoveStage) {
        let timing = movement.kind.hook(stage);
        let seat = movement.seat;
        let cards = self
            .wants_timing(timing)
            .then(|| movement.rows.iter().flatten().copied().collect::<Vec<_>>());
        self.scheduler
            .replace_current(Program::PileOrder(Flow::Movement(movement)));
        if let Some(cards) = cards {
            self.queue_timing(
                timing,
                Some(seat),
                None,
                None,
                cards.len().min(u16::MAX as usize) as u16,
                &cards,
            );
        }
    }
}
