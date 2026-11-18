use crate::scheduler::{EventRef, Outcome};
use crate::{
    Answer, Card, CardId, ChoiceBranch, ChoiceKind, ChoiceView, CompareParticipants, CompareResult,
    Decision, EventId, EventKind, Game, Program, Resolution, RuleError, SeatId,
};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct State {
    // A bid in ordering still has exactly one physical location. The reservation
    // expires on the first move out of that zone, even if the same ID later returns.
    ordering: HashMap<CardId, HeldBid>,
    live: HashMap<EventId, Scored>,
    history: Vec<CompareResult>,
}

struct HeldBid {
    comparison: EventId,
    revealed: bool,
}

struct Scored {
    result: CompareResult,
    gained: [bool; 2],
}

impl State {
    pub(crate) fn forget(&mut self, id: CardId) {
        self.ordering.remove(&id);
    }

    pub(crate) fn hidden(&self, id: CardId) -> bool {
        self.ordering.get(&id).is_some_and(|bid| !bid.revealed)
    }

    fn owns_ordering(&self, comparison: EventId, id: CardId) -> bool {
        self.ordering
            .get(&id)
            .is_some_and(|bid| bid.comparison == comparison)
    }
}

impl CompareParticipants {
    fn seats(self) -> [SeatId; 2] {
        [self.initiator, self.target]
    }
}

