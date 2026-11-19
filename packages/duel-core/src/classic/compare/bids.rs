//! Sealed multi-seat bid collection, common payment and ordering cleanup.
use super::{Bid, HeldBid};
use crate::scheduler::{EventRef, Outcome};
use crate::{
    Answer, Card, ChoiceBranch, ChoiceKind, ChoiceView, CompareGroup, CompareParticipants,
    Decision, EventId, EventKind, Game, Program, Resolution, SeatId,
};
use std::sync::Arc;

#[cfg(test)]
#[path = "bids_tests.rs"]
mod tests;

#[derive(Clone)]
pub(crate) enum BidGroup {
    Pair(CompareParticipants),
    Group {
        initiator: SeatId,
        targets: Arc<[SeatId]>,
    },
}
impl BidGroup {
    fn initiator(&self) -> SeatId {
        match self {
            Self::Pair(p) => p.initiator,
            Self::Group { initiator, .. } => *initiator,
        }
    }
    fn seats(&self) -> Vec<SeatId> {
        match self {
            Self::Pair(p) => vec![p.initiator, p.target],
            Self::Group { initiator, targets } => std::iter::once(*initiator)
                .chain(targets.iter().copied())
                .collect(),
        }
    }
    fn pair(&self) -> Option<CompareParticipants> {
        if let Self::Pair(pair) = self {
            Some(*pair)
        } else {
            None
        }
    }
    fn target(&self) -> Option<SeatId> {
        self.pair().map(|p| p.target)
    }
}
#[derive(Clone)]
pub(crate) enum BidFlow {
    Collect {
        group: BidGroup,
        stage: CollectStage,
        cards: Vec<Card>,
    },
    Bid {
        group: BidGroup,
        index: usize,
        stage: Bid,
    },
    Pay {
        group: BidGroup,
        cards: Vec<Card>,
        stage: PayStage,
    },
    Show {
        seat: SeatId,
        card: Card,
        visible: bool,
    },
    Cleanup {
        cards: Vec<Card>,
    },
    Discard {
        cards: Vec<Card>,
        moved: bool,
    },
}
#[derive(Clone, Copy)]
pub(crate) enum CollectStage {
    Begin,
    Next,
    Await,
    After,
    Finish,
}
#[derive(Clone, Copy)]
pub(crate) enum PayStage {
    Begin,
    Lose,
    After,
    Finish,
}
impl BidFlow {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::Collect {
                group: BidGroup::Pair(p),
                ..
            } => EventKind::CompareChoices {
                seat: p.initiator,
                target: p.target,
            },
            Self::Collect {
                group: BidGroup::Group { initiator, targets },
                ..
            } => EventKind::GroupChoices {
                seat: *initiator,
                count: targets.len(),
            },
            Self::Bid {
                group: BidGroup::Pair(p),
                index,
                ..
            } => EventKind::CompareBid {
                seat: p.seats()[*index],
            },
            Self::Bid {
                group: BidGroup::Group { initiator, targets },
                index,
                ..
            } => EventKind::GroupBid {
                seat: if *index == 0 {
                    *initiator
                } else {
                    targets[*index - 1]
                },
                initiator: *initiator,
            },
            Self::Pay { .. } => EventKind::LoseAsync,
            Self::Show {
                seat,
                card,
                visible: true,
            } => EventKind::ShowCards {
                seat: *seat,
                card: *card,
            },
            Self::Show {
                seat,
                visible: false,
                ..
            } => EventKind::ShowBid { seat: *seat },
            Self::Cleanup { .. } => EventKind::OrderingDiscard,
            Self::Discard { cards, .. } => EventKind::CardsDiscard {
                amount: cards.len(),
            },
        }
    }
    pub(super) fn pair_choice(&self) -> Option<CompareParticipants> {
        if let Self::Bid { group, .. } = self {
            group.pair()
        } else {
            None
        }
    }
    pub(super) fn selected(&self) -> Option<Card> {
        if let Self::Bid {
            stage: Bid::Finish(card),
            ..
        } = self
        {
            Some(*card)
        } else {
            None
        }
    }
}
impl Game {
    pub(crate) fn collect_bids(&self, group: BidGroup) -> Program {
        Program::Bids(BidFlow::Collect {
            group,
            stage: CollectStage::Begin,
            cards: Vec::new(),
        })
    }
    pub(crate) fn pay_bids(&self, group: BidGroup, cards: Vec<Card>) -> Program {
        Program::Bids(BidFlow::Pay {
            group,
            cards,
            stage: PayStage::Begin,
        })
    }
    pub(crate) fn cleanup_bids(&self, cards: Vec<Card>) -> Program {
        Program::Bids(BidFlow::Cleanup { cards })
    }
    pub(crate) fn show_bid(&self, seat: SeatId, card: Card, visible: bool) -> Program {
        Program::Bids(BidFlow::Show {
            seat,
            card,
            visible,
        })
    }
    pub(crate) fn reveal_compare_bids(&mut self, root: EventId, cards: &[Card]) {
        for card in cards {
            if let Some(HeldBid { revealed, .. }) = self
                .compares
                .ordering
                .get_mut(&card.id)
                .filter(|bid| bid.comparison == root)
            {
                *revealed = true;
            }
        }
    }
    fn bids_hook(&mut self, timing: &'static str, group: &BidGroup) {
        self.queue_timing(
            timing,
            Some(group.initiator()),
            None,
            group.target(),
            0,
            &[],
        );
    }
    pub(crate) fn resolve_bids(
        &mut self,
        event: EventRef,
        flow: BidFlow,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        if self.match_result().is_some()
            && !matches!(flow, BidFlow::Cleanup { .. } | BidFlow::Discard { .. })
        {
            self.scheduler.end(Outcome::Cancelled);
            return;
        }
        match flow {
            BidFlow::Collect {
                group,
                mut stage,
                mut cards,
            } => {
                if matches!(child, Some(Outcome::Cancelled)) {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                match stage {
                    CollectStage::Begin => {
                        self.bids_hook("chooseCardOLBegin", &group);
                        stage = CollectStage::Next;
                    }
                    CollectStage::Next => {
                        let index = cards.len();
                        if index == group.seats().len() {
                            stage = CollectStage::After;
                        } else {
                            self.scheduler.next(Program::Bids(BidFlow::Bid {
                                group: group.clone(),
                                index,
                                stage: Bid::Begin,
                            }));
                            stage = CollectStage::Await;
                        }
                    }
                    CollectStage::Await => {
                        let Some(Outcome::Finished(Resolution::CompareBid(card))) = child else {
                            self.scheduler.end(Outcome::Cancelled);
                            return;
                        };
                        cards.push(card);
                        if cards.len() == group.seats().len() {
                            self.bids_hook("chooseCardOLAfter", &group);
                            stage = CollectStage::Finish;
                        } else {
                            self.scheduler.next(Program::Bids(BidFlow::Bid {
                                group: group.clone(),
                                index: cards.len(),
                                stage: Bid::Begin,
                            }));
                        }
                    }
                    CollectStage::After => {
                        self.bids_hook("chooseCardOLAfter", &group);
                        stage = CollectStage::Finish;
                    }
                    CollectStage::Finish => {
                        self.scheduler
                            .end(Outcome::Finished(Resolution::LockedBids(cards)));
                        return;
                    }
                }
                self.scheduler
                    .replace_current(Program::Bids(BidFlow::Collect {
                        group,
                        stage,
                        cards,
                    }));
            }
            BidFlow::Bid {
                group,
                index,
                stage,
            } => {
                if matches!(child, Some(Outcome::Cancelled)) {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                let seats = group.seats();
                let seat = seats[index];
                let initiator = group.initiator();
                if matches!(stage, Bid::Begin | Bid::Select)
                    && seats.into_iter().any(|seat| self.is_out(seat))
                {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                match stage {
                    Bid::Begin => {
                        self.queue_timing(
                            "chooseCardBegin",
                            Some(seat),
                            Some(initiator),
                            None,
                            0,
                            &[],
                        );
                        self.scheduler.replace_current(Program::Bids(BidFlow::Bid {
                            group,
                            index,
                            stage: Bid::Select,
                        }));
                    }
                    Bid::Select => {
                        let player = self.seat(seat).expect("validated participant");
                        if !self.is_in(seat) || player.hand.is_empty() {
                            self.scheduler.end(Outcome::Cancelled);
                            return;
                        }
                        if let Some(Answer::Select(selection)) = answer.take() {
                            let Some(card) = player
                                .hand
                                .iter()
                                .find(|card| selection.cards == [card.id])
                                .copied()
                            else {
                                self.scheduler.end(Outcome::Cancelled);
                                return;
                            };
                            self.scheduler.replace_current(Program::Bids(BidFlow::Bid {
                                group,
                                index,
                                stage: Bid::Finish(card),
                            }));
                            self.queue_timing(
                                "chooseCardAfter",
                                Some(seat),
                                Some(initiator),
                                None,
                                0,
                                &[],
                            );
                        } else {
                            let target = match group.pair() {
                                Some(p) => Some(if index == 0 { p.target } else { p.initiator }),
                                None if index > 0 => Some(initiator),
                                None => None,
                            };
                            let compare = group.pair();
                            let compare_group = match &group {
                                BidGroup::Group { initiator, targets } => Some(CompareGroup {
                                    initiator: *initiator,
                                    targets: targets.to_vec(),
                                }),
                                _ => None,
                            };
                            self.decision = Some(Decision::Cards(ChoiceView {
                                equipment_buttons: None,
                                move_cards: None,
                                event: event.id,
                                seat,
                                target,
                                kind: ChoiceKind::Content,
                                min: 0,
                                max: 0,
                                cards: vec![],
                                counter: None,
                                compare,
                                compare_group,
                                branches: vec![ChoiceBranch {
                                    id: 1,
                                    label: "请选择一张手牌拼点".into(),
                                    cards: player.hand.iter().map(|c| c.id).collect(),
                                    min_cards: 1,
                                    max_cards: 1,
                                    targets: vec![],
                                    min_targets: 0,
                                    max_targets: 0,
                                }],
                            }));
                        }
                    }
                    Bid::Finish(card) => self
                        .scheduler
                        .end(Outcome::Finished(Resolution::CompareBid(card))),
                }
            }
            BidFlow::Pay {
                group,
                cards,
                stage,
            } => match stage {
                PayStage::Begin => {
                    let is_group = matches!(group, BidGroup::Group { .. });
                    self.scheduler.replace_current(Program::Bids(BidFlow::Pay {
                        group,
                        cards,
                        stage: PayStage::Lose,
                    }));
                    if is_group {
                        self.queue_timing("loseAsyncBegin", None, None, None, 0, &[]);
                    }
                }
                PayStage::Lose => {
                    if matches!(child, Some(Outcome::Cancelled))
                        || group.seats().into_iter().any(|seat| self.is_out(seat))
                    {
                        self.scheduler.end(Outcome::Cancelled);
                        return;
                    }
                    let root = event.parent.expect("comparison parent");
                    for (seat, card) in group.seats().into_iter().zip(cards.iter().copied()) {
                        self.scheduler
                            .next(self.lose_compare_program(root, seat, card));
                    }
                    self.scheduler.replace_current(Program::Bids(BidFlow::Pay {
                        group,
                        cards,
                        stage: PayStage::After,
                    }));
                }
                PayStage::After => {
                    let is_group = matches!(group, BidGroup::Group { .. });
                    self.scheduler.replace_current(Program::Bids(BidFlow::Pay {
                        group,
                        cards,
                        stage: PayStage::Finish,
                    }));
                    if is_group {
                        self.queue_timing("loseAsyncAfter", None, None, None, 0, &[]);
                    }
                }
                PayStage::Finish => self.finish(),
            },
            BidFlow::Show { seat, card, .. } => {
                self.queue_timing("showCardsAfter", Some(seat), None, None, 1, &[card]);
                self.finish();
            }
            BidFlow::Cleanup { cards } => {
                let root = event.parent.expect("comparison parent");
                let remaining: Vec<_> = cards
                    .into_iter()
                    .filter(|card| self.compares.owns_ordering(root, card.id))
                    .collect();
                if !remaining.is_empty() {
                    self.scheduler.next(Program::Bids(BidFlow::Discard {
                        cards: remaining,
                        moved: false,
                    }));
                }
                self.finish();
            }
            BidFlow::Discard { cards, moved } => {
                if moved {
                    self.queue_timing(
                        "cardsDiscardAfter",
                        None,
                        None,
                        None,
                        cards.len() as u16,
                        &cards,
                    );
                    self.finish();
                } else {
                    for &card in &cards {
                        self.next(EventKind::Discard { card });
                    }
                    self.scheduler
                        .replace_current(Program::Bids(BidFlow::Discard { cards, moved: true }));
                }
            }
        }
    }
}
