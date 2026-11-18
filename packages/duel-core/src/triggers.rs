use crate::scheduler::Outcome;
use crate::{
    Card, CardId, ChoiceBranch, ChoiceRequest, EventId, Game, Program, Resolution, RuleError,
    SeatId, Zone,
};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

/// A registration handle, unique for the lifetime of a game.
pub type TriggerId = u64;
pub type TriggerIndex = Arc<dyn Fn(&Game, &TriggerEvent, SeatId) -> u32 + Send + Sync>;
#[cfg(test)]
pub(crate) type TimingObserver = Arc<dyn Fn(&Game, &TriggerEvent) + Send + Sync>;
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
impl TriggerRole {
    pub(crate) fn matches(self, owner: SeatId, event: &TriggerEvent) -> bool {
        match self {
            Self::Player => event.player == Some(owner),
            Self::Source => event.source == Some(owner),
            Self::Target => event.target == Some(owner),
            Self::Global => true,
        }
    }
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
    pub parent: Option<EventId>,
    pub player: Option<SeatId>,
    pub source: Option<SeatId>,
    pub target: Option<SeatId>,
    pub amount: u16,
    pub cards: Vec<Card>,
    pub card_kind: Option<crate::CardKind>,
    pub nature: Option<crate::DamageNature>,
    /// Signed change on changeHp and changeHujia subjects.
    pub delta: Option<i32>,
}