impl CompareResult {
    fn score(&mut self) {
        self.won = self.numbers[0] > self.numbers[1];
        self.tie = self.numbers[0] == self.numbers[1];
        self.winner = if self.tie {
            None
        } else if self.won {
            Some(self.participants.initiator)
        } else {
            Some(self.participants.target)
        };
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Flow {
    Compare(Comparing),
    Collect(CompareParticipants, Collection),
    Bid(CompareParticipants, usize, Bid),
    Lose(CompareParticipants, [Card; 2]),
    Show(SeatId, Card),
    Cleanup([Card; 2]),
    Discard([Option<Card>; 2], bool),
}

#[derive(Clone, Copy)]
pub(crate) struct Comparing {
    seats: CompareParticipants,
    stage: Stage,
}

#[derive(Clone, Copy)]
enum Stage {
    Begin,
    Collect,
    Collected,
    Paid([Card; 2]),
    Reveal([Card; 2]),
    Score([Card; 2]),
    Fix,
    Announce,
    After,
    Finish,
    Cancelled(Option<[Card; 2]>),
}

#[derive(Clone, Copy)]
pub(crate) enum Collection {
    Begin,
    First,
    FirstChosen,
    SecondChosen(Card),
    Finish([Card; 2]),
}

#[derive(Clone, Copy)]
pub(crate) enum Bid {
    Begin,
    Select,
    Finish(Card),
}

impl Flow {
    pub(crate) fn kind(self) -> EventKind {
        match self {
            Self::Compare(event) => EventKind::Compare {
                seat: event.seats.initiator,
                target: event.seats.target,
            },
            Self::Collect(seats, _) => EventKind::CompareChoices {
                seat: seats.initiator,
                target: seats.target,
            },
            Self::Bid(seats, index, _) => EventKind::CompareBid {
                seat: seats.seats()[index],
            },
            Self::Lose(_, _) => EventKind::LoseAsync,
            Self::Show(seat, card) => EventKind::ShowCards { seat, card },
            Self::Cleanup(_) => EventKind::OrderingDiscard,
            Self::Discard(cards, _) => EventKind::CardsDiscard {
                amount: cards.into_iter().flatten().count(),
            },
        }
    }
}

impl Game {
    /// Whether these two living seats can each contribute a hand card to an ordinary comparison.
    pub fn can_compare(&self, initiator: SeatId, target: SeatId) -> bool {
        self.compare_program(initiator, target).is_ok()
    }

    pub(crate) fn compare_program(
        &self,
        initiator: SeatId,
        target: SeatId,
    ) -> Result<Program, RuleError> {
        if self.match_result().is_some() {
            return Err(RuleError::GameOver);
        }
        if self.classic.is_none() {
            return Err(RuleError::UnsupportedCard);
        }
        if initiator == target {
            return Err(RuleError::InvalidTarget);
        }
        for seat in [initiator, target] {
            let player = self.seat(seat)?;
            if !player.alive {
                return Err(RuleError::InactiveSeat { seat });
            }
            if player.hand.is_empty() {
                return Err(RuleError::InvalidCard);
            }
        }
        Ok(Program::Compare(Flow::Compare(Comparing {
            seats: CompareParticipants { initiator, target },
            stage: Stage::Begin,
        })))
    }

    /// The two physical bids and their current effective numbers, once both cards have been shown.
    /// A finished comparison remains queryable by its own event ID.
    pub fn comparison(&self, subject: EventId) -> Option<&CompareResult> {
        self.compares
            .live
            .get(&subject)
            .map(|s| &s.result)
            .or_else(|| {
                self.compares
                    .history
                    .iter()
                    .rev()
                    .find(|r| r.event == subject)
            })
    }

    pub fn compare_history(&self) -> &[CompareResult] {
        &self.compares.history
    }

    /// Identify a real chooseCard subject made for a comparison, including inside its begin/after skills.
    pub fn compare_choice(&self, subject: EventId) -> Option<CompareParticipants> {
        match self.scheduler.active_frame(subject)?.1 {
            Program::Compare(Flow::Bid(seats, ..)) => Some(*seats),
            _ => None,
        }
    }

    /// Trusted chooseCardAfter content can read this chooser's validated result.
    pub fn selected_compare_card(&self, subject: EventId) -> Option<Card> {
        match self.scheduler.active_frame(subject)?.1 {
            Program::Compare(Flow::Bid(_, _, Bid::Finish(card))) => Some(*card),
            _ => None,
        }
    }

    /// The two cards locked by the participants. Before reveal these are private
    /// to trusted rule content; player and spectator projections use the reveal event.
    pub fn compare_materials(&self, subject: EventId) -> Option<[Card; 2]> {
        match self.scheduler.active_frame(subject).map(|(_, p)| p) {
            Some(Program::Compare(Flow::Compare(Comparing {
                stage: Stage::Paid(cards) | Stage::Reveal(cards) | Stage::Score(cards),
                ..
            }))) => Some(*cards),
            Some(Program::Compare(Flow::Compare(Comparing {
                stage: Stage::Cancelled(cards),
                ..
            }))) => *cards,
            _ => self.comparison(subject).map(|r| r.cards),
        }
    }

    pub(crate) fn change_compare_number(
        &mut self,
        event: &crate::TriggerEvent,
        seat: SeatId,
        number: i32,
    ) -> Result<(), RuleError> {
        if !matches!(event.timing.as_str(), "compare" | "compareFixing") {
            return Err(RuleError::WrongEvent);
        }
        let comparison = &mut self
            .compares
            .live
            .get_mut(&event.subject)
            .ok_or(RuleError::WrongEvent)?
            .result;
        let index = comparison
            .participants
            .seats()
            .iter()
            .position(|&s| s == seat)
            .ok_or(RuleError::InvalidTarget)?;
        comparison.numbers[index] = number;
        comparison.score();
        Ok(())
    }

    /// True while this comparison is awarding its result and this bid is in ordering or discard.
    pub fn can_gain_compare_card(&self, subject: EventId, seat: SeatId, id: CardId) -> bool {
        self.compare_gain_program(subject, seat, id).is_ok()
    }

    pub(crate) fn compare_gain_program(
        &self,
        subject: EventId,
        seat: SeatId,
        id: CardId,
    ) -> Result<Program, RuleError> {
        if !matches!(
            self.scheduler.active_frame(subject),
            Some((
                _,
                Program::Compare(Flow::Compare(Comparing {
                    stage: Stage::Finish,
                    ..
                }))
            ))
        ) {
            return Err(RuleError::WrongEvent);
        }
        if !self.seat(seat)?.alive {
            return Err(RuleError::InactiveSeat { seat });
        }
        let (_, card) = self
            .available_compare_bid(subject, id)
            .ok_or(RuleError::InvalidCard)?;
        Ok(self.gain_compare_program(subject, seat, card))
    }

    fn available_compare_bid(&self, subject: EventId, id: CardId) -> Option<(usize, Card)> {
        let scored = self.compares.live.get(&subject)?;
        let index = scored.result.cards.iter().position(|c| c.id == id)?;
        let card = scored.result.cards[index];
        (!scored.gained[index] && (self.resolving.contains(&card) || self.discard.contains(&card)))
            .then_some((index, card))
    }

    pub(crate) fn take_compare_bid(&mut self, subject: EventId, id: CardId) -> Option<Card> {
        let (index, card) = self.available_compare_bid(subject, id)?;
        self.compares.live.get_mut(&subject)?.gained[index] = true;
        Some(if self.resolving.contains(&card) {
            self.take_resolving(id)
        } else {
            Self::remove_card(&mut self.discard, id)
        })
    }

    pub(crate) fn place_compare_bid(&mut self, comparison: EventId, owner: SeatId, card: Card) {
        self.resolving.push(card);
        self.compares.ordering.insert(
            card.id,
            HeldBid {
                comparison,
                revealed: false,
            },
        );
        // Losing an unknown hand card obscures this payer's entire remaining
        // hand: keeping one old mark would identify which card was *not* paid.
        if !self.public_hand_cards.is_empty() {
            let player = self
                .seats
                .iter()
                .find(|p| p.id == owner)
                .expect("validated payer");
            for held in &player.hand {
                self.public_hand_cards.remove(&held.id);
            }
        }
    }

    fn queue_compare_hook(&mut self, timing: &'static str, seats: CompareParticipants) {
        self.queue_timing(
            timing,
            Some(seats.initiator),
            None,
            Some(seats.target),
            0,
            &[],
        );
    }

    pub(crate) fn resolve_compare(
        &mut self,
        event: EventRef,
        flow: Flow,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        if self.match_result().is_some() {
            match flow {
                Flow::Cleanup(..)
                | Flow::Discard(..)
                | Flow::Compare(Comparing {
                    stage: Stage::After | Stage::Finish | Stage::Cancelled(_),
                    ..
                }) => {}
                _ => {
                    self.compares.live.remove(&event.id);
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
            }
        }
        match flow {
            Flow::Compare(compare) => self.advance_compare(event, compare, child),
            Flow::Collect(seats, stage) => self.collect_compare_cards(seats, stage, child),
            Flow::Bid(seats, index, stage) => {
                self.choose_compare_card(event, seats, index, stage, child, answer)
            }
            Flow::Lose(seats, cards) => {
                // The participants' locked cards belong to the comparison. Each
                // ordinary lose child reports only what it physically moved.
                let comparison = event.parent.expect("comparison parent");
                for (seat, card) in seats.seats().into_iter().zip(cards) {
                    self.scheduler
                        .next(self.lose_compare_program(comparison, seat, card));
                }
                self.finish();
            }
            Flow::Show(seat, card) => {
                self.queue_timing("showCardsAfter", Some(seat), None, None, 1, &[card]);
                self.finish();
            }
            Flow::Cleanup(cards) => {
                self.cleanup_compare(event.parent.expect("comparison parent"), cards)
            }
            Flow::Discard(cards, moved) => {
                if moved {
                    let discarded: Vec<_> = cards.into_iter().flatten().collect();
                    self.queue_timing(
                        "cardsDiscardAfter",
                        None,
                        None,
                        None,
                        discarded.len() as u16,
                        &discarded,
                    );
                    self.finish();
                } else {
                    for card in cards.into_iter().flatten() {
                        self.next(EventKind::Discard { card });
                    }
                    self.scheduler
                        .replace_current(Program::Compare(Flow::Discard(cards, true)));
                }
            }
        }
    }

    fn advance_compare(
        &mut self,
        event: EventRef,
        mut compare: Comparing,
        child: Option<Outcome<Resolution>>,
    ) {
        use Stage::*;
        let seats = compare.seats;
        if matches!(child, Some(Outcome::Cancelled))
            && !matches!(compare.stage, Finish | Cancelled(_))
        {
            compare.stage = Cancelled(self.compare_materials(event.id));
            self.scheduler
                .replace_current(Program::Compare(Flow::Compare(compare)));
            self.queue_compare_hook("chooseToCompareCancelled", seats);
            return;
        }
        compare.stage = match compare.stage {
            Begin => {
                self.queue_compare_hook("chooseToCompareBegin", seats);
                Collect
            }
            Collect => {
                if !self.can_compare(seats.initiator, seats.target) {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                self.scheduler
                    .next(Program::Compare(Flow::Collect(seats, Collection::Begin)));
                Collected
            }
            Collected => {
                let Some(Outcome::Finished(Resolution::CompareBids(cards))) = child else {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                };
                if cards.iter().any(|card| self.face(card.id).is_none()) {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                self.scheduler
                    .next(Program::Compare(Flow::Lose(seats, cards)));
                self.scheduler.after(Program::Compare(Flow::Cleanup(cards)));
                Paid(cards)
            }
            Paid(cards) => {
                self.queue_compare_hook("compareCardShowBefore", seats);
                Reveal(cards)
            }
            Reveal(cards) => {
                for card in cards {
                    if let Some(bid) = self
                        .compares
                        .ordering
                        .get_mut(&card.id)
                        .filter(|bid| bid.comparison == event.id)
                    {
                        bid.revealed = true;
                    }
                }
                self.next(EventKind::CompareReveal {
                    seat: seats.initiator,
                    target: seats.target,
                    cards,
                });
                for (seat, card) in seats.seats().into_iter().zip(cards) {
                    self.scheduler
                        .next(Program::Compare(Flow::Show(seat, card)));
                }
                Score(cards)
            }
            Score(cards) => {
                let numbers = cards.map(|card| {
                    i32::from(self.face(card.id).expect("validated physical bid").rank)
                });
                let mut result = CompareResult {
                    event: event.id,
                    parent: event.parent,
                    turn: self
                        .classic
                        .as_ref()
                        .and_then(|table| table.turn)
                        .map(|turn| turn.number),
                    participants: seats,
                    cards,
                    numbers,
                    won: false,
                    tie: false,
                    winner: None,
                };
                result.score();
                self.compares.live.insert(
                    event.id,
                    Scored {
                        result,
                        gained: [false; 2],
                    },
                );
                self.queue_compare_hook("compare", seats);
                Fix
            }
            Fix => {
                self.queue_compare_hook("compareFixing", seats);
                Announce
            }
            Announce => {
                let result = self.compares.live[&event.id].result;
                self.next(EventKind::CompareResult { result });
                After
            }
            After => {
                // The eligibility check for claiming a bid runs even when a filter
                // queries this frame synchronously during the timing request.
                compare.stage = Finish;
                self.scheduler
                    .replace_current(Program::Compare(Flow::Compare(compare)));
                self.queue_compare_hook("chooseToCompareAfter", seats);
                return;
            }
            Finish => {
                let result = self
                    .compares
                    .live
                    .remove(&event.id)
                    .expect("public comparison")
                    .result;
                self.compares.history.push(result);
                self.scheduler
                    .end(Outcome::Finished(Resolution::Compared(result)));
                return;
            }
            Cancelled(_) => {
                self.compares.live.remove(&event.id);
                self.scheduler.end(Outcome::Cancelled);
                return;
            }
        };
        self.scheduler
            .replace_current(Program::Compare(Flow::Compare(compare)));
    }

    fn collect_compare_cards(
        &mut self,
        seats: CompareParticipants,
        stage: Collection,
        child: Option<Outcome<Resolution>>,
    ) {
        use Collection::*;
        let stage = match (stage, child) {
            (_, Some(Outcome::Cancelled)) => {
                self.scheduler.end(Outcome::Cancelled);
                return;
            }
            (Begin, _) => {
                self.queue_compare_hook("chooseCardOLBegin", seats);
                First
            }
            (First, _) => {
                self.scheduler
                    .next(Program::Compare(Flow::Bid(seats, 0, Bid::Begin)));
                FirstChosen
            }
            (FirstChosen, Some(Outcome::Finished(Resolution::CompareBid(first)))) => {
                self.scheduler
                    .next(Program::Compare(Flow::Bid(seats, 1, Bid::Begin)));
                SecondChosen(first)
            }
            (SecondChosen(first), Some(Outcome::Finished(Resolution::CompareBid(second)))) => {
                let cards = [first, second];
                self.queue_compare_hook("chooseCardOLAfter", seats);
                Finish(cards)
            }
            (Finish(cards), _) => {
                self.scheduler
                    .end(Outcome::Finished(Resolution::CompareBids(cards)));
                return;
            }
            _ => unreachable!("a completed compare bid returns its selected hand card"),
        };
        self.scheduler
            .replace_current(Program::Compare(Flow::Collect(seats, stage)));
    }

    fn choose_compare_card(
        &mut self,
        event: EventRef,
        seats: CompareParticipants,
        index: usize,
        stage: Bid,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        let [seat, other] = [seats.seats()[index], seats.seats()[1 - index]];
        if matches!(child, Some(Outcome::Cancelled)) {
            self.scheduler.end(Outcome::Cancelled);
            return;
        }
        match stage {
            Bid::Begin => {
                self.queue_timing(
                    "chooseCardBegin",
                    Some(seat),
                    Some(seats.initiator),
                    None,
                    0,
                    &[],
                );
                self.scheduler.replace_current(Program::Compare(Flow::Bid(
                    seats,
                    index,
                    Bid::Select,
                )));
            }
            Bid::Select => {
                let player = self.seat(seat).expect("compare participant");
                if !player.alive || player.hand.is_empty() {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                if let Some(Answer::Select(selection)) = answer.take() {
                    let card = *player
                        .hand
                        .iter()
                        .find(|card| selection.cards == [card.id])
                        .expect("a compare bid is one validated hand card");
                    self.scheduler.replace_current(Program::Compare(Flow::Bid(
                        seats,
                        index,
                        Bid::Finish(card),
                    )));
                    self.queue_timing(
                        "chooseCardAfter",
                        Some(seat),
                        Some(seats.initiator),
                        None,
                        0,
                        &[],
                    );
                } else {
                    self.decision = Some(Decision::Cards(ChoiceView {
                        event: event.id,
                        seat,
                        target: Some(other),
                        kind: ChoiceKind::Content,
                        min: 0,
                        max: 0,
                        cards: vec![],
                        counter: None,
                        compare: Some(seats),
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

    fn cleanup_compare(&mut self, comparison: EventId, cards: [Card; 2]) {
        let remaining = cards.map(|card| {
            self.compares
                .owns_ordering(comparison, card.id)
                .then_some(card)
        });
        if remaining.iter().any(Option::is_some) {
            self.scheduler
                .next(Program::Compare(Flow::Discard(remaining, false)));
        }
        self.finish();
    }
}

#[cfg(test)]
mod tests;
