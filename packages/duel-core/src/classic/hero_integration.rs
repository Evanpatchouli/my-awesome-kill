use super::skills::{HeroAdjustment, HeroMoment};
use crate::scheduler::Outcome;
use crate::{Card, CardKind, EventId, EventKind, Game, Program, Resolution, SeatId, Zone};

#[derive(Clone)]
pub(crate) enum Cleanup {
    Place {
        owner: EventId,
        seat: SeatId,
        card: Card,
    },
    Ordering {
        owner: EventId,
        cards: Vec<Card>,
    },
    Cards {
        cards: Vec<Card>,
        stage: DiscardStage,
    },
    MoveToDiscard(Card),
}

#[derive(Clone, Copy)]
pub(crate) enum DiscardStage {
    Before,
    Begin,
    Apply,
    End,
    After,
    Finish,
    Cancel,
    FinishCancelled,
}

impl Cleanup {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::Place { seat, card, .. } => EventKind::Reveal {
                seat: *seat,
                card: *card,
            },
            Self::Ordering { .. } => EventKind::OrderingDiscard,
            Self::Cards { cards, .. } => EventKind::CardsDiscard {
                amount: cards.len(),
            },
            Self::MoveToDiscard(card) => EventKind::Discard { card: *card },
        }
    }
}

#[derive(Clone, Copy)]
enum JudgeStep {
    Draw,
    Replaced(Card),
    Fixing(Card),
    AfterFixing(Card),
    Finish(Card),
}

#[derive(Clone, Copy)]
pub(crate) struct Judgment {
    pub(crate) seat: SeatId,
    keep_black: bool,
    delayed_kind: Option<CardKind>,
    drawn: Option<Card>,
    step: JudgeStep,
}

impl Game {
    /// The current or most recent physical judgment result on an active frame.
    pub fn judgment_card(&self, id: EventId) -> Option<Card> {
        let (_, program) = self.scheduler.active_frame(id)?;
        match program {
            Program::Phase(flow) => flow.judgment_card(),
            Program::Judgment(judge) => match judge.step {
                JudgeStep::Replaced(card)
                | JudgeStep::Fixing(card)
                | JudgeStep::AfterFixing(card)
                | JudgeStep::Finish(card) => Some(card),
                JudgeStep::Draw => None,
            },
            _ => None,
        }
    }
    /// The fixed result of a live delayed-card judgment, once `judgeFixing` starts.
    pub fn judgment_result(&self, id: EventId) -> Option<bool> {
        let (_, program) = self.scheduler.active_frame(id)?;
        if let Program::Phase(flow) = program {
            return flow.judgment_result();
        }
        let Program::Judgment(judge) = program else {
            return None;
        };
        let card = match judge.step {
            JudgeStep::AfterFixing(card) | JudgeStep::Finish(card) => card,
            _ => return None,
        };
        self.delayed_judgment_value(judge.delayed_kind?, card)
    }

    pub(crate) fn delayed_judgment_value(&self, kind: CardKind, card: Card) -> Option<bool> {
        let face = self.face(card.id)?;
        match kind {
            CardKind::Lebu => Some(face.suit == crate::classic_data::Suit::Heart),
            CardKind::Shandian => Some(
                !(face.suit == crate::classic_data::Suit::Spade && (2..=9).contains(&face.rank)),
            ),
            _ => None,
        }
    }

    /// Card currently held by the live judge event before its result is fixed.
    pub fn judging_card(&self, seat: SeatId) -> Option<Card> {
        let mut id = Some(self.scheduler.current_id());
        while let Some(current) = id {
            let (event, program) = self.scheduler.active_frame(current)?;
            if let Program::Judgment(judge) = program {
                if judge.seat == seat {
                    if let JudgeStep::Replaced(card) = judge.step {
                        return Some(card);
                    }
                }
            }
            id = event.parent;
        }
        None
    }

    pub fn judging_owner(&self, card: crate::CardId) -> Option<SeatId> {
        let mut id = Some(self.scheduler.current_id());
        while let Some(current) = id {
            let (event, program) = self.scheduler.active_frame(current)?;
            if let Program::Judgment(judge) = program {
                if matches!(judge.step, JudgeStep::Replaced(judging) if judging.id == card) {
                    return Some(judge.seat);
                }
            }
            id = event.parent;
        }
        None
    }

