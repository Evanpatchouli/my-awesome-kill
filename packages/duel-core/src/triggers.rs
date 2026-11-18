use crate::scheduler::Outcome;
use crate::{
    Card, CardId, ChoiceBranch, ChoiceRequest, EventId, Game, Program, Resolution, RuleError,
    SeatId, Zone,
};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

/// A registration handle, unique for the lifetime of a game.
pub type TriggerId = u64;
pub type TriggerFilter = Arc<dyn Fn(&Game, &TriggerEvent, SeatId) -> bool + Send + Sync>;
pub type TriggerEffect = Arc<
    dyn Fn(&mut TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep + Send + Sync,
>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The subject role that must match the registration owner.
pub enum TriggerRole {
    Player,
    Source,
    Target,
    Global,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// Execution group. Normal triggers are grouped by seat.
pub enum TriggerOrder {
    First,
    Normal,
    Last,
}
#[derive(Clone)]
/// A skill callback installed on a game. Higher priorities execute first within a group.
pub struct TriggerRegistration {
    pub owner: SeatId,
    pub skill: String,
    pub timing: String,
    pub role: TriggerRole,
    pub order: TriggerOrder,
    pub priority: i32,
    pub label: Option<String>,
    pub silent: bool,
    pub filter: TriggerFilter,
    pub effect: TriggerEffect,
}
#[derive(Clone, Debug)]
/// Snapshot of the subject event at the moment the timing is dispatched.
pub struct TriggerEvent {
    pub timing: String,
    pub subject: EventId,
    pub player: Option<SeatId>,
    pub source: Option<SeatId>,
    pub target: Option<SeatId>,
    pub amount: u16,
    pub cards: Vec<Card>,
}
#[derive(Clone, Debug)]
/// The validated response to a content choice.
pub struct TriggerSelection {
    pub branch: u32,
    pub cards: Vec<CardId>,
    pub targets: Vec<SeatId>,
}
/// The continuation to run once this stage's queued operations have settled.
#[derive(Clone)]
pub enum TriggerStep {
    Done,
    CancelSubject,
    Continue(u16),
    Choose {
        branches: Vec<ChoiceBranch>,
        then: u16,
    },
}
/// Checked operations available to a running registered skill.
pub struct TriggerContext<'a> {
    game: &'a mut Game,
    owner: SeatId,
    event: &'a TriggerEvent,
    operations: VecDeque<TriggerOperation>,
}
impl TriggerContext<'_> {
    pub fn game(&self) -> &Game {
        self.game
    }
    /// Add a registration while this event is being dispatched.
    pub fn register_trigger(
        &mut self,
        registration: TriggerRegistration,
    ) -> Result<TriggerId, RuleError> {
        self.game.register_trigger(registration)
    }
    /// Revoke a registration, including a pending candidate in this event.
    pub fn unregister_trigger(&mut self, id: TriggerId) -> bool {
        self.game.unregister_trigger(id)
    }
    pub fn owner(&self) -> SeatId {
        self.owner
    }
    pub fn event(&self) -> &TriggerEvent {
        self.event
    }
    /// Schedule one atomic hand payment. Each physical card can be promised once per stage.
    /// If an intervening skill moves a promised card, this skill ends before its continuation.
    pub fn discard_hand(&mut self, seat: SeatId, ids: &[CardId]) -> Result<(), RuleError> {
        let hand = &self.game.seat(seat)?.hand;
        let mut claimed = HashSet::new();
        for operation in &self.operations {
            if let TriggerOperation::DiscardHand { cards, .. } = operation {
                claimed.extend(cards.iter().map(|card| card.id));
            }
        }
        let mut cards = Vec::with_capacity(ids.len());
        for id in ids {
            if !claimed.insert(*id) {
                return Err(RuleError::InvalidSelection);
            }
            cards.push(
                *hand
                    .iter()
                    .find(|card| card.id == *id)
                    .ok_or(RuleError::InvalidCard)?,
            );
        }
        if !cards.is_empty() {
            self.operations
                .push_back(TriggerOperation::DiscardHand { seat, cards });
        }
        Ok(())
    }
    /// Enqueue a nested timing, returning whether a matching registration exists.
    pub fn emit(
        &mut self,
        timing: &'static str,
        player: Option<SeatId>,
        source: Option<SeatId>,
        target: Option<SeatId>,
        amount: u16,
        cards: &[Card],
    ) -> bool {
        let Some(event) = self
            .game
            .prepare_timing(timing, player, source, target, amount, cards)
        else {
            return false;
        };
        self.operations.push_back(TriggerOperation::Emit(event));
        true
    }
}

#[derive(Clone)]
enum TriggerOperation {
    DiscardHand { seat: SeatId, cards: Vec<Card> },
    Emit(TriggerEvent),
}

#[derive(Clone)]
enum RegisteredState {
    Call(u16),
    Choice(u16),
    Operations {
        remaining: VecDeque<TriggerOperation>,
        then: TriggerStep,
    },
}

#[derive(Clone)]
pub(crate) struct Registered {
    pub id: TriggerId,
    pub owner: SeatId,
    event: TriggerEvent,
    effect: TriggerEffect,
    state: RegisteredState,
}

