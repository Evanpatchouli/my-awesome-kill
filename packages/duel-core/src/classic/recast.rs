use crate::scheduler::{EventRef, Outcome};
#[cfg(test)]
use crate::TriggerEvent;
use crate::{
    Card, CardFace, CardId, CardKind, CardZone, DrawReason, EventId, EventKind, Game, Program,
    Resolution, RuleError, SeatId, TurnView, Zone,
};
use std::sync::Arc;

mod batch_movement;
use batch_movement::BatchMovement;
pub use batch_movement::LossRecord;

pub type RecastBatchLossFactory =
    Arc<dyn Fn(RecastState<'_>, SeatId, &[Card]) -> RecastLoss + Send + Sync>;
pub type RecastBatchGainFactory = Arc<
    dyn Fn(RecastState<'_>, SeatId, &[Card], &RecastBatchMovement) -> RecastBatchGain + Send + Sync,
>;

#[derive(Clone)]
pub struct RecastBatchRule {
    pub loss: RecastBatchLossFactory,
    pub gain: RecastBatchGainFactory,
}

impl Default for RecastBatchRule {
    fn default() -> Self {
        Self {
            loss: Arc::new(|_, _, _| RecastLoss::Discard),
            gain: Arc::new(|_, _, _, _| RecastBatchGain::Draw),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecastBatchGain {
    Draw,
    /// Gain these physical draw-pile cards face up in the supplied order.
    FromDeck {
        cards: Vec<CardId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecastBatchMovement {
    pub event: EventId,
    pub kind: RecastLoss,
    pub moved: Vec<Card>,
    pub cancelled: bool,
}

/// Read-only table state for trusted card content. Reading it never enumerates player actions.
#[derive(Clone, Copy)]
pub struct RecastState<'a>(&'a Game);

#[derive(Clone, Copy)]
pub struct RecastSeat<'a> {
    pub id: SeatId,
    pub alive: bool,
    pub hp: i32,
    pub max_hp: i32,
    pub hand: &'a [Card],
    pub equipment: &'a [Card],
    pub judges: &'a [Card],
}

impl<'a> RecastState<'a> {
    pub fn seat(self, id: SeatId) -> Option<RecastSeat<'a>> {
        self.0.seat(id).ok().map(|player| RecastSeat {
            id: player.id,
            alive: player.alive,
            hp: player.hp,
            max_hp: player.max_hp,
            hand: &player.hand,
            equipment: &player.equipment,
            judges: &player.judges,
        })
    }

    pub fn seats(self) -> impl Iterator<Item = SeatId> + 'a {
        self.0.seats.iter().map(|player| player.id)
    }

    /// The top card is last, as in the engine's draw pile.
    pub fn draw_pile(self) -> &'a [Card] {
        self.0
            .classic
            .as_ref()
            .map_or(&[], |table| table.deck.as_slice())
    }

    pub fn discard(self) -> &'a [Card] {
        &self.0.discard
    }

    pub fn resolving(self) -> &'a [Card] {
        &self.0.resolving
    }

    /// Hand-to-hand transfers between their nested lose and gain events.
    pub fn transferring(self) -> &'a [Card] {
        &self.0.transferring
    }

    pub fn face(self, id: CardId) -> Option<CardFace> {
        self.0.face(id)
    }

    pub fn turn(self) -> Option<TurnView> {
        self.0.classic.as_ref()?.turn
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecastKey {
    Kind(CardKind),
    Card(CardId),
}

pub type RecastFilter = Arc<dyn Fn(RecastState<'_>, SeatId, Card) -> bool + Send + Sync>;
pub type RecastLossFactory = Arc<dyn Fn(RecastState<'_>, SeatId, Card) -> RecastLoss + Send + Sync>;
pub type RecastGainFactory =
    Arc<dyn Fn(RecastState<'_>, SeatId, Card, &RecastMovement) -> RecastGain + Send + Sync>;

#[derive(Clone)]
pub struct RecastRule {
    pub filter: Option<RecastFilter>,
    pub loss: RecastLossFactory,
    pub gain: RecastGainFactory,
}

impl Default for RecastRule {
    fn default() -> Self {
        Self {
            filter: None,
            loss: Arc::new(|_, _, _| RecastLoss::Discard),
            gain: Arc::new(|_, _, _, _| RecastGain::Draw),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecastLoss {
    Discard,
    Give { to: SeatId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecastGain {
    Draw,
    /// Gain this particular physical draw-pile card face up (gain2).
    FromDeck {
        card: CardId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecastMovement {
    pub event: EventId,
    pub kind: RecastLoss,
    pub card: Card,
    /// True when this particular loss moved the material. A skill can intercept it first.
    pub moved: bool,
}

#[derive(Clone)]
pub(crate) enum Flow {
    Recast(Recasting),
    Movement(Movement),
    BatchMovement(BatchMovement),
}

impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::Recast(event) => EventKind::Recast { seat: event.seat },
            Self::Movement(event) => event.event_kind(),
            Self::BatchMovement(event) => event.event_kind(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Recasting {
    seat: SeatId,
    terms: RecastTerms,
    stage: RecastStage,
    post: Option<crate::classic::skills::Dispatch>,
    cancelled_before_movement: bool,
}

#[derive(Clone)]
enum RecastTerms {
    Single(Card, RecastRule),
    Batch(Arc<[Card]>, RecastBatchRule),
}

#[derive(Clone)]
enum RecastPayment {
    Single(RecastMovement),
    Batch(RecastBatchMovement),
}

#[derive(Clone)]
enum RecastChild {
    Single(Movement),
    Batch(BatchMovement),
}

impl RecastChild {
    fn program(self) -> Program {
        Program::Recast(match self {
            Self::Single(movement) => Flow::Movement(movement),
            Self::Batch(movement) => Flow::BatchMovement(movement),
        })
    }
}

#[derive(Clone)]
enum RecastStage {
    Before,
    Begin,
    Announce,
    LossFactory,
    RunLoss(RecastLoss),
    Lost(RecastLoss),
    Paid(RecastPayment),
    GainFactory(RecastPayment),
    RunGain(Option<RecastChild>),
    Gained,
    End,
    After,
    Finish,
    Cancel(Option<RemainingRecast>),
    FinishCancelled(Option<RemainingRecast>),
}

#[derive(Clone)]
enum RemainingRecast {
    AfterLoss(RecastPayment),
    Gain(RecastPayment),
}

impl Recasting {
    fn cards(&self) -> &[Card] {
        match &self.terms {
            RecastTerms::Single(card, _) => std::slice::from_ref(card),
            RecastTerms::Batch(cards, _) => cards,
        }
    }

    fn request(
        &self,
        game: &mut Game,
        name: &'static str,
    ) -> Option<crate::classic::skills::Dispatch> {
        game.prepare_timing(
            name,
            Some(self.seat),
            None,
            None,
            self.cards().len() as u16,
            self.cards(),
        )
    }

    fn gain_movement(&self, state: RecastState<'_>, loss: &RecastPayment) -> Option<RecastChild> {
        let seat = self.seat;
        match (&self.terms, loss) {
            (RecastTerms::Single(card, rule), RecastPayment::Single(loss)) => {
                match (rule.gain)(state, seat, *card, loss) {
                    RecastGain::Draw => Some(Movement::new(seat, MoveKind::Draw)),
                    RecastGain::FromDeck { card: id } => {
                        state.draw_pile().iter().find(|c| c.id == id).map(|&card| {
                            Movement::new(
                                seat,
                                MoveKind::Gain {
                                    card,
                                    origin: GainOrigin::DrawPile,
                                },
                            )
                        })
                    }
                }
                .map(RecastChild::Single)
            }
            (RecastTerms::Batch(cards, rule), RecastPayment::Batch(loss)) => {
                let movement = match (rule.gain)(state, seat, cards, loss) {
                    RecastBatchGain::Draw => {
                        BatchMovement::draw(seat, cards.len() as u16, DrawReason::CardEffect)
                    }
                    RecastBatchGain::FromDeck { cards } => BatchMovement::gain_from_pile(
                        seat,
                        CardZone::DrawPile,
                        select_cards(&cards, |id| {
                            state.draw_pile().iter().copied().find(|card| card.id == id)
                        })
                        .ok()?,
                    ),
                };
                Some(RecastChild::Batch(movement))
            }
            _ => unreachable!("the payment comes from this recast"),
        }
    }
}

fn select_cards(
    ids: &[CardId],
    mut card_for: impl FnMut(CardId) -> Option<Card>,
) -> Result<Arc<[Card]>, RuleError> {
    if ids.is_empty() || ids.len() > usize::from(u16::MAX) {
        return Err(RuleError::InvalidSelection);
    }
    let mut seen = std::collections::HashSet::with_capacity(ids.len());
    ids.iter()
        .map(|&id| {
            if !seen.insert(id) {
                return Err(RuleError::InvalidSelection);
            }
            card_for(id).ok_or(RuleError::InvalidCard)
        })
        .collect()
}

#[derive(Clone, Copy)]
pub(crate) struct Movement {
    seat: SeatId,
    kind: MoveKind,
    stage: MoveStage,
    committed: Option<Card>,
    allow_intercepted: bool,
}

#[derive(Clone, Copy)]
enum MoveKind {
    DiscardToPile(Card),
    Lose {
        card: Card,
        zone: Zone,
        destination: LossDestination,
    },
    Draw,
    Gain {
        card: Card,
        origin: GainOrigin,
    },
}

#[derive(Clone, Copy)]
enum LossDestination {
    Discard,
    JudgeSpecial,
    Ordering(EventId),
    Transfer { to: SeatId, visible: bool },
    Compare(EventId),
}

#[derive(Clone, Copy)]
enum GainOrigin {
    Compare(EventId),
    Judgment(EventId),
    Created,
    Hand {
        seat: SeatId,
        public: bool,
    },
    Expansion {
        seat: SeatId,
    },
    DrawPile,
    /// Draw has already taken this card out of the pile, privately.
    Drawn,
}

#[derive(Clone, Copy)]
enum MoveStage {
    Before,
    Begin,
    Move,
    ChildMoved,
    Discarded,
    End,
    After,
    Finish,
    Cancel(MoveCancellation),
    FinishCancelled(MoveCancellation),
}

#[derive(Clone, Copy)]
enum MoveCancellation {
    Trigger,
    InvalidMovement,
}

impl Movement {
    fn new(seat: SeatId, kind: MoveKind) -> Self {
        Self {
            seat,
            kind,
            stage: MoveStage::Before,
            committed: None,
            allow_intercepted: false,
        }
    }

    fn source(self) -> Option<SeatId> {
        match self.kind {
            MoveKind::Draw => Some(self.seat),
            MoveKind::Gain {
                origin: GainOrigin::Hand { seat, .. },
                ..
            } => Some(seat),
            MoveKind::Gain {
                origin: GainOrigin::Expansion { seat },
                ..
            } => Some(seat),
            _ => None,
        }
    }

    fn card(self) -> Option<Card> {
        match self.kind {
            MoveKind::DiscardToPile(card)
            | MoveKind::Lose { card, .. }
            | MoveKind::Gain { card, .. } => Some(card),
            MoveKind::Draw => None,
        }
    }

    fn event_kind(self) -> EventKind {
        let seat = self.seat;
        match self.kind {
            MoveKind::DiscardToPile(_) => EventKind::LoseToDiscardpile { seat, amount: 1 },
            MoveKind::Lose { .. } => EventKind::Lose { seat, amount: 1 },
            MoveKind::Draw => EventKind::Draw {
                seat,
                amount: match self.stage {
                    MoveStage::Finish | MoveStage::FinishCancelled(_) => {
                        u16::from(self.committed.is_some())
                    }
                    _ => 1,
                },
                reason: DrawReason::CardEffect,
            },
            MoveKind::Gain { origin, .. } => EventKind::Gain {
                seat,
                source: self.source(),
                amount: 1,
                card: self.committed.filter(|_| {
                    matches!(
                        origin,
                        GainOrigin::DrawPile
                            | GainOrigin::Compare(_)
                            | GainOrigin::Judgment(_)
                            | GainOrigin::Hand { public: true, .. }
                            | GainOrigin::Expansion { .. }
                    )
                }),
            },
        }
    }

    fn hook(self, stage: MoveStage) -> &'static str {
        let names = match self.kind {
            MoveKind::DiscardToPile(_) => [
                "loseToDiscardpileBefore",
                "loseToDiscardpileBegin",
                "loseToDiscardpileEnd",
                "loseToDiscardpileAfter",
                "loseToDiscardpileCancelled",
            ],
            MoveKind::Lose { .. } => [
                "loseBefore",
                "loseBegin",
                "loseEnd",
                "loseAfter",
                "loseCancelled",
            ],
            MoveKind::Draw => [
                "drawBefore",
                "drawBegin",
                "drawEnd",
                "drawAfter",
                "drawCancelled",
            ],
            MoveKind::Gain { .. } => [
                "gainBefore",
                "gainBegin",
                "gainEnd",
                "gainAfter",
                "gainCancelled",
            ],
        };
        match stage {
            MoveStage::Before => names[0],
            MoveStage::Begin => names[1],
            MoveStage::End => names[2],
            MoveStage::After => names[3],
            MoveStage::Cancel(_) => names[4],
            _ => unreachable!("only a lifecycle stage dispatches these timings"),
        }
    }
}

impl Game {
    /// Committed loss records from the movement flow, including empty successful losses.
    pub fn loss_history(&self) -> &[LossRecord] {
        &self.loss_history
    }

    pub fn loss_record(&self, event: EventId) -> Option<&LossRecord> {
        self.loss_history
            .iter()
            .rev()
            .find(|record| record.event == event)
    }

    pub(crate) fn gain_judgment_program(
        &self,
        seat: SeatId,
        card: Card,
        owner: EventId,
    ) -> Program {
        Program::Recast(Flow::Movement(Movement::new(
            seat,
            MoveKind::Gain {
                card,
                origin: GainOrigin::Judgment(owner),
            },
        )))
    }
    pub(crate) fn judgment_loss_program(
        &self,
        owner: EventId,
        seat: SeatId,
        card: Card,
    ) -> Program {
        self.lose_to_ordering(seat, Zone::Judgment, card, owner)
    }

    pub(crate) fn lose_to_judge_special(&self, seat: SeatId, zone: Zone, card: Card) -> Program {
        let mut movement = Movement::new(
            seat,
            MoveKind::Lose {
                card,
                zone,
                destination: LossDestination::JudgeSpecial,
            },
        );
        movement.allow_intercepted = true;
        Program::Recast(Flow::Movement(movement))
    }
    pub(crate) fn lose_to_ordering(
        &self,
        seat: SeatId,
        zone: Zone,
        card: Card,
        owner: EventId,
    ) -> Program {
        Program::Recast(Flow::Movement(Movement::new(
            seat,
            MoveKind::Lose {
                card,
                zone,
                destination: LossDestination::Ordering(owner),
            },
        )))
    }
    pub(crate) fn use_loss_to_ordering(
        &self,
        seat: SeatId,
        zone: Zone,
        card: Card,
        owner: EventId,
    ) -> Program {
        let mut movement = Movement::new(
            seat,
            MoveKind::Lose {
                card,
                zone,
                destination: LossDestination::Ordering(owner),
            },
        );
        movement.allow_intercepted = true;
        Program::Recast(Flow::Movement(movement))
    }
    pub(crate) fn expansion_lose_program(
        &self,
        seat: SeatId,
        card: Card,
        zone: Zone,
        discard: bool,
    ) -> Program {
        Program::Recast(Flow::Movement(Movement::new(
            seat,
            MoveKind::Lose {
                card,
                zone,
                destination: if discard {
                    LossDestination::Discard
                } else {
                    LossDestination::Transfer {
                        to: seat,
                        visible: true,
                    }
                },
            },
        )))
    }

    pub(crate) fn expansion_gain_program(&self, from: SeatId, to: SeatId, card: Card) -> Program {
        Program::Recast(Flow::Movement(Movement::new(
            to,
            MoveKind::Gain {
                card,
                origin: GainOrigin::Expansion { seat: from },
            },
        )))
    }

    pub(crate) fn lose_compare_program(
        &self,
        comparison: EventId,
        seat: SeatId,
        card: Card,
    ) -> Program {
        Program::Recast(Flow::Movement(Movement::new(
            seat,
            MoveKind::Lose {
                card,
                zone: Zone::Hand,
                destination: LossDestination::Compare(comparison),
            },
        )))
    }

    pub(crate) fn gain_compare_program(
        &self,
        comparison: EventId,
        seat: SeatId,
        card: Card,
    ) -> Program {
        Program::Recast(Flow::Movement(Movement::new(
            seat,
            MoveKind::Gain {
                card,
                origin: GainOrigin::Compare(comparison),
            },
        )))
    }

    pub fn register_recast(&mut self, key: RecastKey, rule: RecastRule) -> Option<RecastRule> {
        if let Some((_, old)) = self.recast_rules.iter_mut().find(|(k, _)| *k == key) {
            return Some(std::mem::replace(old, rule));
        }
        self.recast_rules.push((key, rule));
        None
    }

    pub(crate) fn draw_one_program(&self, seat: SeatId) -> Program {
        Program::Recast(Flow::Movement(Movement::new(seat, MoveKind::Draw)))
    }

    pub(crate) fn draw_many_program(
        &self,
        seat: SeatId,
        count: u16,
        reason: DrawReason,
    ) -> Program {
        Program::Recast(Flow::BatchMovement(BatchMovement::draw(
            seat, count, reason,
        )))
    }

    pub(crate) fn created_gain_program(&self, seat: SeatId, card: Card) -> Program {
        Program::Recast(Flow::Movement(Movement::new(
            seat,
            MoveKind::Gain {
                card,
                origin: GainOrigin::Created,
            },
        )))
    }

    fn recast_rule(&self, card: Card) -> Option<&RecastRule> {
        self.recast_rules
            .iter()
            .find(|(key, _)| *key == RecastKey::Card(card.id))
            .or_else(|| {
                self.recast_rules
                    .iter()
                    .find(|(key, _)| *key == RecastKey::Kind(card.kind))
            })
            .map(|(_, rule)| rule)
    }

    pub(crate) fn recast_terms(
        &self,
        seat: SeatId,
        id: CardId,
        strict: bool,
    ) -> Result<(Card, Option<&RecastRule>), RuleError> {
        let card = self.recast_hand_card(seat, id)?;
        let rule = self.recast_rule(card);
        if (strict && card.kind != CardKind::Tiesuo && rule.is_none())
            || rule
                .and_then(|r| r.filter.as_ref())
                .is_some_and(|f| !f(RecastState(self), seat, card))
        {
            return Err(RuleError::InvalidCard);
        }
        Ok((card, rule))
    }

    fn check_movement_seat(&self, seat: SeatId) -> Result<&crate::Seat, RuleError> {
        if self.match_result().is_some() {
            return Err(RuleError::GameOver);
        }
        if self.classic.is_none() {
            return Err(RuleError::UnsupportedCard);
        }
        let player = self.seat(seat)?;
        if !self.is_in(seat) {
            return Err(RuleError::InactiveSeat { seat });
        }
        Ok(player)
    }

    fn recast_hand_card(&self, seat: SeatId, id: CardId) -> Result<Card, RuleError> {
        self.check_movement_seat(seat)?
            .hand
            .iter()
            .copied()
            .find(|c| c.id == id)
            .ok_or(RuleError::InvalidCard)
    }

    pub fn can_recast(&self, seat: SeatId, id: CardId) -> bool {
        self.recast_terms(seat, id, true).is_ok()
    }

    /// Eligibility for material selected by a skill that supplies recast permission.
    pub fn can_recast_material(&self, seat: SeatId, id: CardId) -> bool {
        self.recast_materials(seat, &[id]).is_ok()
    }

    pub(crate) fn recast_materials(
        &self,
        seat: SeatId,
        ids: &[CardId],
    ) -> Result<Arc<[Card]>, RuleError> {
        let cards = self.held_materials(seat, ids)?;
        if cards.iter().any(|&card| {
            self.recast_rule(card)
                .and_then(|rule| rule.filter.as_ref())
                .is_some_and(|filter| !filter(RecastState(self), seat, card))
        }) {
            return Err(RuleError::InvalidCard);
        }
        Ok(cards)
    }

    pub(crate) fn held_materials(
        &self,
        seat: SeatId,
        ids: &[CardId],
    ) -> Result<Arc<[Card]>, RuleError> {
        let player = self.check_movement_seat(seat)?;
        select_cards(ids, |id| {
            player
                .hand
                .iter()
                .chain(&player.equipment)
                .copied()
                .find(|card| card.id == id)
        })
    }

    pub(crate) fn recast_batch_program(
        &self,
        seat: SeatId,
        ids: &[CardId],
        rule: RecastBatchRule,
    ) -> Result<Program, RuleError> {
        let cards = self.recast_materials(seat, ids)?;
        Ok(self.recasting_program(seat, RecastTerms::Batch(cards, rule)))
    }

    pub(crate) fn plan_recast(&self, seat: SeatId, id: CardId) -> Result<Program, RuleError> {
        self.checked_play_seat(seat)?;
        self.recast_program(seat, id, true)
    }

    pub(crate) fn recast_program(
        &self,
        seat: SeatId,
        id: CardId,
        strict: bool,
    ) -> Result<Program, RuleError> {
        let (card, rule) = self.recast_terms(seat, id, strict)?;
        Ok(self.recasting_program(
            seat,
            RecastTerms::Single(card, rule.cloned().unwrap_or_default()),
        ))
    }

    fn recasting_program(&self, seat: SeatId, terms: RecastTerms) -> Program {
        Program::Recast(Flow::Recast(Recasting {
            seat,
            terms,
            stage: RecastStage::Before,
            post: None,
            cancelled_before_movement: false,
        }))
    }

    pub(crate) fn give_cards_program(
        &self,
        from: SeatId,
        to: SeatId,
        ids: &[CardId],
        visible: bool,
    ) -> Result<Program, RuleError> {
        let cards = self.held_materials(from, ids)?;
        self.seat(to)?;
        if from == to || !self.is_in(to) {
            return Err(RuleError::InvalidTarget);
        }
        Ok(RecastChild::Batch(BatchMovement::give(from, to, cards, visible)).program())
    }

    pub(crate) fn gain_public_pile_program(
        &self,
        seat: SeatId,
        pile: CardZone,
        id: CardId,
    ) -> Result<Program, RuleError> {
        self.check_movement_seat(seat)?;
        if pile == CardZone::Ordering {
            let owner = self
                .ordering_owners
                .get(&id)
                .copied()
                .ok_or(RuleError::InvalidCard)?;
            if self.private_ordering.contains(&owner) || self.compares.hidden(id) {
                return Err(RuleError::InvalidCard);
            }
            let card = self
                .resolving
                .iter()
                .find(|card| card.id == id)
                .copied()
                .ok_or(RuleError::InvalidCard)?;
            return Ok(self.gain_judgment_program(seat, card, owner));
        }
        let cards = match pile {
            CardZone::DrawPile => RecastState(self).draw_pile(),
            CardZone::DiscardPile => &self.discard,
            _ => return Err(RuleError::InvalidSelection),
        };
        let card = cards
            .iter()
            .copied()
            .find(|card| card.id == id)
            .ok_or(RuleError::InvalidCard)?;
        Ok(
            RecastChild::Batch(BatchMovement::gain_from_pile(seat, pile, Arc::from([card])))
                .program(),
        )
    }

    pub(crate) fn give_hand_program(
        &self,
        from: SeatId,
        to: SeatId,
        id: CardId,
    ) -> Result<Program, RuleError> {
        let card = self.recast_hand_card(from, id)?;
        self.seat(to)?;
        if from == to || !self.is_in(to) {
            return Err(RuleError::InvalidTarget);
        }
        Ok(Program::Recast(Flow::Movement(Movement::new(
            to,
            MoveKind::Gain {
                card,
                origin: GainOrigin::Hand {
                    seat: from,
                    public: self.known_to_all.contains(&id),
                },
            },
        ))))
    }

    pub(crate) fn resolve_recast(
        &mut self,
        event: EventRef,
        flow: Flow,
        child: Option<Outcome<Resolution>>,
    ) {
        match flow {
            Flow::Recast(recast) => self.resolve_recasting(recast, child),
            Flow::Movement(movement) => self.resolve_recast_movement(event, movement, child),
            Flow::BatchMovement(movement) => self.resolve_batch_movement(event, movement, child),
        }
    }

    fn recast_held(&self, seat: SeatId, card: Card) -> bool {
        self.is_in(seat) && self.seat(seat).is_ok_and(|s| s.hand.contains(&card))
    }

    fn dispatch_recast_hook(&mut self, event: Option<crate::classic::skills::Dispatch>) -> bool {
        if let Some(event) = event {
            self.scheduler.next(Program::Dispatch(event));
            return true;
        }
        false
    }

    fn resolve_recasting(&mut self, mut recast: Recasting, mut child: Option<Outcome<Resolution>>) {
        use RecastStage::*;
        let seat = recast.seat;
        if matches!(child, Some(Outcome::Cancelled)) {
            match recast.stage.clone() {
                // The factory already queued the movement. Cancellation requests its
                // notification now, but the existing child runs before its callbacks.
                RunLoss(_) | RunGain(_) => {
                    recast.cancelled_before_movement = true;
                    recast.post = recast.request(self, "recastCancelled");
                }
                FinishCancelled(_) => {}
                Paid(loss) => recast.stage = Cancel(Some(RemainingRecast::AfterLoss(loss))),
                GainFactory(loss) => recast.stage = Cancel(Some(RemainingRecast::Gain(loss))),
                _ => recast.stage = Cancel(None),
            }
        }
        loop {
            let queued = match recast.stage.clone() {
                Before => {
                    recast.stage = Begin;
                    let timing = recast.request(self, "recastBefore");
                    self.dispatch_recast_hook(timing)
                }
                Begin => {
                    recast.stage = Announce;
                    let timing = recast.request(self, "recastBegin");
                    self.dispatch_recast_hook(timing)
                }
                Announce => {
                    if let RecastTerms::Single(card, _) = &recast.terms {
                        if self.recast_held(seat, *card) {
                            self.known_to_all.insert(card.id);
                        }
                    }
                    for &card in recast.cards() {
                        self.next(EventKind::RecastReveal { seat, card });
                    }
                    recast.stage = LossFactory;
                    true
                }
                LossFactory => {
                    let pre = recast.request(self, "recastingLose");
                    let loss = match &recast.terms {
                        RecastTerms::Single(card, rule) => {
                            (rule.loss)(RecastState(self), seat, *card)
                        }
                        RecastTerms::Batch(cards, rule) => {
                            (rule.loss)(RecastState(self), seat, cards)
                        }
                    };
                    recast.post = recast.request(self, "recastingLost");
                    recast.stage = RunLoss(loss);
                    self.dispatch_recast_hook(pre)
                }
                RunLoss(loss) => {
                    let movement = match (&recast.terms, loss) {
                        (RecastTerms::Single(card, _), RecastLoss::Discard) => {
                            RecastChild::Single(Movement::new(seat, MoveKind::DiscardToPile(*card)))
                        }
                        (RecastTerms::Single(card, _), RecastLoss::Give { to })
                            if to != seat && self.is_in(to) =>
                        {
                            RecastChild::Single(Movement::new(
                                to,
                                MoveKind::Gain {
                                    card: *card,
                                    origin: GainOrigin::Hand { seat, public: true },
                                },
                            ))
                        }
                        (RecastTerms::Batch(cards, _), RecastLoss::Discard) => {
                            RecastChild::Batch(BatchMovement::discard(seat, cards.clone()))
                        }
                        (RecastTerms::Batch(cards, _), RecastLoss::Give { to })
                            if to != seat && self.is_in(to) =>
                        {
                            RecastChild::Batch(BatchMovement::give(seat, to, cards.clone(), true))
                        }
                        _ => {
                            recast.stage = Cancel(None);
                            continue;
                        }
                    };
                    recast.stage = Lost(loss);
                    self.scheduler.next(movement.program());
                    true
                }
                Lost(kind) => {
                    let movement = match (&recast.terms, child.take()) {
                        (
                            RecastTerms::Single(card, _),
                            Some(Outcome::Finished(Resolution::RecastMove {
                                event,
                                card: moved,
                                ..
                            })),
                        ) => RecastPayment::Single(RecastMovement {
                            event,
                            kind,
                            card: *card,
                            moved: moved == Some(*card),
                        }),
                        (
                            RecastTerms::Batch(..),
                            Some(Outcome::Finished(Resolution::BatchMove {
                                event,
                                cards,
                                cancelled,
                            })),
                        ) => RecastPayment::Batch(RecastBatchMovement {
                            event,
                            kind,
                            moved: cards,
                            cancelled,
                        }),
                        _ => {
                            recast.stage = Cancel(None);
                            continue;
                        }
                    };
                    if recast.cancelled_before_movement {
                        recast.stage = Cancel(Some(RemainingRecast::AfterLoss(movement)));
                        continue;
                    }
                    recast.stage = Paid(movement);
                    self.dispatch_recast_hook(recast.post.take())
                }
                Paid(movement) => {
                    recast.stage = GainFactory(movement);
                    let timing = recast.request(self, "recast");
                    self.dispatch_recast_hook(timing)
                }
                GainFactory(loss) => {
                    let pre = recast.request(self, "recastingGain");
                    let movement = recast.gain_movement(RecastState(self), &loss);
                    recast.post = recast.request(self, "recastingGained");
                    recast.stage = RunGain(movement);
                    self.dispatch_recast_hook(pre)
                }
                RunGain(Some(movement)) => {
                    recast.stage = Gained;
                    self.scheduler.next(movement.program());
                    true
                }
                RunGain(None) => {
                    recast.stage = Cancel(None);
                    continue;
                }
                Gained => {
                    if recast.cancelled_before_movement {
                        recast.stage = Cancel(None);
                        continue;
                    }
                    recast.stage = End;
                    self.dispatch_recast_hook(recast.post.take())
                }
                End => {
                    recast.stage = After;
                    let timing = recast.request(self, "recastEnd");
                    self.dispatch_recast_hook(timing)
                }
                After => {
                    recast.stage = Finish;
                    let timing = recast.request(self, "recastAfter");
                    self.dispatch_recast_hook(timing)
                }
                Cancel(remaining) => {
                    recast.stage = FinishCancelled(remaining);
                    if recast.cancelled_before_movement {
                        self.dispatch_recast_hook(recast.post.take())
                    } else {
                        let timing = recast.request(self, "recastCancelled");
                        self.dispatch_recast_hook(timing)
                    }
                }
                FinishCancelled(remaining) => {
                    // The async content still makes its remaining timing requests
                    // and calls the gain factory. A cancelled parent accepts no new children.
                    if let Some(remaining) = remaining {
                        let loss = match remaining {
                            RemainingRecast::AfterLoss(loss) => {
                                let _ = recast.request(self, "recast");
                                loss
                            }
                            RemainingRecast::Gain(loss) => loss,
                        };
                        let _ = recast.request(self, "recastingGain");
                        let _ = recast.gain_movement(RecastState(self), &loss);
                        let _ = recast.request(self, "recastingGained");
                    }
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                Finish => {
                    self.finish();
                    return;
                }
            };
            if queued {
                self.scheduler
                    .replace_current(Program::Recast(Flow::Recast(recast)));
                return;
            }
        }
    }

    fn resolve_recast_movement(
        &mut self,
        event: EventRef,
        mut movement: Movement,
        child: Option<Outcome<Resolution>>,
    ) {
        use MoveKind::*;
        use MoveStage::*;
        let seat = movement.seat;
        if matches!(child, Some(Outcome::Cancelled))
            && !matches!(movement.stage, FinishCancelled(_))
        {
            movement.stage = Cancel(match movement.stage {
                ChildMoved => MoveCancellation::InvalidMovement,
                _ => MoveCancellation::Trigger,
            });
        }
        loop {
            let stage = movement.stage;
            let queued = match stage {
                Before | Begin | End | After | Cancel(_) => {
                    movement.stage = match stage {
                        Before => Begin,
                        Begin => Move,
                        End => After,
                        After => Finish,
                        Cancel(reason) => FinishCancelled(reason),
                        _ => unreachable!(),
                    };
                    // An ordinary draw reserves its private card before gainBefore.
                    if matches!(stage, Cancel(_)) && movement.committed.is_none() {
                        if let Gain {
                            card,
                            origin: GainOrigin::Drawn,
                        } = movement.kind
                        {
                            self.classic.as_mut().unwrap().deck.push(card);
                        }
                    }
                    let settled = matches!(
                        (stage, movement.kind),
                        (End | After, Lose { .. } | Gain { .. })
                    );
                    let cards = if settled {
                        movement.committed
                    } else {
                        movement.card()
                    };
                    if let (After, Lose { zone, .. }, Some(card)) = (stage, movement.kind, cards) {
                        if matches!(zone, Zone::Hand | Zone::Equipment)
                            && self.queue_hero_losses(seat, vec![(zone, card)])
                        {
                            true
                        } else {
                            self.queue_timing("loseAfter", Some(seat), None, None, 1, &[card])
                        }
                    } else {
                        let amount = if settled {
                            cards.as_slice().len() as u16
                        } else {
                            1
                        };
                        self.queue_timing(
                            movement.hook(stage),
                            Some(seat),
                            movement.source(),
                            None,
                            amount,
                            cards.as_slice(),
                        )
                    }
                }
                Move => {
                    if !self.is_alive(seat)
                        && !matches!(
                            movement.kind,
                            Lose {
                                zone: Zone::Expansion,
                                ..
                            } | Lose {
                                destination: LossDestination::JudgeSpecial,
                                ..
                            }
                        )
                    {
                        movement.stage = Cancel(MoveCancellation::InvalidMovement);
                        continue;
                    }
                    if self.is_out(seat)
                        && !matches!(
                            movement.kind,
                            Lose {
                                zone: Zone::Expansion,
                                ..
                            } | Lose {
                                destination: LossDestination::Compare(_)
                                    | LossDestination::Ordering(_),
                                ..
                            }
                        )
                    {
                        movement.stage = Cancel(MoveCancellation::InvalidMovement);
                        continue;
                    }
                    if let Lose {
                        card,
                        zone,
                        destination: LossDestination::JudgeSpecial,
                    } = &mut movement.kind
                    {
                        let owner = self.seat(seat).expect("material holder");
                        if let Some(current) = crate::location::HELD_ZONES
                            .into_iter()
                            .find(|&current| owner.zone(current).contains(card))
                        {
                            *zone = current;
                        }
                    }
                    let child_move = match movement.kind {
                        DiscardToPile(card) => Some(Movement::new(
                            seat,
                            Lose {
                                card,
                                zone: Zone::Hand,
                                destination: LossDestination::Discard,
                            },
                        )),
                        Draw => self
                            .draw_from_piles(1)
                            .and_then(|mut cards| cards.pop())
                            .map(|card| {
                                Movement::new(
                                    seat,
                                    Gain {
                                        card,
                                        origin: GainOrigin::Drawn,
                                    },
                                )
                            }),
                        Gain {
                            card,
                            origin: GainOrigin::Hand { seat: from, public },
                        } if self.recast_held(from, card) => Some(Movement::new(
                            from,
                            Lose {
                                card,
                                zone: Zone::Hand,
                                destination: LossDestination::Transfer {
                                    to: seat,
                                    visible: public,
                                },
                            },
                        )),
                        Gain {
                            card,
                            origin: GainOrigin::Expansion { seat: from },
                        } if self.seat(from).is_ok_and(|s| s.expansions.contains(&card)) => {
                            Some(Movement::new(
                                from,
                                Lose {
                                    card,
                                    zone: Zone::Expansion,
                                    destination: LossDestination::Transfer {
                                        to: seat,
                                        visible: true,
                                    },
                                },
                            ))
                        }
                        Lose {
                            card,
                            zone,
                            destination,
                        } if self.seat(seat).is_ok_and(|s| match zone {
                            Zone::Hand => s.hand.contains(&card),
                            Zone::Equipment => s.equipment.contains(&card),
                            Zone::Judgment => s.judges.contains(&card),
                            Zone::Expansion => s.expansions.contains(&card),
                        }) && !matches!(destination, LossDestination::Transfer { to, .. } if !self.is_in(to)) =>
                        {
                            let taken = self.take_card(seat, zone, card.id);
                            if matches!(zone, Zone::Hand)
                                && matches!(
                                    destination,
                                    LossDestination::Transfer { visible: false, .. }
                                )
                            {
                                self.obscure_remaining_hand(seat);
                            }
                            match destination {
                                LossDestination::Discard => self.discard.push(taken),
                                LossDestination::JudgeSpecial => {
                                    self.known_to_all.insert(taken.id);
                                    self.transferring.push(taken);
                                }
                                LossDestination::Ordering(owner) => {
                                    self.resolving.push(taken);
                                    self.ordering_owners.insert(taken.id, owner);
                                }
                                LossDestination::Transfer { .. } => self.transferring.push(taken),
                                LossDestination::Compare(comparison) => {
                                    self.place_compare_bid(comparison, seat, taken);
                                }
                            }
                            movement.committed = Some(taken);
                            None
                        }
                        Gain {
                            card,
                            origin: GainOrigin::Compare(comparison),
                        } => {
                            if let Some(taken) = self.take_compare_bid(comparison, card.id) {
                                self.put_in_hand(seat, taken);
                                self.known_to_all.insert(taken.id);
                                movement.committed = Some(taken);
                            }
                            None
                        }
                        Gain {
                            card,
                            origin: GainOrigin::Judgment(owner),
                        } => {
                            if self.resolving.contains(&card)
                                && self.ordering_owners.get(&card.id) == Some(&owner)
                            {
                                let taken = self.take_resolving(card.id);
                                self.put_in_hand(seat, taken);
                                self.known_to_all.insert(taken.id);
                                movement.committed = Some(taken);
                            }
                            None
                        }
                        Gain {
                            card,
                            origin: GainOrigin::Drawn | GainOrigin::Created,
                        } => {
                            self.seat_mut(seat).hand.push(card);
                            movement.committed = Some(card);
                            None
                        }
                        Gain {
                            card,
                            origin: GainOrigin::DrawPile,
                        } => {
                            let deck = &mut self.classic.as_mut().unwrap().deck;
                            if let Some(index) = deck.iter().position(|c| *c == card) {
                                let taken = deck.remove(index);
                                self.seat_mut(seat).hand.push(taken);
                                self.known_to_all.insert(taken.id);
                                movement.committed = Some(taken);
                            }
                            None
                        }
                        _ => None,
                    };
                    if let Some(child_move) = child_move {
                        movement.stage = ChildMoved;
                        self.scheduler
                            .next(Program::Recast(Flow::Movement(child_move)));
                        true
                    } else {
                        // The original default lose still finishes with an empty card
                        // list if a loseBefore skill moved the material to another zone.
                        let intercepted = movement.allow_intercepted
                            || matches!(
                                movement.kind,
                                Lose {
                                    destination: LossDestination::Discard,
                                    ..
                                }
                            );
                        movement.stage = if movement.committed.is_some() || intercepted {
                            if let Lose { card, zone, .. } = movement.kind {
                                self.loss_history.push(LossRecord {
                                    event: event.id,
                                    seat,
                                    stockcards: vec![card],
                                    cards: movement
                                        .committed
                                        .into_iter()
                                        .map(|card| (zone, card))
                                        .collect(),
                                });
                            }
                            End
                        } else {
                            Cancel(MoveCancellation::InvalidMovement)
                        };
                        false
                    }
                }
                ChildMoved => {
                    let Some(Outcome::Finished(Resolution::RecastMove { card, .. })) = child else {
                        movement.stage = Cancel(MoveCancellation::InvalidMovement);
                        continue;
                    };
                    if let Gain {
                        origin: GainOrigin::Hand { .. } | GainOrigin::Expansion { .. },
                        ..
                    } = movement.kind
                    {
                        let (from, public, from_expansion) = match movement.kind {
                            Gain {
                                origin: GainOrigin::Hand { seat, public },
                                ..
                            } => (seat, public, false),
                            Gain {
                                origin: GainOrigin::Expansion { seat },
                                ..
                            } => (seat, true, true),
                            _ => unreachable!(),
                        };
                        let Some(card) = card else {
                            // The nested lose may have been cancelled. The gain
                            // has no physical card to receive and its own hooks finish.
                            movement.stage = End;
                            continue;
                        };
                        let card = Self::remove_card(&mut self.transferring, card.id);
                        if !self.is_in(seat) {
                            if self.is_alive(from) && !from_expansion {
                                self.seat_mut(from).hand.push(card);
                                if public {
                                    self.known_to_all.insert(card.id);
                                }
                            } else {
                                self.discard.push(card);
                            }
                            movement.stage = Cancel(MoveCancellation::InvalidMovement);
                            continue;
                        }
                        self.seat_mut(seat).hand.push(card);
                        if public {
                            self.known_to_all.insert(card.id);
                        } else {
                            self.mark_known_to(card.id, from);
                        }
                    }
                    movement.committed = card;
                    movement.stage = if matches!(movement.kind, DiscardToPile(_)) {
                        Discarded
                    } else {
                        End
                    };
                    false
                }
                Discarded => {
                    movement.stage = End;
                    self.queue_timing(
                        "loseToDiscardpile",
                        Some(seat),
                        None,
                        None,
                        1,
                        movement.card().as_slice(),
                    )
                }
                Finish | FinishCancelled(_) => {
                    self.scheduler
                        .replace_current(Program::Recast(Flow::Movement(movement)));
                    let outcome = match (stage, movement.committed) {
                        (FinishCancelled(MoveCancellation::InvalidMovement), None) => {
                            Outcome::Cancelled
                        }
                        // A cancelled child reports its own lifecycle and any
                        // settled cards. Its enclosing discard, draw or recast can resume.
                        (_, card) => Outcome::Finished(Resolution::RecastMove {
                            event: event.id,
                            card,
                            cancelled: matches!(stage, FinishCancelled(_)),
                        }),
                    };
                    self.scheduler.end(outcome);
                    return;
                }
            };
            if queued {
                self.scheduler
                    .replace_current(Program::Recast(Flow::Movement(movement)));
                return;
            }
        }
    }
}

#[cfg(test)]
mod batch_tests;
#[cfg(test)]
mod batch_trace;
#[cfg(test)]
mod tests;
