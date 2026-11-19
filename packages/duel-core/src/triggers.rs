use crate::scheduler::Outcome;
use crate::{
    Card, CardId, CardZone, ChoiceBranch, ChoiceRequest, EventId, Game, Program, Resolution,
    RuleError, SeatId, Zone,
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
    pub delta: Option<i64>,
}

impl TriggerEvent {
    /// The subject's numeric argument when its rules define one. `amount` separately
    /// counts physical cards on movement callbacks, e.g. a loss of three cards has num 0.
    pub fn subject_number(&self, game: &Game) -> Option<i64> {
        use crate::EventKind::*;
        match game.event_kind(self.subject)? {
            Damage { .. } | LoseHp { .. } | Draw { .. } => Some(i64::from(self.amount)),
            GainMaxHp { num, .. } | LoseMaxHp { num, .. } => Some(i64::from(num)),
            Lose { .. } | LoseToDiscardpile { .. } | DiscardGroup { .. } | ActivateSkill { .. } => {
                Some(0)
            }
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
    ChooseFor {
        seat: SeatId,
        branches: Vec<ChoiceBranch>,
        then: u16,
    },
    Done,
    CancelSubject,
    Continue(u16),
    /// Resume with a new callback carrying data from this invocation.
    ContinueWith(TriggerEffect),
    MoveCards {
        request: crate::MoveChoiceRequest,
        then: u16,
    },
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
    last_group: Option<EventId>,
    last_move: Option<&'a crate::MoveResult>,
    operations: VecDeque<TriggerOperation>,
    preview_started: bool,
}
impl TriggerContext<'_> {
    pub fn preview_pile(&mut self, seat: SeatId, count: u16) -> Result<(), RuleError> {
        if !self.game.seat(seat)?.is_in() {
            return Err(RuleError::InactiveSeat { seat });
        }
        if self.game.classic.is_none()
            || self.preview_started
            || self
                .operations
                .iter()
                .any(|op| matches!(op, TriggerOperation::PreviewPile { .. }))
        {
            return Err(RuleError::InvalidSelection);
        }
        self.operations
            .push_back(TriggerOperation::PreviewPile { seat, count });
        Ok(())
    }
    pub fn preview_cards(&self) -> Vec<Card> {
        self.game
            .preview_pile_cards(self.game.scheduler.current_id())
    }
    pub fn return_pile_rows(
        &mut self,
        seat: SeatId,
        top: &[Card],
        bottom: &[Card],
    ) -> Result<(), RuleError> {
        if !self.game.seat(seat)?.is_in() {
            return Err(RuleError::InactiveSeat { seat });
        }
        let owned = self.preview_cards();
        let mut claimed = self.claimed_cards();
        for card in top.iter().chain(bottom) {
            if !owned.contains(card) || !claimed.insert(card.id) {
                return Err(RuleError::InvalidSelection);
            }
        }
        self.operations.push_back(TriggerOperation::ReturnPile {
            seat,
            rows: [top.to_vec(), bottom.to_vec()],
        });
        Ok(())
    }
    pub fn move_result(&self) -> Option<&crate::MoveResult> {
        self.last_move
    }

    /// Cause a direct death without a rescue window, as requested by the acting rules content.
    pub fn die(&mut self, target: SeatId, source: Option<SeatId>) -> Result<(), RuleError> {
        self.queue_death(target, source, false)
    }

    /// Queue a death attempt that also executes its contents for an out-of-play seat.
    pub fn die_including_out(
        &mut self,
        target: SeatId,
        source: Option<SeatId>,
    ) -> Result<(), RuleError> {
        self.queue_death(target, source, true)
    }

    fn queue_death(
        &mut self,
        target: SeatId,
        source: Option<SeatId>,
        include_out: bool,
    ) -> Result<(), RuleError> {
        if let Some(source) = source {
            self.game.seat(source)?;
        }
        if !self.game.seat(target)?.alive {
            return Err(RuleError::InactiveSeat { seat: target });
        }
        self.operations.push_back(TriggerOperation::Death {
            target,
            source,
            include_out,
        });
        Ok(())
    }

    /// Reserve the currently dispatched death, retaining selected mark displays.
    pub fn reserve_death(
        &mut self,
        subject: EventId,
        excluded_marks: &[&str],
    ) -> Result<(), RuleError> {
        if subject != self.event.subject
            || !matches!(self.event.timing.as_str(), "dieBefore" | "dieBegin")
        {
            return Err(RuleError::WrongEvent);
        }
        let Some(Program::Death(flow)) = self.game.scheduler.active_program_mut(subject) else {
            return Err(RuleError::WrongEvent);
        };
        if !matches!(
            flow.stage,
            crate::DeathStage::Begin | crate::DeathStage::Apply
        ) {
            return Err(RuleError::WrongEvent);
        }
        flow.reserved = true;
        for mark in excluded_marks {
            if !flow.excluded_marks.iter().any(|existing| existing == mark) {
                flow.excluded_marks.push((*mark).to_owned());
            }
        }
        Ok(())
    }

    /// Revive immediately, including when called from a death timing callback.
    pub fn revive(&mut self, seat: SeatId, hp: Option<i32>) -> Result<(), RuleError> {
        self.game.revive_now(seat, hp)
    }

    /// Queue a cancellable revival event after this callback returns.
    pub fn revive_event(&mut self, seat: SeatId, hp: Option<i32>) -> Result<(), RuleError> {
        self.game.seat(seat)?;
        self.operations
            .push_back(TriggerOperation::Revive { seat, hp });
        Ok(())
    }

    pub fn rest(&mut self, seat: SeatId, clock: crate::RestClock) -> Result<(), RuleError> {
        self.game.seat(seat)?;
        self.operations
            .push_back(TriggerOperation::Rest(crate::classic::rest::Flow::rest(
                seat, clock,
            )));
        Ok(())
    }

    pub fn rest_end(&mut self, seat: SeatId, hp: Option<i32>) -> Result<(), RuleError> {
        let hp = hp.unwrap_or(self.game.seat(seat)?.max_hp);
        self.operations
            .push_back(TriggerOperation::Rest(crate::classic::rest::Flow::end(
                seat, hp,
            )));
        Ok(())
    }

    /// Abolish the seat's judgment area, then attempt to discard its current delayed tricks.
    pub fn disable_judge(&mut self, seat: SeatId) -> Result<(), RuleError> {
        self.queue_judge_area(seat, true)
    }

    /// Restore the seat's ability to receive delayed tricks.
    pub fn enable_judge(&mut self, seat: SeatId) -> Result<(), RuleError> {
        self.queue_judge_area(seat, false)
    }

    /// Place a physical card in a judgment area with the given delayed-trick identity.
    /// Effects that require an available area can check `Game::can_add_judge` first.
    pub fn add_judge(
        &mut self,
        seat: SeatId,
        material: CardId,
        kind: crate::CardKind,
    ) -> Result<(), RuleError> {
        self.game.seat(seat)?;
        if kind.card_type() != crate::CardType::Delay {
            return Err(RuleError::UnsupportedCard);
        }
        let card = self
            .game
            .add_judge_material(material)
            .ok_or(RuleError::InvalidCard)?;
        self.operations.push_back(TriggerOperation::AddJudge(
            crate::classic::add_judge::Flow::new(seat, card, kind),
        ));
        Ok(())
    }

    fn queue_judge_area(&mut self, seat: SeatId, disable: bool) -> Result<(), RuleError> {
        self.game.seat(seat)?;
        self.operations.push_back(TriggerOperation::JudgeArea(
            crate::classic::judge_area::Flow::new(seat, Some(self.owner), disable),
        ));
        Ok(())
    }

    pub(crate) fn tick_rest_clock(&mut self) {
        let seat = self.owner;
        if self.game.tick_rest_clock(seat, self.event.subject) {
            self.rest_end(seat, None)
                .expect("the rest clock owner exists");
        }
    }

    pub fn gain_max_hp(
        &mut self,
        seat: SeatId,
        num: i32,
        source: Option<SeatId>,
    ) -> Result<(), RuleError> {
        self.queue_max_hp(seat, num, source, crate::MaxHpKind::Gain)
    }

    pub fn lose_max_hp(
        &mut self,
        seat: SeatId,
        num: i32,
        source: Option<SeatId>,
    ) -> Result<(), RuleError> {
        self.queue_max_hp(seat, num, source, crate::MaxHpKind::Lose)
    }

    fn queue_max_hp(
        &mut self,
        seat: SeatId,
        num: i32,
        source: Option<SeatId>,
        kind: crate::MaxHpKind,
    ) -> Result<(), RuleError> {
        self.game.max_hp_program(seat, source, kind, num)?;
        self.operations.push_back(TriggerOperation::MaxHp {
            seat,
            source,
            kind,
            num,
        });
        Ok(())
    }
    /// Lock one bid from every seat, then settle the group using the requested mode.
    pub fn compare_many(
        &mut self,
        initiator: SeatId,
        targets: &[SeatId],
        mode: crate::CompareMode,
        callback: Option<TriggerEffect>,
    ) -> Result<(), RuleError> {
        self.game
            .compare_many_program(initiator, targets.to_vec(), mode, callback.clone())?;
        self.operations.push_back(TriggerOperation::CompareMany {
            initiator,
            targets: targets.to_vec(),
            mode,
            callback,
        });
        Ok(())
    }

    pub fn compare_many_result(&self) -> Option<&crate::CompareManyFrame> {
        self.last_group
            .and_then(|id| self.game.group_comparison(id))
    }

    pub fn force_group_winner(&mut self, winner: Option<SeatId>) -> Result<(), RuleError> {
        self.game.force_group_winner(self.event, winner)
    }
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

    /// Move one of the recipient's held hand, equipment or judgment cards to expansion.
    pub fn add_to_expansion(
        &mut self,
        owner: SeatId,
        id: CardId,
        tag: impl Into<String>,
    ) -> Result<(), RuleError> {
        let tag = tag.into();
        self.game.seat(owner)?;
        if self.claimed_cards().contains(&id) {
            return Err(RuleError::InvalidSelection);
        }
        let card = self
            .game
            .held_card_location(id)
            .filter(|(seat, zone, _)| *seat == owner && *zone != Zone::Expansion)
            .map(|(_, _, card)| card)
            .ok_or(RuleError::InvalidCard)?;
        self.game.expansion_program(owner, card, tag.clone())?;
        self.operations
            .push_back(TriggerOperation::Expand { owner, card, tag });
        Ok(())
    }

    /// Gain a public equipment, judgment or expansion card from its present holder.
    pub fn gain_owned_card(
        &mut self,
        from: SeatId,
        to: SeatId,
        id: CardId,
    ) -> Result<(), RuleError> {
        if self.claimed_cards().contains(&id) {
            return Err(RuleError::InvalidSelection);
        }
        self.game.gain_owned_card_program(from, to, id)?;
        self.operations
            .push_back(TriggerOperation::GainOwnedCard { from, to, id });
        Ok(())
    }

    /// Install an actual equipment card from the recipient's hand.
    pub fn equip_from_hand(&mut self, seat: SeatId, id: CardId) -> Result<(), RuleError> {
        if self.claimed_cards().contains(&id) {
            return Err(RuleError::InvalidSelection);
        }
        self.game.seat(seat)?;
        if seat != self.owner && !self.game.is_known_by(id, Some(self.owner)) {
            return Err(RuleError::InvalidCard);
        }
        self.game.equip_from_hand_program(seat, id)?;
        self.operations
            .push_back(TriggerOperation::EquipFromHand { seat, id });
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
        if self.game.group_comparison(self.event.subject).is_some() {
            return self.game.set_group_number(self.event, seat, number);
        }
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
        if !self.game.seat(seat)?.is_in() {
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
        self.discard_owned(seat, ids, &[Zone::Hand])
    }

    pub fn discard_held(&mut self, seat: SeatId, ids: &[CardId]) -> Result<(), RuleError> {
        self.discard_owned(seat, ids, &[Zone::Hand, Zone::Equipment])
    }

    fn discard_owned(
        &mut self,
        seat: SeatId,
        ids: &[CardId],
        zones: &[Zone],
    ) -> Result<(), RuleError> {
        let player = self.game.seat(seat)?;
        let mut claimed = self.claimed_cards();
        let mut cards = Vec::with_capacity(ids.len());
        for id in ids {
            if !claimed.insert(*id) {
                return Err(RuleError::InvalidSelection);
            }
            let held = zones.iter().find_map(|&zone| {
                player
                    .zone(zone)
                    .iter()
                    .find(|card| card.id == *id)
                    .map(|&card| (zone, card))
            });
            cards.push(held.ok_or(RuleError::InvalidCard)?);
        }
        if !cards.is_empty() {
            self.operations
                .push_back(TriggerOperation::DiscardOwned { seat, cards });
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

    /// Recast an ordered selection authorized by this skill.
    pub fn recast_batch(
        &mut self,
        seat: SeatId,
        cards: &[CardId],
        rule: crate::RecastBatchRule,
    ) -> Result<(), RuleError> {
        let claimed = self.claimed_cards();
        if cards.iter().any(|card| claimed.contains(card)) {
            return Err(RuleError::InvalidSelection);
        }
        self.game.recast_materials(seat, cards)?;
        self.operations.push_back(TriggerOperation::RecastBatch {
            seat,
            cards: cards.to_vec(),
            rule,
        });
        Ok(())
    }

    /// Select a physical card by scanning from a seeded random starting position.
    pub fn random_pile_card(
        &mut self,
        filter: impl FnMut(Card) -> bool,
    ) -> Option<(CardZone, Card)> {
        self.game.random_pile_card(filter)
    }

    /// Gain a specific card from the draw or discard pile face up.
    pub fn gain_public_pile_card(
        &mut self,
        seat: SeatId,
        pile: CardZone,
        card: CardId,
    ) -> Result<(), RuleError> {
        if self.claimed_cards().contains(&card) {
            return Err(RuleError::InvalidSelection);
        }
        self.game.gain_public_pile_program(seat, pile, card)?;
        self.operations
            .push_back(TriggerOperation::GainPublicPile { seat, pile, card });
        Ok(())
    }

    /// Transfer one or more hand or equipment cards as a single gain.
    pub fn give_cards(
        &mut self,
        from: SeatId,
        to: SeatId,
        cards: &[CardId],
        visible: bool,
    ) -> Result<(), RuleError> {
        let claimed = self.claimed_cards();
        if cards.iter().any(|card| claimed.contains(card)) {
            return Err(RuleError::InvalidSelection);
        }
        self.game.give_cards_program(from, to, cards, visible)?;
        self.operations.push_back(TriggerOperation::GiveCards {
            from,
            to,
            cards: cards.to_vec(),
            visible,
        });
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
                TriggerOperation::DiscardOwned { cards, .. } => {
                    claimed.extend(cards.iter().map(|(_, c)| c.id))
                }
                TriggerOperation::RecastBatch { cards, .. }
                | TriggerOperation::GiveCards { cards, .. } => {
                    claimed.extend(cards.iter().copied())
                }
                TriggerOperation::Recast { card, .. }
                | TriggerOperation::GiveHand { card, .. }
                | TriggerOperation::GainPublicPile { card, .. }
                | TriggerOperation::GainOwnedCard { id: card, .. }
                | TriggerOperation::EquipFromHand { id: card, .. }
                | TriggerOperation::GainCompare { card, .. } => {
                    claimed.insert(*card);
                }
                TriggerOperation::Expand { card, .. }
                | TriggerOperation::GainExpansion { card, .. } => {
                    claimed.insert(card.id);
                }
                TriggerOperation::ReturnPile { rows, .. } => {
                    claimed.extend(rows.iter().flatten().map(|card| card.id))
                }
                TriggerOperation::PreviewPile { .. }
                | TriggerOperation::MaxHp { .. }
                | TriggerOperation::Death { .. }
                | TriggerOperation::Revive { .. }
                | TriggerOperation::Rest(_)
                | TriggerOperation::JudgeArea(_)
                | TriggerOperation::AddJudge(_)
                | TriggerOperation::CompareMany { .. }
                | TriggerOperation::VirtualSha { .. }
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
        let player = self.game.seat(seat)?;
        if !player.alive
            || (player.out && !matches!(action, crate::elemental::ContentAction::TurnOver { .. }))
        {
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
    Rest(crate::classic::rest::Flow),
    JudgeArea(crate::classic::judge_area::Flow),
    AddJudge(crate::classic::add_judge::Flow),
    MaxHp {
        seat: SeatId,
        source: Option<SeatId>,
        kind: crate::MaxHpKind,
        num: i32,
    },
    PreviewPile {
        seat: SeatId,
        count: u16,
    },
    ReturnPile {
        seat: SeatId,
        rows: [Vec<Card>; 2],
    },
    Death {
        target: SeatId,
        source: Option<SeatId>,
        include_out: bool,
    },
    Revive {
        seat: SeatId,
        hp: Option<i32>,
    },
    CompareMany {
        initiator: SeatId,
        targets: Vec<SeatId>,
        mode: crate::CompareMode,
        callback: Option<TriggerEffect>,
    },
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
    GainOwnedCard {
        from: SeatId,
        to: SeatId,
        id: CardId,
    },
    EquipFromHand {
        seat: SeatId,
        id: CardId,
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
    DiscardOwned {
        seat: SeatId,
        cards: Vec<(Zone, Card)>,
    },
    Recast {
        seat: SeatId,
        card: CardId,
    },
    RecastBatch {
        seat: SeatId,
        cards: Vec<CardId>,
        rule: crate::RecastBatchRule,
    },
    GainPublicPile {
        seat: SeatId,
        pile: CardZone,
        card: CardId,
    },
    GiveCards {
        from: SeatId,
        to: SeatId,
        cards: Vec<CardId>,
        visible: bool,
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
    MoveChoice(u16),
    Operations {
        remaining: VecDeque<TriggerOperation>,
        then: TriggerStep,
        require_success: bool,
    },
}

#[derive(Clone)]
pub(crate) struct Registered {
    pair_callback: Option<SeatId>,
    last_group: Option<EventId>,
    last_move: Option<crate::MoveResult>,
    preview_started: bool,
    active_skill: bool,
    last_compare: Option<crate::CompareResult>,
    pub id: TriggerId,
    pub owner: SeatId,
    event: TriggerEvent,
    effect: TriggerEffect,
    state: RegisteredState,
}

impl Registered {
    pub(crate) fn pair(
        owner: SeatId,
        target: SeatId,
        event: TriggerEvent,
        effect: TriggerEffect,
    ) -> Self {
        Self {
            pair_callback: Some(target),
            ..Self::new(event.subject, owner, event, effect)
        }
    }
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
        if let Some(target) = self.pair_callback {
            crate::EventKind::ComparePairCallback {
                seat: self.owner,
                target,
                comparison: self.event.subject,
            }
        } else if self.active_skill {
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
            pair_callback: None,
            last_group: None,
            last_move: None,
            preview_started: false,
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
        self.force_die_triggers.remove(&id);
        self.force_out_triggers.remove(&id);
        self.trigger_registrations.remove(&id).is_some()
    }
    /// Allow this registration to run even when its owner is dead.
    pub fn set_trigger_force_die(&mut self, id: TriggerId, enabled: bool) -> Result<(), RuleError> {
        if !self.trigger_registrations.contains_key(&id) {
            return Err(RuleError::InvalidSelection);
        }
        if enabled {
            self.force_die_triggers.insert(id);
        } else {
            self.force_die_triggers.remove(&id);
        }
        Ok(())
    }
    pub(crate) fn has_timing_registrations(&self, timing: &str) -> bool {
        self.has_expiry_at(timing)
            || self
                .trigger_registrations
                .values()
                .any(|r| r.timing == timing)
    }

    /// Permit this registration to run while its owner is out of play.
    pub fn set_trigger_force_out(&mut self, id: TriggerId, enabled: bool) -> Result<(), RuleError> {
        if !self.trigger_registrations.contains_key(&id) {
            return Err(RuleError::InvalidSelection);
        }
        if enabled {
            self.force_out_triggers.insert(id);
        } else {
            self.force_out_triggers.remove(&id);
        }
        Ok(())
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
            Some((_, Program::AddJudge(flow))) => (Some(flow.card_kind()), None),
            Some((_, program)) => (
                match program.kind() {
                    crate::EventKind::UseSha { .. } | crate::EventKind::UseShaTargets { .. } => {
                        Some(crate::CardKind::Sha)
                    }
                    crate::EventKind::UseTrick { card, .. }
                    | crate::EventKind::Equip { card, .. }
                    | crate::EventKind::UseResponseCard { card, .. } => Some(card.kind),
                    crate::EventKind::UseVirtual { kind, .. } => Some(kind),
                    crate::EventKind::ReplaceEquip { card_kind, .. } => Some(card_kind),
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
        self.registration_applies(id, r, event) && (r.filter)(self, event, r.owner)
    }
    fn registration_applies(
        &self,
        id: TriggerId,
        r: &TriggerRegistration,
        event: &TriggerEvent,
    ) -> bool {
        r.timing == event.timing
            && self.seat(r.owner).is_ok_and(|s| {
                (s.alive || self.force_die_triggers.contains(&id))
                    && (!s.out || self.force_out_triggers.contains(&id))
            })
            && r.role.matches(r.owner, event)
    }
    pub(crate) fn has_registered_candidates(&self, event: &TriggerEvent) -> bool {
        self.trigger_registrations
            .iter()
            .any(|(&id, r)| self.registration_applies(id, r, event))
    }
    pub(crate) fn resolve_registered(
        &mut self,
        mut flow: Registered,
        child: Option<Outcome<Resolution>>,
    ) {
        if let Some(Outcome::Finished(Resolution::Compared(result))) = &child {
            flow.last_compare = Some(*result);
        }
        if let Some(Outcome::Finished(Resolution::ComparedMany { event, .. })) = &child {
            flow.last_group = Some(*event);
        }
        let (stage, selected) = match std::mem::replace(&mut flow.state, RegisteredState::Call(0)) {
            RegisteredState::Activate(selection) => (0, Some(selection)),
            RegisteredState::Operations {
                require_success: true,
                ..
            } if matches!(
                &child,
                Some(
                    Outcome::Cancelled
                        | Outcome::Finished(Resolution::RecastMove { card: None, .. })
                )
            ) || matches!(
                &child,
                Some(Outcome::Finished(Resolution::BatchMove { cards, .. })) if cards.is_empty()
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
            RegisteredState::MoveChoice(stage) => match child {
                Some(Outcome::Finished(Resolution::MovedCards(result))) => {
                    flow.last_move = Some(result);
                    (stage, None)
                }
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
            last_group: flow.last_group,
            last_move: flow.last_move.as_ref(),
            operations: VecDeque::new(),
            preview_started: flow.preview_started,
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
                    | TriggerOperation::RecastBatch { .. }
                    | TriggerOperation::GiveCards { .. }
                    | TriggerOperation::Expand { .. }
                    | TriggerOperation::GainExpansion { .. }
                    | TriggerOperation::GainOwnedCard { .. }
                    | TriggerOperation::GiveHand { .. }
                    | TriggerOperation::Compare { .. }
                    | TriggerOperation::CompareMany { .. }
                    | TriggerOperation::GainCompare { .. }
                    | TriggerOperation::GainPublicPile { .. }
            );
            let program = match operation {
                TriggerOperation::PreviewPile { seat, count } => {
                    flow.preview_started = true;
                    let Some(program) =
                        self.preview_pile_program(self.scheduler.current_id(), seat, count)
                    else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::ReturnPile { seat, rows } => {
                    self.return_pile_program(self.scheduler.current_id(), seat, rows)
                }
                TriggerOperation::Death {
                    target,
                    source,
                    include_out,
                } => {
                    if !self.is_alive(target) {
                        continue;
                    }
                    let mut program = self.death_program(target, source);
                    if let Program::Death(flow) = &mut program {
                        flow.include_out = include_out;
                    }
                    program
                }
                TriggerOperation::Revive { seat, hp } => self.revive_program(seat, hp),
                TriggerOperation::Rest(flow) => Program::Rest(flow),
                TriggerOperation::JudgeArea(flow) => Program::JudgeArea(flow),
                TriggerOperation::AddJudge(flow) => Program::AddJudge(flow),
                TriggerOperation::MaxHp {
                    seat,
                    source,
                    kind,
                    num,
                } => {
                    let Ok(program) = self.max_hp_program(seat, source, kind, num) else {
                        continue;
                    };
                    program
                }
                TriggerOperation::CompareMany {
                    initiator,
                    targets,
                    mode,
                    callback,
                } => {
                    let Ok(program) = self.compare_many_program(initiator, targets, mode, callback)
                    else {
                        self.finish();
                        return;
                    };
                    program
                }
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
                TriggerOperation::GainOwnedCard { from, to, id } => {
                    let Ok(program) = self.gain_owned_card_program(from, to, id) else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::EquipFromHand { seat, id } => {
                    if seat != flow.owner && !self.is_known_by(id, Some(flow.owner)) {
                        self.finish();
                        return;
                    }
                    let Ok(program) = self.equip_from_hand_program(seat, id) else {
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
                TriggerOperation::DiscardOwned { seat, cards } => {
                    if !self.seat(seat).is_ok_and(|player| {
                        cards
                            .iter()
                            .all(|(zone, card)| player.zone(*zone).contains(card))
                    }) {
                        self.finish();
                        return;
                    }
                    Program::DiscardGroup { seat, cards }
                }
                TriggerOperation::Recast { seat, card } => {
                    let Ok(program) = self.recast_program(seat, card, false) else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::RecastBatch { seat, cards, rule } => {
                    let Ok(program) = self.recast_batch_program(seat, &cards, rule) else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::GiveCards {
                    from,
                    to,
                    cards,
                    visible,
                } => {
                    let Ok(program) = self.give_cards_program(from, to, &cards, visible) else {
                        self.finish();
                        return;
                    };
                    program
                }
                TriggerOperation::GainPublicPile { seat, pile, card } => {
                    let Ok(program) = self.gain_public_pile_program(seat, pile, card) else {
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
            TriggerStep::ContinueWith(effect) => {
                flow.effect = effect;
                flow.state = RegisteredState::Call(0);
                self.scheduler.replace_current(Program::Registered(flow));
            }
            TriggerStep::MoveCards { request, then } => {
                let Ok(program) = self.plan_move_choice(request) else {
                    self.finish();
                    return;
                };
                flow.state = RegisteredState::MoveChoice(then);
                self.scheduler.replace_current(Program::Registered(flow));
                self.scheduler.next(program);
            }
            TriggerStep::Choose { branches, then } => {
                let seat = flow.owner;
                self.registered_choice(flow, seat, branches, then);
            }
            TriggerStep::ChooseFor {
                seat,
                branches,
                then,
            } => self.registered_choice(flow, seat, branches, then),
        }
    }

    fn registered_choice(
        &mut self,
        mut flow: Registered,
        seat: SeatId,
        branches: Vec<ChoiceBranch>,
        then: u16,
    ) {
        if !self.is_in(seat) || self.match_result().is_some() {
            self.finish();
            return;
        }
        flow.state = RegisteredState::Choice(then);
        self.scheduler.replace_current(Program::Registered(flow));
        self.scheduler.next(Program::Choose(ChoiceRequest {
            seat,
            target: None,
            branches,
        }));
    }
}