impl TriggerEvent {
    /// The subject's numeric argument when its rules define one. `amount` separately
    /// counts physical cards on movement callbacks, e.g. a loss of three cards has num 0.
    pub fn subject_number(&self, game: &Game) -> Option<i32> {
        use crate::EventKind::*;
        match game.event_kind(self.subject)? {
            Damage { .. } | LoseHp { .. } | Draw { .. } => Some(i32::from(self.amount)),
            Lose { .. } | LoseToDiscardpile { .. } | ActivateSkill { .. } => Some(0),
            Turn { .. } if self.timing == "roundStart" => Some(0),
            _ => self.delta,
        }
    }
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
    last_compare: Option<crate::CompareResult>,
    operations: VecDeque<TriggerOperation>,
}
impl TriggerContext<'_> {
    /// Set the damage a paid Sha, Duel or mass attack will start with for each affected target.
    pub fn set_base_damage(&mut self, amount: u16) -> Result<(), RuleError> {
        if self.event.timing != "useCard2" {
            return Err(RuleError::WrongEvent);
        }
        let base = match self.game.scheduler.active_program_mut(self.event.subject) {
            Some(Program::Equipment(crate::equipment::Flow::Attack(attack))) => {
                attack.base_damage_mut()
            }
            Some(Program::Trick(trick)) => trick.base_damage_mut(),
            _ => None,
        }
        .ok_or(RuleError::WrongEvent)?;
        *base = amount;
        Ok(())
    }
    pub fn add_skill_mark(
        &mut self,
        seat: SeatId,
        skill: &str,
        source: Option<SeatId>,
        count: u32,
    ) -> Result<(), RuleError> {
        self.game.add_skill_mark(seat, skill, source, count)
    }
    pub fn remove_skill_mark(
        &mut self,
        seat: SeatId,
        skill: &str,
        source: Option<SeatId>,
        count: u32,
    ) -> Result<(), RuleError> {
        self.game.remove_skill_mark(seat, skill, source, count)
    }
    pub fn clear_skill_mark(&mut self, seat: SeatId, skill: &str, source: Option<SeatId>) {
        self.game.clear_skill_mark(seat, skill, source);
    }
    pub fn skip_next_phase(&mut self, seat: SeatId, phase: crate::Phase) -> Result<(), RuleError> {
        self.game.skip_next_phase(seat, phase)
    }

    pub fn insert_next_phase(
        &mut self,
        phase: crate::Phase,
        reason: &str,
    ) -> Result<(), RuleError> {
        self.game.insert_next_phase(self.event, phase, reason)
    }

    pub fn insert_extra_turn(&mut self, seat: SeatId, reason: &str) -> Result<(), RuleError> {
        self.game.insert_extra_turn(seat, reason)
    }

    pub fn insert_extra_turn_at(
        &mut self,
        seat: SeatId,
        reason: &str,
        position: crate::TurnPosition,
    ) -> Result<(), RuleError> {
        self.game.insert_extra_turn_at(seat, reason, position)
    }

    /// Increase the current damage before it is committed.
    pub fn add_damage(&mut self, amount: u16) -> Result<(), RuleError> {
        self.game.add_event_damage(self.event, amount)
    }

    /// Append a legal primary target to the current paid useCard2 subject.
    pub fn add_use_target(&mut self, target: SeatId) -> Result<(), RuleError> {
        self.game.edit_use_target(self.event, target, true)
    }

    /// Remove one live primary target while keeping at least one.
    pub fn remove_use_target(&mut self, target: SeatId) -> Result<(), RuleError> {
        self.game.edit_use_target(self.event, target, false)
    }

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
    /// Register an indexed callback; its count is sampled when it joins a timing request.
    pub fn register_indexed_trigger(
        &mut self,
        registration: TriggerRegistration,
        count: TriggerIndex,
    ) -> Result<TriggerId, RuleError> {
        self.game.register_indexed_trigger(registration, count)
    }

    /// Revoke a registration, including a pending candidate in this event.
    pub fn unregister_trigger(&mut self, id: TriggerId) -> bool {
        self.game.unregister_trigger(id)
    }

    /// Grant a named skill. A second grant refreshes its expiry.
    pub fn grant_temporary(
        &mut self,
        owner: SeatId,
        skill: crate::TemporarySkill,
    ) -> Result<(), RuleError> {
        self.game.grant_temporary(owner, skill)
    }

    /// Remove just this temporary skill and its owned callbacks and modifiers.
    pub fn remove_temporary(&mut self, owner: SeatId, name: &str) -> bool {
        self.game.remove_temporary(owner, name)
    }
    pub fn owner(&self) -> SeatId {
        self.owner
    }

    /// Use a virtual Sha with an explicitly specified nature and no physical material.
    pub fn use_virtual_sha(
        &mut self,
        source: SeatId,
        target: SeatId,
        nature: Option<crate::DamageNature>,
    ) -> Result<(), RuleError> {
        self.game.content_sha_program(source, target, nature)?;
        self.operations.push_back(TriggerOperation::VirtualSha {
            source,
            target,
            nature,
        });
        Ok(())
    }

    /// Move one held hand or equipment card through the public expansion gain lifecycle.
    pub fn add_to_expansion(
        &mut self,
        owner: SeatId,
        id: CardId,
        tag: impl Into<String>,
    ) -> Result<(), RuleError> {
        let tag = tag.into();
        if self.claimed_cards().contains(&id) {
            return Err(RuleError::InvalidSelection);
        }
        let card = self
            .game
            .hand_and_equipment(owner)?
            .find(|card| card.id == id)
            .copied()
            .ok_or(RuleError::InvalidCard)?;
        self.game.expansion_program(owner, card, tag.clone())?;
        self.operations
            .push_back(TriggerOperation::Expand { owner, card, tag });
        Ok(())
    }

    /// Gain one physical expansion card from its present owner.
    pub fn gain_expansion(
        &mut self,
        from: SeatId,
        to: SeatId,
        id: CardId,
    ) -> Result<(), RuleError> {
        if self.claimed_cards().contains(&id) {
            return Err(RuleError::InvalidSelection);
        }
        let card = self
            .game
            .seat(from)?
            .expansions
            .iter()
            .find(|card| card.id == id)
            .copied()
            .ok_or(RuleError::InvalidCard)?;
        self.game.gain_expansion_program(from, to, card)?;
        self.operations
            .push_back(TriggerOperation::GainExpansion { from, to, card });
        Ok(())
    }

    /// Schedule two independent private hand choices, a common reveal, and resolution.
    pub fn compare(&mut self, initiator: SeatId, target: SeatId) -> Result<(), RuleError> {
        self.game.compare_program(initiator, target)?;
        self.operations
            .push_back(TriggerOperation::Compare { initiator, target });
        Ok(())
    }

    /// The latest comparison this skill awaited, including its original cards and event parent.
    pub fn compare_result(&self) -> Option<crate::CompareResult> {
        self.last_compare
    }

    /// Replace an effective score on the current compare or compareFixing subject.
    pub fn set_compare_number(&mut self, seat: SeatId, number: i32) -> Result<(), RuleError> {
        self.game.change_compare_number(self.event, seat, number)
    }

    /// Claim one revealed bid at chooseToCompareAfter through the ordinary gain lifecycle.
    pub fn gain_compare_card(&mut self, seat: SeatId, card: CardId) -> Result<(), RuleError> {
        if self.event.timing != "chooseToCompareAfter" {
            return Err(RuleError::WrongEvent);
        }
        if self.claimed_cards().contains(&card) {
            return Err(RuleError::InvalidSelection);
        }
        let subject = self.event.subject;
        self.game.compare_gain_program(subject, seat, card)?;
        self.operations.push_back(TriggerOperation::GainCompare {
            subject,
            seat,
            card,
        });
        Ok(())
    }

    /// Reserve a new physical identity and queue its private gain through the normal gain event.
    pub fn gain_created_card(
        &mut self,
        seat: SeatId,
        kind: crate::CardKind,
        suit: crate::classic_data::Suit,
        rank: u8,
        nature: Option<crate::DamageNature>,
    ) -> Result<CardId, RuleError> {
        if !self.game.seat(seat)?.alive {
            return Err(RuleError::InactiveSeat { seat });
        }
        let card = self.game.create_content_card(kind, suit, rank, nature)?;
        self.operations.push_back(TriggerOperation::Elemental(
            crate::elemental::ContentAction::GainCreated { seat, card },
        ));
        Ok(card.id)
    }
    pub fn event(&self) -> &TriggerEvent {
        self.event
    }
    /// Schedule one atomic hand payment. Each physical card can be promised once per stage.
    /// If an intervening skill moves a promised card, this skill ends before its continuation.
    pub fn discard_hand(&mut self, seat: SeatId, ids: &[CardId]) -> Result<(), RuleError> {
        let hand = &self.game.seat(seat)?.hand;
        let mut claimed = self.claimed_cards();
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

    /// Recast one held card at this skill's timing, even outside its owner's play phase.
    /// The skill supplies permission for an ordinary card. Registered filters and
    /// movement overrides still apply. A cancelled payment stops this continuation.
    pub fn recast(&mut self, seat: SeatId, card: CardId) -> Result<(), RuleError> {
        if self.claimed_cards().contains(&card) {
            return Err(RuleError::InvalidSelection);
        }
        self.game.recast_terms(seat, card, false)?;
        self.operations
            .push_back(TriggerOperation::Recast { seat, card });
        Ok(())
    }

    /// Give one held card using the ordinary gain/lose lifecycle. A publicly
    /// known card stays known and a hidden card stays private to its holders.
    pub fn give_hand(&mut self, from: SeatId, to: SeatId, card: CardId) -> Result<(), RuleError> {
        if self.claimed_cards().contains(&card) {
            return Err(RuleError::InvalidSelection);
        }
        self.game.give_hand_program(from, to, card)?;
        self.operations
            .push_back(TriggerOperation::GiveHand { from, to, card });
        Ok(())
    }

    fn claimed_cards(&self) -> HashSet<CardId> {
        let mut claimed = HashSet::new();
        for operation in &self.operations {
            match operation {
                TriggerOperation::DiscardHand { cards, .. } => {
                    claimed.extend(cards.iter().map(|c| c.id))
                }
                TriggerOperation::Recast { card, .. }
                | TriggerOperation::GiveHand { card, .. }
                | TriggerOperation::GainCompare { card, .. } => {
                    claimed.insert(*card);
                }
                TriggerOperation::Expand { card, .. }
                | TriggerOperation::GainExpansion { card, .. } => {
                    claimed.insert(card.id);
                }
                TriggerOperation::VirtualSha { .. }
                | TriggerOperation::Emit(_)
                | TriggerOperation::Elemental(_)
                | TriggerOperation::Compare { .. } => {}
            }
        }
        claimed
    }

    pub(crate) fn enqueue_elemental(
        &mut self,
        seat: SeatId,
        action: crate::elemental::ContentAction,
    ) -> Result<(), RuleError> {
        if !self.game.seat(seat)?.alive {
            return Err(RuleError::InactiveSeat { seat });
        }
        self.operations
            .push_back(TriggerOperation::Elemental(action));
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
    VirtualSha {
        source: SeatId,
        target: SeatId,
        nature: Option<crate::DamageNature>,
    },
    Expand {
        owner: SeatId,
        card: Card,
        tag: String,
    },
    GainExpansion {
        from: SeatId,
        to: SeatId,
        card: Card,
    },
    Compare {
        initiator: SeatId,
        target: SeatId,
    },
    GainCompare {
        subject: EventId,
        seat: SeatId,
        card: CardId,
    },
    Elemental(crate::elemental::ContentAction),
    DiscardHand {
        seat: SeatId,
        cards: Vec<Card>,
    },
    Recast {
        seat: SeatId,
        card: CardId,
    },
    GiveHand {
        from: SeatId,
        to: SeatId,
        card: CardId,
    },
    Emit(crate::classic::skills::Dispatch),
}

#[derive(Clone)]
enum RegisteredState {
    Activate(TriggerSelection),
    Call(u16),
    Choice(u16),
    Operations {
        remaining: VecDeque<TriggerOperation>,
        then: TriggerStep,
        require_success: bool,
    },
}

#[derive(Clone)]
pub(crate) struct Registered {
    active_skill: bool,
    last_compare: Option<crate::CompareResult>,
    pub id: TriggerId,
    pub owner: SeatId,
    event: TriggerEvent,
    effect: TriggerEffect,
    state: RegisteredState,
}

impl Registered {
    pub(crate) fn active(
        id: u64,
        owner: SeatId,
        event: TriggerEvent,
        effect: TriggerEffect,
        selection: TriggerSelection,
    ) -> Self {
        Self {
            active_skill: true,
            state: RegisteredState::Activate(selection),
            ..Self::new(id, owner, event, effect)
        }
    }

    pub(crate) fn kind(&self) -> crate::EventKind {
        if self.active_skill {
            crate::EventKind::ActiveSkillEffect {
                seat: self.owner,
                registration: self.id,
            }
        } else {
            crate::EventKind::RegisteredTrigger {
                seat: self.owner,
                registration: self.id,
            }
        }
    }

    pub(crate) fn new(
        id: TriggerId,
        owner: SeatId,
        event: TriggerEvent,
        effect: TriggerEffect,
    ) -> Self {
        Self {
            active_skill: false,
            id,
            owner,
            event,
            effect,
            state: RegisteredState::Call(0),
            last_compare: None,
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
    pub fn register_indexed_trigger(
        &mut self,
        registration: TriggerRegistration,
        count: TriggerIndex,
    ) -> Result<TriggerId, RuleError> {
        let id = self.register_trigger(registration)?;
        self.indexed_triggers.insert(id, count);
        Ok(id)
    }
    /// Revoke a callback, including any pending, unconsumed occurrence.
    pub fn unregister_trigger(&mut self, id: TriggerId) -> bool {
        self.indexed_triggers.remove(&id);
        self.trigger_registrations.remove(&id).is_some()
    }
    pub(crate) fn has_timing_registrations(&self, timing: &str) -> bool {
        self.has_expiry_at(timing)
            || self
                .trigger_registrations
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
        self.scheduler.next(Program::Dispatch(event));
        true
    }

    pub(crate) fn prepare_timing(
        &mut self,
        timing: &'static str,
        player: Option<SeatId>,
        source: Option<SeatId>,
        target: Option<SeatId>,
        amount: u16,
        cards: &[Card],
    ) -> Option<crate::classic::skills::Dispatch> {
        if !self.wants_timing(timing) {
            return None;
        }
        let event = self.subject_timing(timing, player, source, target, amount, cards);
        if self.begin_timing_request(&event) {
            Some(self.dispatch_for_timing(event))
        } else {
            None
        }
    }

    pub(crate) fn wants_timing(&self, timing: &str) -> bool {
        #[cfg(test)]
        if self.timing_observer.is_some() {
            return true;
        }
        self.has_timing_registrations(timing)
    }

    #[cfg(test)]
    pub(crate) fn observe_timing_request(&self, event: &TriggerEvent) {
        if let Some(observer) = &self.timing_observer {
            observer(self, event);
        }
    }

    #[cfg(not(test))]
    #[inline]
    pub(crate) fn observe_timing_request(&self, _: &TriggerEvent) {}

    pub(crate) fn subject_timing(
        &self,
        timing: &'static str,
        player: Option<SeatId>,
        source: Option<SeatId>,
        target: Option<SeatId>,
        amount: u16,
        cards: &[Card],
    ) -> TriggerEvent {
        let subject = self.scheduler.current_id();
        let frame = self.scheduler.active_frame(subject);
        let target = target.or_else(
            || match (timing, frame.map(|(_, program)| program.kind())) {
                (
                    "useCard1" | "useCard2" | "useCard" | "useCardAfter",
                    Some(crate::EventKind::UseTrick { card, target, .. }),
                ) if card.kind == crate::CardKind::Jiedao => target,
                _ => None,
            },
        );
        let (card_kind, nature) = match frame {
            Some((_, Program::CardDamage(hit))) => (hit.card.kind, hit.card.nature),
            Some((_, program)) => (
                match program.kind() {
                    crate::EventKind::UseSha { .. } | crate::EventKind::UseShaTargets { .. } => {
                        Some(crate::CardKind::Sha)
                    }
                    crate::EventKind::UseTrick { card, .. }
                    | crate::EventKind::UseResponseCard { card, .. } => Some(card.kind),
                    crate::EventKind::UseVirtual { kind, .. } => Some(kind),
                    _ => None,
                },
                None,
            ),
            None => (None, None),
        };
        TriggerEvent {
            timing: timing.into(),
            subject,
            parent: frame.and_then(|(event, _)| event.parent),
            player,
            source,
            target,
            amount,
            cards: cards.to_vec(),
            card_kind,
            nature,
            delta: None,
        }
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
            && r.role.matches(r.owner, event)
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
        if let Some(Outcome::Finished(Resolution::Compared(result))) = &child {
            flow.last_compare = Some(*result);
        }
        let (stage, selected) = match std::mem::replace(&mut flow.state, RegisteredState::Call(0)) {
            RegisteredState::Activate(selection) => (0, Some(selection)),
            RegisteredState::Operations {
                require_success: true,
                ..
            } if matches!(
                child,
                Some(
                    Outcome::Cancelled
                        | Outcome::Finished(Resolution::RecastMove { card: None, .. })
                )
            ) =>
            {
                self.finish();
                return;
            }
            RegisteredState::Operations {
                remaining, then, ..
            } => {
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
            last_compare: flow.last_compare,
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
        while let Some(operation) = operations.pop_front() {
            // Earlier children may have triggered other skills or paused for a decision.
            // Recheck the entire next payment before allowing any card in it to move.
            let require_success = matches!(
                operation,
                TriggerOperation::Recast { .. }
                    | TriggerOperation::Expand { .. }
                    | TriggerOperation::GainExpansion { .. }
                    | TriggerOperation::GiveHand { .. }
                    | TriggerOperation::Compare { .. }
                    | TriggerOperation::GainCompare { .. }
            );
            let program = match operation {
                TriggerOperation::VirtualSha {
                    source,
                    target,
                    nature,
                } => {
                    let Ok(program) = self.content_sha_program(source, target, nature) else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::Expand { owner, card, tag } => {
                    let Ok(program) = self.expansion_program(owner, card, tag) else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::GainExpansion { from, to, card } => {
                    let Ok(program) = self.gain_expansion_program(from, to, card) else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::Compare { initiator, target } => {
                    let Ok(program) = self.compare_program(initiator, target) else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::GainCompare {
                    subject,
                    seat,
                    card,
                } => {
                    let Ok(program) = self.compare_gain_program(subject, seat, card) else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::Elemental(action) => match action.program(self) {
                    Some(program) => program,
                    None => continue,
                },
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
                TriggerOperation::Recast { seat, card } => {
                    let Ok(program) = self.recast_program(seat, card, false) else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::GiveHand { from, to, card } => {
                    let Ok(program) = self.give_hand_program(from, to, card) else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::Emit(queue) => Program::Dispatch(queue),
            };
            flow.state = RegisteredState::Operations {
                remaining: operations,
                then,
                require_success,
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
