mod protocol;
mod scheduler;

use std::collections::HashSet;

pub use protocol::*;

use scheduler::{EventRef, Outcome, Scheduler, Work};

#[derive(Clone, Copy)]
enum Resolution {
    Complete,
    Passed,
    Dodged,
}

#[derive(Clone, Copy)]
enum Answer {
    Play(Card),
    Pass,
}

pub struct Game {
    seats: Vec<SeatSetup>,
    active_seat: SeatId,
    version: u32,
    resolving: Vec<Card>,
    discard: Vec<Card>,
    choice: Option<ChoiceView>,
    scheduler: Scheduler<EventKind, Resolution>,
    events: Vec<GameEvent>,
}

impl Game {
    pub fn new(setup: GameSetup) -> Result<Self, RuleError> {
        if setup.seats.len() < 2 {
            return Err(RuleError::NotEnoughSeats);
        }
        let mut seats = HashSet::new();
        let mut cards = HashSet::new();
        for seat in &setup.seats {
            if !seats.insert(seat.id) {
                return Err(RuleError::DuplicateSeat { seat: seat.id });
            }
            for card in &seat.hand {
                if !cards.insert(card.id) {
                    return Err(RuleError::DuplicateCard { card: card.id });
                }
            }
        }
        let active = setup
            .seats
            .iter()
            .find(|seat| seat.id == setup.active_seat)
            .ok_or(RuleError::UnknownSeat {
                seat: setup.active_seat,
            })?;
        if active.hp == 0 {
            return Err(RuleError::InactiveSeat { seat: active.id });
        }
        Ok(Self {
            seats: setup.seats,
            active_seat: setup.active_seat,
            version: 0,
            resolving: Vec::new(),
            discard: Vec::new(),
            choice: None,
            scheduler: Scheduler::new(),
            events: Vec::new(),
        })
    }

    /// Walk the table once, starting at `start` and following the setup order.
    pub fn seat_order_from(&self, start: SeatId) -> Result<Vec<SeatId>, RuleError> {
        let index = self
            .seats
            .iter()
            .position(|seat| seat.id == start)
            .ok_or(RuleError::UnknownSeat { seat: start })?;
        let (before, from_start) = self.seats.split_at(index);
        Ok(from_start
            .iter()
            .chain(before)
            .map(|seat| seat.id)
            .collect())
    }

    /// Return public information and the specified seat's own hand and choices.
    /// With no viewer, return the spectator view.
    pub fn view(&self, viewer: Option<SeatId>) -> Result<GameView, RuleError> {
        if let Some(seat) = viewer {
            self.seat(seat)?;
        }
        let choice = self.choice.as_ref();
        Ok(GameView {
            version: self.version,
            active_seat: self.active_seat,
            seats: self
                .seats
                .iter()
                .map(|seat| SeatView {
                    id: seat.id,
                    hp: seat.hp,
                    hand_count: seat.hand.len(),
                    hand: (viewer == Some(seat.id)).then(|| seat.hand.clone()),
                })
                .collect(),
            resolving: self.resolving.clone(),
            discard: self.discard.clone(),
            waiting_for: choice.map(|choice| choice.seat),
            choice: choice.filter(|choice| viewer == Some(choice.seat)).cloned(),
            events: self.events.clone(),
        })
    }

    /// Validate the command against the current state before running any events.
    /// A successful command runs through the next decision or until the table is idle.
    pub fn submit(&mut self, command: Command) -> Result<(), RuleError> {
        if command.version != self.version {
            return Err(RuleError::WrongVersion {
                expected: self.version,
                received: command.version,
            });
        }
        let next_version = self
            .version
            .checked_add(1)
            .ok_or(RuleError::VersionExhausted)?;
        let answer = match command.action {
            Action::PlaySha { card, target } => {
                let use_sha = self.plan_sha(command.seat, card, target)?;
                self.scheduler.start(use_sha);
                None
            }
            Action::Choose { event, card } => {
                let answer = self.validate_choice(command.seat, event, card)?;
                self.choice = None;
                Some(answer)
            }
        };
        self.run(answer);
        self.version = next_version;
        Ok(())
    }

    fn plan_sha(&self, seat: SeatId, card: CardId, target: SeatId) -> Result<EventKind, RuleError> {
        if self.choice.is_some() {
            return Err(RuleError::ChoicePending);
        }
        if self.active_seat != seat {
            return Err(RuleError::WrongActor {
                expected: self.active_seat,
            });
        }
        if target == seat || self.seat(target)?.hp == 0 {
            return Err(RuleError::InvalidTarget);
        }
        let card = self.card_in_hand(seat, card, CardKind::Sha)?;
        Ok(EventKind::UseSha { seat, target, card })
    }

