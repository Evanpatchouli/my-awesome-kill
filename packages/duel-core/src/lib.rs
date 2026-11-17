mod classic;
pub mod classic_data;
mod protocol;
mod scheduler;

use std::collections::{HashMap, HashSet};

use classic::equipment;
use classic::tricks::Trick;
use classic::{Classic, MatchResult};
use classic_data::{General, Role};
pub use protocol::*;

use scheduler::{EventRef, Outcome, Scheduler, Work};

#[derive(Clone)]
enum Resolution {
    Complete,
    Passed,
    Dodged,
    PhaseFinished(Phase),
    SavePassed(SeatId),
    Selected(Selection),
    Responded,
    Judged(Card),
    Revealed(Card),
    HpChanged,
    ForcedHit,
}

enum Answer {
    Card(Card),
    Pass,
    Play(Program),
    EndPlay,
    Discard(Vec<CardId>),
    Select(Selection),
}

#[derive(Clone)]
struct Selection {
    branch: u32,
    cards: Vec<CardId>,
    targets: Vec<SeatId>,
}

#[derive(Clone)]
struct ChoiceRequest {
    seat: SeatId,
    target: Option<SeatId>,
    branches: Vec<ChoiceBranch>,
}

enum Decision {
    Play(SeatId),
    Cards(ChoiceView),
}

impl Decision {
    fn seat(&self) -> SeatId {
        match self {
            Self::Play(seat) => *seat,
            Self::Cards(choice) => choice.seat,
        }
    }
}

#[derive(Clone, Copy)]
enum Zone {
    Hand,
    Equipment,
    Judgment,
}

#[derive(Clone)]
enum Program {
    Event(EventKind),
    DiscardFrom {
        seat: SeatId,
        zone: Zone,
        card: Card,
    },
    Choose(ChoiceRequest),
    Judge {
        seat: SeatId,
    },
    Equipment(equipment::Flow),
    Trick(Trick),
}

impl Program {
    fn event(kind: EventKind) -> Self {
        Self::Event(kind)
    }

    fn kind(&self) -> EventKind {
        match self {
            Self::Event(kind) => *kind,
            Self::DiscardFrom { card, .. } => EventKind::Discard { card: *card },
            Self::Choose(choice) => EventKind::ChooseContent {
                seat: choice.seat,
                target: choice.target,
            },
            Self::Judge { seat } => EventKind::Judge { seat: *seat },
            Self::Equipment(flow) => flow.kind(),
            Self::Trick(flow) => flow.kind(),
        }
    }

    fn respond(seat: SeatId, kind: CardKind) -> Self {
        Self::Equipment(equipment::Flow::response(seat, kind, false))
    }
}

struct Seat {
    id: SeatId,
    hp: i32,
    max_hp: u16,
    alive: bool,
    hero: Option<&'static General>,
    role: Option<Role>,
    hand: Vec<Card>,
    equipment: Vec<Card>,
    judges: Vec<Card>,
}

impl Seat {
    fn zone_mut(&mut self, zone: Zone) -> &mut Vec<Card> {
        match zone {
            Zone::Hand => &mut self.hand,
            Zone::Equipment => &mut self.equipment,
            Zone::Judgment => &mut self.judges,
        }
    }
}

impl From<SeatSetup> for Seat {
    fn from(setup: SeatSetup) -> Self {
        Self {
            id: setup.id,
            hp: i32::from(setup.hp),
            max_hp: setup.hp,
            alive: setup.hp > 0,
            hero: None,
            role: None,
            hand: setup.hand,
            equipment: Vec::new(),
            judges: Vec::new(),
        }
    }
}