    pub(crate) fn update_judging_card(&mut self, old: Card, replacement: Card) {
        let mut id = Some(self.scheduler.current_id());
        while let Some(current) = id {
            let Some((frame, program)) = self.scheduler.active_frame(current) else {
                return;
            };
            let parent = frame.parent;
            let matches = matches!(program, Program::Judgment(judge) if matches!(judge.step, JudgeStep::Replaced(card) if card == old));
            if matches {
                if let Some(Program::Judgment(judge)) = self.scheduler.active_program_mut(current) {
                    judge.step = JudgeStep::Replaced(replacement);
                }
                return;
            }
            id = parent;
        }
    }

    pub(crate) fn resolve_judgment_cleanup(
        &mut self,
        cleanup: Cleanup,
        child: Option<Outcome<Resolution>>,
    ) {
        match cleanup {
            Cleanup::Place {
                owner,
                seat: _,
                card,
            } => {
                self.resolving.push(card);
                self.ordering_owners.insert(card.id, owner);
                self.finish();
            }
            Cleanup::Ordering { owner, cards } => {
                let cards: Vec<_> = cards
                    .into_iter()
                    .filter(|card| {
                        self.resolving.contains(card)
                            && self.ordering_owners.get(&card.id) == Some(&owner)
                    })
                    .collect();
                if !cards.is_empty() {
                    self.scheduler
                        .next(Program::JudgmentCleanup(Cleanup::Cards {
                            cards,
                            stage: DiscardStage::Before,
                        }));
                }
                self.finish();
            }
            Cleanup::MoveToDiscard(card) => {
                let physical = if let Some((seat, zone, _)) = self.held_card_location(card.id) {
                    Some(self.take_card(seat, zone, card.id))
                } else {
                    self.take_unowned_card(card)
                };
                if let Some(physical) = physical {
                    self.discard.push(physical);
                }
                self.finish();
            }
            Cleanup::Cards { cards, stage } => {
                self.resolve_ordering_cards_discard(cards, stage, child)
            }
        }
    }