    fn validate_choice(
        &self,
        seat: SeatId,
        event: EventId,
        card: Option<CardId>,
    ) -> Result<Answer, RuleError> {
        let choice = self.choice.as_ref().ok_or(RuleError::NoChoice)?;
        if choice.seat != seat {
            return Err(RuleError::WrongActor {
                expected: choice.seat,
            });
        }
        if choice.event != event {
            return Err(RuleError::WrongEvent);
        }
        if let Some(card) = card {
            if !choice.cards.contains(&card) {
                return Err(RuleError::InvalidCard);
            }
            Ok(Answer::Play(self.card_in_hand(
                seat,
                card,
                CardKind::Shan,
            )?))
        } else {
            Ok(Answer::Pass)
        }
    }

    fn run(&mut self, mut answer: Option<Answer>) {
        while let Some(work) = self.scheduler.poll() {
            match work {
                Work::Started(event, kind) => self.record(event, kind, EventStatus::Started),
                Work::Run(event, kind, child) => self.resolve(event, kind, child, &mut answer),
                Work::Ended(event, kind, outcome) => {
                    let status = match outcome {
                        Outcome::Finished(_) => EventStatus::Finished,
                        Outcome::Cancelled => EventStatus::Cancelled,
                    };
                    self.record(event, kind, status);
                }
            }
            if self.choice.is_some() {
                break;
            }
        }
    }

    fn resolve(
        &mut self,
        event: EventRef,
        kind: EventKind,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        use EventKind::*;
        use Outcome::{Cancelled, Finished};
        use Resolution::{Complete, Dodged, Passed};

        match (kind, child) {
            (UseSha { seat, target, card }, None) => {
                self.move_to_resolving(seat, card.id);
                self.scheduler.next(ShaEffect { seat, target });
                self.scheduler.after(Discard { card });
            }
            (UseSha { .. }, Some(_)) => self.scheduler.end(Finished(Complete)),
            (ShaEffect { target, .. }, None) => self.scheduler.next(ChooseShan { seat: target }),
            (ShaEffect { seat, target }, Some(Finished(Passed))) => self.scheduler.next(Damage {
                source: seat,
                target,
                amount: 1,
            }),
            (ShaEffect { .. }, Some(Finished(Dodged))) => {
                self.scheduler.end(Cancelled);
            }
            (ShaEffect { .. }, Some(Finished(Complete))) => self.scheduler.end(Finished(Complete)),
            (ChooseShan { seat }, None) => match answer.take() {
                Some(Answer::Pass) => self.scheduler.end(Finished(Passed)),
                Some(Answer::Play(card)) => self.scheduler.next(UseShan { seat, card }),
                None => {
                    let cards = self
                        .seat(seat)
                        .expect("the attack target was validated")
                        .hand
                        .iter()
                        .filter(|card| card.kind == CardKind::Shan)
                        .map(|card| card.id)
                        .collect();
                    self.choice = Some(ChoiceView {
                        event: event.id,
                        seat,
                        cards,
                    });
                }
            },
            (ChooseShan { .. }, Some(Finished(Dodged))) => self.scheduler.end(Finished(Dodged)),
            (UseShan { seat, card }, None) => {
                self.move_to_resolving(seat, card.id);
                self.scheduler.after(Discard { card });
                self.scheduler.end(Finished(Dodged));
            }
            (Damage { target, amount, .. }, None) => {
                let seat = self.seat_mut(target);
                seat.hp = seat.hp.saturating_sub(amount);
                self.scheduler.end(Finished(Complete));
            }
            (Discard { card }, None) => {
                let index = self
                    .resolving
                    .iter()
                    .position(|current| current.id == card.id)
                    .expect("the used card is still resolving");
                self.discard.push(self.resolving.remove(index));
                self.scheduler.end(Finished(Complete));
            }
            _ => unreachable!("this event cannot receive that child result"),
        }
    }

    fn record(&mut self, event: EventRef, kind: EventKind, status: EventStatus) {
        self.events.push(GameEvent {
            id: event.id,
            parent: event.parent,
            status,
            kind,
        });
    }

    fn seat(&self, id: SeatId) -> Result<&SeatSetup, RuleError> {
        self.seats
            .iter()
            .find(|seat| seat.id == id)
            .ok_or(RuleError::UnknownSeat { seat: id })
    }

    fn seat_mut(&mut self, id: SeatId) -> &mut SeatSetup {
        self.seats
            .iter_mut()
            .find(|seat| seat.id == id)
            .expect("the seat was validated")
    }

    fn card_in_hand(&self, seat: SeatId, id: CardId, kind: CardKind) -> Result<Card, RuleError> {
        self.seat(seat)?
            .hand
            .iter()
            .find(|card| card.id == id && card.kind == kind)
            .copied()
            .ok_or(RuleError::InvalidCard)
    }

    fn move_to_resolving(&mut self, seat: SeatId, id: CardId) {
        let hand = &mut self.seat_mut(seat).hand;
        let index = hand
            .iter()
            .position(|card| card.id == id)
            .expect("the card was validated");
        let card = hand.remove(index);
        self.resolving.push(card);
    }
}