pub struct Game {
    seats: Vec<Seat>,
    active_seat: SeatId,
    version: u32,
    resolving: Vec<Card>,
    discard: Vec<Card>,
    judgment_overrides: HashMap<CardId, CardKind>,
    decision: Option<Decision>,
    scheduler: Scheduler<Program, Resolution>,
    events: Vec<GameEvent>,
    classic: Option<Classic>,
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
        Ok(Self::with_seats(
            setup.seats.into_iter().map(Seat::from).collect(),
            setup.active_seat,
        ))
    }

    fn with_seats(seats: Vec<Seat>, active_seat: SeatId) -> Self {
        Self {
            seats,
            active_seat,
            version: 0,
            resolving: Vec::new(),
            discard: Vec::new(),
            judgment_overrides: HashMap::new(),
            decision: None,
            scheduler: Scheduler::new(),
            events: Vec::new(),
            classic: None,
        }
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

    /// Count the shortest path through the living seats in either direction.
    pub fn distance(&self, from: SeatId, to: SeatId) -> Result<usize, RuleError> {
        for id in [from, to] {
            if !self.seat(id)?.alive {
                return Err(RuleError::InactiveSeat { seat: id });
            }
        }
        let living = self.living_order_from(from);
        let clockwise = living
            .iter()
            .position(|part| *part == to)
            .expect("both seats live");
        let raw = clockwise.min(living.len() - clockwise);
        let rider = self.seat(from)?;
        let target = self.seat(to)?;
        let adjustment = i32::from(equipment::has_slot(target, equipment::Slot::DefenseHorse))
            - i32::from(equipment::has_slot(rider, equipment::Slot::AttackHorse));
        Ok(if raw == 0 {
            0
        } else {
            (raw as i32 + adjustment).max(1) as usize
        })
    }

    /// Return public information and the specified seat's own hand and choices.
    /// With no viewer, return the spectator view.
    pub fn view(&self, viewer: Option<SeatId>) -> Result<GameView, RuleError> {
        if let Some(seat) = viewer {
            self.seat(seat)?;
        }
        let state = self.classic.as_ref();
        let result = state.and_then(|state| state.result);
        let seats = self
            .seats
            .iter()
            .map(|seat| {
                let own = viewer == Some(seat.id);
                let role_visible =
                    own || !seat.alive || seat.role == Some(Role::Lord) || result.is_some();
                SeatView {
                    id: seat.id,
                    hp: seat.hp,
                    max_hp: seat.max_hp,
                    alive: seat.alive,
                    hero: seat.hero.map(|hero| hero.id.clone()),
                    role: seat.role.filter(|_| role_visible),
                    hand_count: seat.hand.len(),
                    hand: own.then(|| seat.hand.clone()),
                    equipment: seat.equipment.clone(),
                    judges: seat.judges.clone(),
                    judge_assignments: seat
                        .judges
                        .iter()
                        .filter_map(|card| {
                            self.judgment_overrides
                                .get(&card.id)
                                .map(|&kind| JudgmentAssignment { id: card.id, kind })
                        })
                        .collect(),
                }
            })
            .collect::<Vec<_>>();
        let (choice, actions) = match self.decision.as_ref() {
            Some(Decision::Cards(choice)) if viewer == Some(choice.seat) => {
                (Some(choice.clone()), vec![])
            }
            Some(Decision::Play(seat)) if viewer == Some(*seat) => (None, self.play_choices(*seat)),
            _ => (None, vec![]),
        };
        let card_faces = state
            .map(|state| self.visible_faces(state, &seats))
            .unwrap_or_default();
        Ok(GameView {
            version: self.version,
            active_seat: self.active_seat,
            human_seat: state.map(|state| state.human),
            turn: state.and_then(|state| state.turn),
            winner: result.and_then(MatchResult::winner),
            drawn: result == Some(MatchResult::Draw),
            seats,
            deck_count: state.map_or(0, |state| state.deck.len()),
            resolving: self.resolving.clone(),
            discard: self.discard.clone(),
            waiting_for: self.decision.as_ref().map(Decision::seat),
            choice,
            actions,
            virtual_plays: match self.decision.as_ref() {
                Some(Decision::Play(seat)) if viewer == Some(*seat) => self.virtual_plays(*seat),
                _ => vec![],
            },
            card_faces,
            events: self.events.clone(),
        })
    }

    fn visible_faces(&self, state: &Classic, seats: &[SeatView]) -> Vec<CardFace> {
        use EventKind::{
            Discard, Equip, Reveal, UseCost, UseResponseCard, UseSha, UseShaTargets, UseShan,
            UseTao, UseTrick,
        };

        let public_zones = self.resolving.iter().chain(&self.discard);
        let seat_zones = seats.iter().flat_map(|seat| {
            seat.hand
                .as_deref()
                .unwrap_or_default()
                .iter()
                .chain(&seat.equipment)
                .chain(&seat.judges)
        });
        let revealed_events = self.events.iter().filter_map(|event| match event.kind {
            UseSha { card, .. }
            | UseShaTargets { card, .. }
            | UseShan { card, .. }
            | UseTao { card, .. }
            | UseTrick { card, .. }
            | UseCost { card, .. }
            | UseResponseCard { card, .. }
            | Equip { card, .. }
            | Reveal { card, .. }
            | Discard { card } => Some(card.id),
            _ => None,
        });
        let visible: HashSet<_> = public_zones
            .chain(seat_zones)
            .map(|card| card.id)
            .chain(revealed_events)
            .collect();
        state
            .faces
            .iter()
            .filter(|face| visible.contains(&face.id))
            .copied()
            .collect()
    }

    /// Validate the command against the current state before running any events.
    /// A successful command runs through the next decision or until the table is idle.
    pub fn submit(&mut self, command: Command) -> Result<(), RuleError> {
        if self.match_result().is_some() {
            return Err(RuleError::GameOver);
        }
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
            Action::PlaySha { card, target } if self.classic.is_none() => {
                let use_sha = self.plan_sha(command.seat, card, target)?;
                self.scheduler.start(Program::event(use_sha));
                None
            }
            Action::PlaySha { card, target } => {
                self.checked_play_seat(command.seat)?;
                self.card_in_hand(command.seat, card, CardKind::Sha)?;
                Some(Answer::Play(self.plan_play(
                    command.seat,
                    card,
                    vec![target],
                )?))
            }
            Action::PlayCard { card, targets } => {
                Some(Answer::Play(self.plan_play(command.seat, card, targets)?))
            }
            Action::EndPlay => {
                self.checked_play_seat(command.seat)?;
                Some(Answer::EndPlay)
            }
            Action::Choose { event, card } => {
                Some(self.validate_choice(command.seat, event, card)?)
            }
            Action::Discard { event, cards } => {
                Some(self.validate_discard(command.seat, event, cards)?)
            }
            Action::Select {
                event,
                branch,
                cards,
                targets,
            } => Some(self.validate_selection(
                command.seat,
                event,
                Selection {
                    branch,
                    cards,
                    targets,
                },
            )?),
            Action::PlayVirtual {
                kind,
                cards,
                targets,
            } => Some(Answer::Play(self.plan_virtual(
                command.seat,
                kind,
                cards,
                targets,
            )?)),
        };
        self.decision = None;
        self.events.clear();
        self.run(answer);
        self.version = next_version;
        Ok(())
    }

    fn plan_sha(&self, seat: SeatId, card: CardId, target: SeatId) -> Result<EventKind, RuleError> {
        if self.decision.is_some() {
            return Err(RuleError::ChoicePending);
        }
        if self.active_seat != seat {
            return Err(RuleError::WrongActor {
                expected: self.active_seat,
            });
        }
        if target == seat || !self.seat(target)?.alive {
            return Err(RuleError::InvalidTarget);
        }
        let card = self.card_in_hand(seat, card, CardKind::Sha)?;
        Ok(EventKind::UseSha { seat, target, card })
    }

    fn checked_play_seat(&self, seat: SeatId) -> Result<(), RuleError> {
        match self.decision.as_ref() {
            Some(Decision::Play(actor)) if *actor == seat => Ok(()),
            Some(Decision::Play(actor)) => Err(RuleError::WrongActor { expected: *actor }),
            Some(Decision::Cards(_)) => Err(RuleError::ChoicePending),
            None => Err(RuleError::WrongPhase),
        }
    }

    fn checked_choice(&self, seat: SeatId, event: EventId) -> Result<&ChoiceView, RuleError> {
        let Some(Decision::Cards(choice)) = self.decision.as_ref() else {
            return Err(RuleError::NoChoice);
        };
        if choice.seat != seat {
            return Err(RuleError::WrongActor {
                expected: choice.seat,
            });
        }
        if choice.event != event {
            return Err(RuleError::WrongEvent);
        }
        Ok(choice)
    }

    fn validate_choice(
        &self,
        seat: SeatId,
        event: EventId,
        card: Option<CardId>,
    ) -> Result<Answer, RuleError> {
        let choice = self.checked_choice(seat, event)?;
        let expected = match choice.kind {
            ChoiceKind::Shan => CardKind::Shan,
            ChoiceKind::Save => CardKind::Tao,
            ChoiceKind::Discard | ChoiceKind::Content => return Err(RuleError::InvalidSelection),
        };
        match card {
            Some(card) if choice.cards.contains(&card) => {
                Ok(Answer::Card(self.card_in_hand(seat, card, expected)?))
            }
            Some(_) => Err(RuleError::InvalidCard),
            None => Ok(Answer::Pass),
        }
    }

    fn validate_discard(
        &self,
        seat: SeatId,
        event: EventId,
        cards: Vec<CardId>,
    ) -> Result<Answer, RuleError> {
        let choice = self.checked_choice(seat, event)?;
        if choice.kind != ChoiceKind::Discard || !(choice.min..=choice.max).contains(&cards.len()) {
            return Err(RuleError::InvalidSelection);
        }
        let unique: HashSet<_> = cards.iter().copied().collect();
        if unique.len() != cards.len() {
            return Err(RuleError::InvalidSelection);
        }
        if !cards.iter().all(|id| choice.cards.contains(id)) {
            return Err(RuleError::InvalidCard);
        }
        Ok(Answer::Discard(cards))
    }

    fn validate_selection(
        &self,
        seat: SeatId,
        event: EventId,
        selection: Selection,
    ) -> Result<Answer, RuleError> {
        let choice = self.checked_choice(seat, event)?;
        let Some(branch) = choice
            .branches
            .iter()
            .find(|branch| branch.id == selection.branch)
        else {
            return Err(RuleError::InvalidSelection);
        };
        if !Self::matches_branch(branch, &selection.cards, &selection.targets) {
            return Err(RuleError::InvalidSelection);
        }
        Ok(Answer::Select(selection))
    }

    fn matches_branch(branch: &ChoiceBranch, cards: &[CardId], targets: &[SeatId]) -> bool {
        fn valid<T: Copy + Eq + std::hash::Hash>(
            values: &[T],
            pool: &[T],
            min: usize,
            max: usize,
        ) -> bool {
            (min..=max).contains(&values.len())
                && values.iter().copied().collect::<HashSet<_>>().len() == values.len()
                && values.iter().all(|value| pool.contains(value))
        }

        valid(cards, &branch.cards, branch.min_cards, branch.max_cards)
            && valid(
                targets,
                &branch.targets,
                branch.min_targets,
                branch.max_targets,
            )
    }

    fn run(&mut self, mut answer: Option<Answer>) {
        loop {
            let Some(work) = self.scheduler.poll() else {
                if !self.start_next_turn() {
                    break;
                }
                continue;
            };
            match work {
                Work::Started(event, program) => {
                    self.record(event, program.kind(), EventStatus::Started)
                }
                Work::Run(event, program, child) => {
                    self.resolve(event, program, child, &mut answer)
                }
                Work::Ended(event, program, outcome) => {
                    let status = match outcome {
                        Outcome::Finished(_) => EventStatus::Finished,
                        Outcome::Cancelled => EventStatus::Cancelled,
                    };
                    self.record(event, program.kind(), status);
                }
            }
            if self.decision.is_some() {
                break;
            }
        }
    }

    fn resolve(
        &mut self,
        event: EventRef,
        program: Program,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        use EventKind::*;
        use Outcome::{Cancelled, Finished};
        use Resolution::{Complete, Dodged, Passed};

        let kind = program.kind();
        let program = match program {
            Program::Equipment(flow) => return self.resolve_equipment(event, flow, child),
            Program::Trick(flow) => return self.resolve_trick(event, flow, child),
            Program::Choose(choice) => return self.resolve_selection(event, choice, answer),
            Program::Judge { seat } => return self.resolve_judge(seat, child),
            program => program,
        };
        match (kind, child) {
            (UseSha { seat, target, card }, None) => {
                self.move_to_resolving(seat, card.id);
                self.next(ShaEffect { seat, target });
                self.scheduler.after(Program::event(Discard { card }));
            }
            (UseSha { .. }, Some(_)) => self.finish(),
            (ShaEffect { target, .. }, None) => self.next(ChooseShan { seat: target }),
            (ShaEffect { seat, target }, Some(Finished(Passed))) => self.next(Damage {
                source: Some(seat),
                target,
                amount: 1,
            }),
            (ShaEffect { .. }, Some(Finished(Dodged))) => {
                self.scheduler.end(Cancelled);
            }
            (ShaEffect { .. }, Some(Finished(Complete))) => self.finish(),
            (ChooseShan { seat }, None) => match answer.take() {
                Some(Answer::Pass) => self.scheduler.end(Finished(Passed)),
                Some(Answer::Card(card)) => self.next(UseShan { seat, card }),
                None => self.ask_for_card(event, seat, ChoiceKind::Shan, None, CardKind::Shan),
                _ => unreachable!("a shan choice receives one card or a pass"),
            },
            (ChooseShan { .. }, Some(Finished(Dodged))) => self.scheduler.end(Finished(Dodged)),
            (UseShan { seat, card }, None) => {
                self.move_to_resolving(seat, card.id);
                self.scheduler.after(Program::event(Discard { card }));
                self.scheduler.end(Finished(Dodged));
            }
            (UseTao { seat, target, card }, None) => {
                self.move_to_resolving(seat, card.id);
                self.next(Recover {
                    seat,
                    target,
                    amount: 1,
                });
                self.scheduler.after(Program::event(Discard { card }));
            }
            (UseTao { .. }, Some(_)) => self.finish(),
            (Recover { target, amount, .. }, None) => {
                self.queue_hp_change(target, i32::from(amount))
            }
            (Recover { .. }, Some(_)) => self.finish(),
            (
                Damage {
                    source,
                    target,
                    amount,
                },
                child,
            ) => self.resolve_damage(source, target, amount, child),
            (ChangeHp { seat, after, .. }, None) => {
                let classic = self.classic.is_some();
                let player = self.seat_mut(seat);
                player.hp = after;
                if !classic {
                    player.alive = after > 0;
                }
                self.scheduler.end(Finished(Resolution::HpChanged));
            }
            (Reveal { card, .. }, None) => self.scheduler.end(Finished(Resolution::Revealed(card))),
            (UseCost { seat, card }, None) => {
                self.move_to_resolving(seat, card.id);
                self.finish();
            }
            (Discard { card }, None) => {
                let card = match program {
                    Program::DiscardFrom { seat, zone, .. } => self.take_card(seat, zone, card.id),
                    _ => self.take_resolving(card.id),
                };
                self.discard.push(card);
                self.finish();
            }
            (kind, child) => self.resolve_classic(event, kind, child, answer),
        }
    }

    fn resolve_selection(
        &mut self,
        event: EventRef,
        choice: ChoiceRequest,
        answer: &mut Option<Answer>,
    ) {
        match answer.take() {
            Some(Answer::Select(selected)) => self
                .scheduler
                .end(Outcome::Finished(Resolution::Selected(selected))),
            None => {
                self.decision = Some(Decision::Cards(ChoiceView {
                    event: event.id,
                    seat: choice.seat,
                    kind: ChoiceKind::Content,
                    target: choice.target,
                    min: 0,
                    max: 0,
                    cards: vec![],
                    branches: choice.branches,
                }));
            }
            _ => unreachable!("a content choice receives a selection"),
        }
    }

    fn resolve_judge(&mut self, seat: SeatId, child: Option<Outcome<Resolution>>) {
        match child {
            None => {
                let Some(cards) = self.reveal_cards(1) else {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                };
                let card = cards[0];
                self.next(EventKind::Reveal { seat, card });
                self.scheduler
                    .after(Program::event(EventKind::Discard { card }));
            }
            Some(Outcome::Finished(Resolution::Revealed(card))) => {
                self.scheduler
                    .end(Outcome::Finished(Resolution::Judged(card)));
            }
            _ => unreachable!("a judge only waits for a revealed card"),
        }
    }

    fn resolve_damage(
        &mut self,
        source: Option<SeatId>,
        target: SeatId,
        amount: u16,
        child: Option<Outcome<Resolution>>,
    ) {
        match child {
            None => self.queue_hp_change(target, -i32::from(amount)),
            Some(Outcome::Finished(Resolution::HpChanged))
                if self.classic.is_some()
                    && self.seat(target).expect("the damage target exists").hp <= 0 =>
            {
                self.next(EventKind::Dying { source, target });
            }
            Some(_) => self.finish(),
        }
    }

    fn queue_hp_change(&mut self, target: SeatId, delta: i32) {
        let player = self.seat(target).expect("the health change target exists");
        let before = player.hp;
        let signed = before.saturating_add(delta).min(i32::from(player.max_hp));
        let after = if self.classic.is_some() {
            signed
        } else {
            signed.max(0)
        };
        self.next(EventKind::ChangeHp {
            seat: target,
            before,
            after,
        });
    }

    fn ask_for_card(
        &mut self,
        event: EventRef,
        seat: SeatId,
        kind: ChoiceKind,
        target: Option<SeatId>,
        card_kind: CardKind,
    ) {
        let cards = self
            .seat(seat)
            .expect("the actor was validated")
            .hand
            .iter()
            .filter(|card| card.kind == card_kind)
            .map(|card| card.id)
            .collect();
        self.decision = Some(Decision::Cards(ChoiceView {
            event: event.id,
            seat,
            kind,
            target,
            min: 0,
            max: 1,
            cards,
            branches: vec![],
        }));
    }

    fn next(&mut self, event: EventKind) {
        self.scheduler.next(Program::event(event));
    }

    fn finish(&mut self) {
        self.scheduler.end(Outcome::Finished(Resolution::Complete));
    }

    fn record(&mut self, event: EventRef, kind: EventKind, status: EventStatus) {
        self.events.push(GameEvent {
            id: event.id,
            parent: event.parent,
            status,
            kind,
        });
    }

    fn seat(&self, id: SeatId) -> Result<&Seat, RuleError> {
        self.seats
            .iter()
            .find(|seat| seat.id == id)
            .ok_or(RuleError::UnknownSeat { seat: id })
    }

    fn seat_mut(&mut self, id: SeatId) -> &mut Seat {
        self.seats
            .iter_mut()
            .find(|seat| seat.id == id)
            .expect("the seat was validated")
    }

    fn living_order_from(&self, start: SeatId) -> Vec<SeatId> {
        self.seat_order_from(start)
            .expect("the starting seat is present")
            .into_iter()
            .filter(|id| self.seat(*id).is_ok_and(|seat| seat.alive))
            .collect()
    }

    fn card_in_hand(&self, seat: SeatId, id: CardId, kind: CardKind) -> Result<Card, RuleError> {
        self.seat(seat)?
            .hand
            .iter()
            .find(|card| card.id == id && card.kind == kind)
            .copied()
            .ok_or(RuleError::InvalidCard)
    }

    fn remove_card(cards: &mut Vec<Card>, id: CardId) -> Card {
        let index = cards
            .iter()
            .position(|card| card.id == id)
            .expect("the card was validated");
        cards.remove(index)
    }

    fn take_card(&mut self, seat: SeatId, zone: Zone, id: CardId) -> Card {
        let card = Self::remove_card(self.seat_mut(seat).zone_mut(zone), id);
        if matches!(zone, Zone::Judgment) {
            self.judgment_overrides.remove(&id);
        }
        card
    }

    fn take_resolving(&mut self, id: CardId) -> Card {
        Self::remove_card(&mut self.resolving, id)
    }

    fn put_in_hand(&mut self, seat: SeatId, card: Card) {
        self.seat_mut(seat).hand.push(card);
    }

    fn judgment_kind(&self, card: Card) -> CardKind {
        self.judgment_overrides
            .get(&card.id)
            .copied()
            .unwrap_or(card.kind)
    }

    fn put_in_judgment(&mut self, seat: SeatId, card: Card, kind: CardKind) {
        if kind != card.kind {
            self.judgment_overrides.insert(card.id, kind);
        }
        self.seat_mut(seat).judges.push(card);
    }

    fn move_to_resolving(&mut self, seat: SeatId, id: CardId) {
        let zone = self.zone_for_held_card(seat, id);
        let card = self.take_card(seat, zone, id);
        self.resolving.push(card);
    }

    fn zone_for_held_card(&self, seat: SeatId, id: CardId) -> Zone {
        let player = self.seat(seat).expect("the actor exists");
        if player.hand.iter().any(|card| card.id == id) {
            Zone::Hand
        } else {
            Zone::Equipment
        }
    }
}