    fn resolve_ordering_cards_discard(
        &mut self,
        cards: Vec<Card>,
        mut stage: DiscardStage,
        child: Option<Outcome<Resolution>>,
    ) {
        use DiscardStage::*;
        if matches!(child, Some(Outcome::Cancelled)) && !matches!(stage, FinishCancelled) {
            stage = Cancel;
        }
        loop {
            let (timing, next) = match stage {
                Before => ("cardsDiscardBefore", Begin),
                Begin => ("cardsDiscardBegin", Apply),
                Apply => {
                    // orderingDiscard selected its list before this event was created.
                    // cardsDiscard acts on those same entities, even after a Before hook moved one.
                    for &card in &cards {
                        self.scheduler
                            .next(Program::JudgmentCleanup(Cleanup::MoveToDiscard(card)));
                    }
                    self.scheduler
                        .replace_current(Program::JudgmentCleanup(Cleanup::Cards {
                            cards,
                            stage: End,
                        }));
                    return;
                }
                End => ("cardsDiscardEnd", After),
                After => ("cardsDiscardAfter", Finish),
                Cancel => ("cardsDiscardCancelled", FinishCancelled),
                Finish => {
                    self.finish();
                    return;
                }
                FinishCancelled => {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
            };
            stage = next;
            if self.queue_timing(timing, None, None, None, cards.len() as u16, &cards) {
                self.scheduler
                    .replace_current(Program::JudgmentCleanup(Cleanup::Cards { cards, stage }));
                return;
            }
        }
    }
    pub(crate) fn judge_program(&self, seat: SeatId, keep_black: bool) -> Program {
        Program::Judgment(Judgment {
            seat,
            keep_black,
            delayed_kind: None,
            drawn: None,
            step: JudgeStep::Draw,
        })
    }

    pub(crate) fn delayed_judge_program(&self, seat: SeatId, kind: CardKind) -> Program {
        Program::Judgment(Judgment {
            seat,
            keep_black: false,
            delayed_kind: Some(kind),
            drawn: None,
            step: JudgeStep::Draw,
        })
    }

    pub(crate) fn resolve_judgment(
        &mut self,
        mut judgment: Judgment,
        child: Option<Outcome<Resolution>>,
    ) {
        use Outcome::{Cancelled, Finished};
        use Resolution::{Hero, Judged};
        let seat = judgment.seat;
        match judgment.step {
            JudgeStep::Draw => {
                let Some(cards) = self.draw_from_piles(1) else {
                    self.scheduler.end(Cancelled);
                    return;
                };
                let card = cards[0];
                let owner = self.scheduler.current_id();
                judgment.drawn = Some(card);
                judgment.step = JudgeStep::Replaced(card);
                self.scheduler.replace_current(Program::Judgment(judgment));
                self.scheduler
                    .next(Program::JudgmentCleanup(Cleanup::Place {
                        owner,
                        seat,
                        card,
                    }));
                let moment = HeroMoment::JudgeBefore { seat, card };
                if self.wants_timing("judge") || self.has_hero_triggers(&moment) {
                    let dispatch = self.hero_triggers(moment);
                    self.scheduler.next(dispatch);
                }
            }
            JudgeStep::Replaced(original) => {
                let card = match child {
                    Some(Finished(Hero(HeroAdjustment::Judgment(card)))) => card,
                    _ => original,
                };
                if Some(card) != judgment.drawn && self.resolving.contains(&card) {
                    let owner = self.scheduler.current_id();
                    self.ordering_owners.insert(card.id, owner);
                }
                judgment.step = JudgeStep::Fixing(card);
                self.scheduler.replace_current(Program::Judgment(judgment));
            }
            JudgeStep::Fixing(card) => {
                judgment.step = JudgeStep::AfterFixing(card);
                self.scheduler.replace_current(Program::Judgment(judgment));
                self.queue_timing("judgeFixing", Some(seat), None, None, 0, &[]);
            }
            JudgeStep::AfterFixing(card) => {
                self.apply_judgment(judgment, card);
            }
            JudgeStep::Finish(card) => {
                let owner = self.scheduler.current_id();
                let mut cards = judgment.drawn.into_iter().collect::<Vec<_>>();
                if !cards.contains(&card) {
                    cards.push(card);
                }
                if cards.iter().any(|card| {
                    self.resolving.contains(card)
                        && self.ordering_owners.get(&card.id) == Some(&owner)
                }) {
                    self.scheduler
                        .after(Program::JudgmentCleanup(Cleanup::Ordering { owner, cards }));
                }
                self.scheduler.end(Finished(Judged(card)));
            }
        }
    }

    fn apply_judgment(&mut self, mut judgment: Judgment, card: Card) {
        let moment = HeroMoment::JudgeAfter {
            seat: judgment.seat,
            card,
            keep_black: judgment.keep_black,
        };
        judgment.step = JudgeStep::Finish(card);
        self.scheduler.replace_current(Program::Judgment(judgment));
        if self.wants_timing("judgeEnd") || self.has_hero_triggers(&moment) {
            let dispatch = self.hero_triggers(moment);
            self.scheduler.next(dispatch);
        }
    }

    pub(crate) fn queue_hero_moment(&mut self, moment: HeroMoment) -> bool {
        if self.match_result().is_some() || !self.has_hero_triggers(&moment) {
            return false;
        }
        let dispatch = self.hero_triggers(moment);
        self.scheduler.next(dispatch);
        true
    }

    pub(crate) fn queue_discard_batch(&mut self, seat: SeatId, cards: Vec<(Zone, Card)>) {
        if !cards.is_empty() {
            self.scheduler.next(Program::DiscardGroup { seat, cards });
        }
    }

    pub(crate) fn resolve_discard_group(&mut self, seat: SeatId, cards: Vec<(Zone, Card)>) {
        for &(zone, card) in &cards {
            let card = self.take_card(seat, zone, card.id);
            self.discard.push(card);
            self.scheduler.next(Program::Discarded(card));
        }
        self.queue_hero_losses(seat, cards);
        self.finish();
    }

    pub(crate) fn queue_hero_losses(&mut self, seat: SeatId, cards: Vec<(Zone, Card)>) -> bool {
        cards
            .iter()
            .any(|(zone, _)| matches!(zone, Zone::Hand | Zone::Equipment))
            && self.queue_hero_moment(HeroMoment::LostCards { seat, cards })
    }

    pub(crate) fn queue_use_costs(&mut self, seat: SeatId, cards: &[Card]) {
        if cards.is_empty() {
            return;
        }
        let cards = cards
            .iter()
            .map(|&card| (self.zone_for_held_card(seat, card.id), card))
            .collect();
        self.scheduler.next(Program::UseCosts {
            seat,
            cards,
            paid: false,
        });
    }

    pub(crate) fn resolve_use_costs(&mut self, seat: SeatId, cards: Vec<(Zone, Card)>, paid: bool) {
        if !paid {
            for &(_, card) in &cards {
                self.next(EventKind::UseCost { seat, card });
            }
            self.scheduler.replace_current(Program::UseCosts {
                seat,
                cards,
                paid: true,
            });
            return;
        }
        let moment = HeroMoment::LostCards { seat, cards };
        if self.wants_timing("loseAfter") || self.has_hero_triggers(&moment) {
            let dispatch = self.hero_triggers(moment);
            self.scheduler.next(dispatch);
        }
        self.finish();
    }
}
#[cfg(test)]
mod dispatch_tests;