impl Registered {
    pub(crate) fn new(
        id: TriggerId,
        owner: SeatId,
        event: TriggerEvent,
        effect: TriggerEffect,
    ) -> Self {
        Self {
            id,
            owner,
            event,
            effect,
            state: RegisteredState::Call(0),
        }
    }
}
impl Game {
    /// Register a callback; the owner must be a seat in this game.
    pub fn register_trigger(
        &mut self,
        registration: TriggerRegistration,
    ) -> Result<TriggerId, RuleError> {
        self.seat(registration.owner)?;
        self.next_trigger_id = self
            .next_trigger_id
            .checked_add(1)
            .ok_or(RuleError::VersionExhausted)?;
        let id = self.next_trigger_id;
        self.trigger_registrations.insert(id, registration);
        Ok(id)
    }
    /// Revoke a callback, including any pending, unconsumed occurrence.
    pub fn unregister_trigger(&mut self, id: TriggerId) -> bool {
        self.trigger_registrations.remove(&id).is_some()
    }
    pub(crate) fn has_timing_registrations(&self, timing: &str) -> bool {
        self.trigger_registrations
            .values()
            .any(|r| r.timing == timing)
    }
    pub(crate) fn queue_timing(
        &mut self,
        timing: &'static str,
        player: Option<SeatId>,
        source: Option<SeatId>,
        target: Option<SeatId>,
        amount: u16,
        cards: &[Card],
    ) -> bool {
        let Some(event) = self.prepare_timing(timing, player, source, target, amount, cards) else {
            return false;
        };
        self.scheduler
            .next(Program::Dispatch(self.dispatch_for_timing(event)));
        true
    }

    fn prepare_timing(
        &self,
        timing: &'static str,
        player: Option<SeatId>,
        source: Option<SeatId>,
        target: Option<SeatId>,
        amount: u16,
        cards: &[Card],
    ) -> Option<TriggerEvent> {
        if !self.has_timing_registrations(timing) {
            return None;
        }
        let event = TriggerEvent {
            timing: timing.into(),
            subject: self.scheduler.current_id(),
            player,
            source,
            target,
            amount,
            cards: cards.to_vec(),
        };
        self.has_registered_candidates(&event).then_some(event)
    }
    pub(crate) fn registered_matches(&self, id: TriggerId, event: &TriggerEvent) -> bool {
        let Some(r) = self.trigger_registrations.get(&id) else {
            return false;
        };
        self.registration_applies(r, event) && (r.filter)(self, event, r.owner)
    }
    fn registration_applies(&self, r: &TriggerRegistration, event: &TriggerEvent) -> bool {
        r.timing == event.timing
            && self.seat(r.owner).is_ok_and(|s| s.alive)
            && match r.role {
                TriggerRole::Player => event.player == Some(r.owner),
                TriggerRole::Source => event.source == Some(r.owner),
                TriggerRole::Target => event.target == Some(r.owner),
                TriggerRole::Global => true,
            }
    }
    pub(crate) fn has_registered_candidates(&self, event: &TriggerEvent) -> bool {
        self.trigger_registrations
            .values()
            .any(|r| self.registration_applies(r, event))
    }
    pub(crate) fn resolve_registered(
        &mut self,
        mut flow: Registered,
        child: Option<Outcome<Resolution>>,
    ) {
        let (stage, selected) = match std::mem::replace(&mut flow.state, RegisteredState::Call(0)) {
            RegisteredState::Operations { remaining, then } => {
                return self.advance_registered(flow, remaining, then);
            }
            RegisteredState::Call(stage) => (stage, None),
            RegisteredState::Choice(stage) => match child {
                Some(Outcome::Finished(Resolution::Selected(s))) => (
                    stage,
                    Some(TriggerSelection {
                        branch: s.branch,
                        cards: s.cards,
                        targets: s.targets,
                    }),
                ),
                _ => {
                    self.finish();
                    return;
                }
            },
        };
        let mut context = TriggerContext {
            game: self,
            owner: flow.owner,
            event: &flow.event,
            operations: VecDeque::new(),
        };
        let then = (flow.effect)(&mut context, stage, selected.as_ref());
        let operations = context.operations;
        self.advance_registered(flow, operations, then);
    }

    fn advance_registered(
        &mut self,
        mut flow: Registered,
        mut operations: VecDeque<TriggerOperation>,
        then: TriggerStep,
    ) {
        if let Some(operation) = operations.pop_front() {
            // Earlier children may have triggered other skills or paused for a decision.
            // Recheck the entire next payment before allowing any card in it to move.
            let program = match operation {
                TriggerOperation::DiscardHand { seat, cards } => {
                    if !self
                        .seat(seat)
                        .is_ok_and(|player| cards.iter().all(|card| player.hand.contains(card)))
                    {
                        self.finish();
                        return;
                    }
                    Program::DiscardGroup {
                        seat,
                        cards: cards.into_iter().map(|card| (Zone::Hand, card)).collect(),
                    }
                }
                TriggerOperation::Emit(event) => Program::Dispatch(self.dispatch_for_timing(event)),
            };
            flow.state = RegisteredState::Operations {
                remaining: operations,
                then,
            };
            self.scheduler.replace_current(Program::Registered(flow));
            self.scheduler.next(program);
            return;
        }
        match then {
            TriggerStep::Done => self.finish(),
            TriggerStep::CancelSubject => self.scheduler.end(Outcome::Cancelled),
            TriggerStep::Continue(next) => {
                flow.state = RegisteredState::Call(next);
                self.scheduler.replace_current(Program::Registered(flow));
            }
            TriggerStep::Choose { branches, then } => {
                flow.state = RegisteredState::Choice(then);
                let seat = flow.owner;
                self.scheduler.replace_current(Program::Registered(flow));
                self.scheduler.next(Program::Choose(ChoiceRequest {
                    seat,
                    target: None,
                    branches,
                }));
            }
        }
    }
}
