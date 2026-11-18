use crate::scheduler::{EventRef, Outcome};
#[cfg(test)]
use crate::TriggerEvent;
use crate::{
    Card, CardFace, CardId, CardKind, DrawReason, EventId, EventKind, Game, Program, Resolution,
    RuleError, SeatId, TurnView, Zone,
};
use std::sync::Arc;

/// Read-only table state for trusted card content. Reading it never enumerates player actions.
#[derive(Clone, Copy)]
pub struct RecastState<'a>(&'a Game);

#[derive(Clone, Copy)]
pub struct RecastSeat<'a> {
    pub id: SeatId,
    pub alive: bool,
    pub hp: i32,
    pub max_hp: u16,
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
}

impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::Recast(event) => EventKind::Recast { seat: event.seat },
            Self::Movement(event) => event.event_kind(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Recasting {
    seat: SeatId,
    card: Card,
    rule: RecastRule,
    stage: RecastStage,
    post: Option<crate::classic::skills::Dispatch>,
    cancelled_before_movement: bool,
}

#[derive(Clone, Copy)]
enum RecastStage {
    Before,
    Begin,
    Announce,
    LossFactory,
    RunLoss(RecastLoss),
    Lost(RecastLoss),
    Paid(RecastMovement),
    GainFactory(RecastMovement),
    RunGain(Option<Movement>),
    Gained,
    End,
    After,
    Finish,
    Cancel(Option<RemainingRecast>),
    FinishCancelled(Option<RemainingRecast>),
}

#[derive(Clone, Copy)]
enum RemainingRecast {
    AfterLoss(RecastMovement),
    Gain(RecastMovement),
}

impl Recasting {
    fn request(
        &self,
        game: &mut Game,
        name: &'static str,
    ) -> Option<crate::classic::skills::Dispatch> {
        game.prepare_timing(name, Some(self.seat), None, None, 1, &[self.card])
    }

    fn plan_gain(
        &self,
        game: &mut Game,
        loss: RecastMovement,
    ) -> (
        Option<crate::classic::skills::Dispatch>,
        RecastGain,
        Option<crate::classic::skills::Dispatch>,
    ) {
        let before = self.request(game, "recastingGain");
        let gain = (self.rule.gain)(RecastState(game), self.seat, self.card, &loss);
        let after = self.request(game, "recastingGained");
        (before, gain, after)
    }
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

    fn recast_hand_card(&self, seat: SeatId, id: CardId) -> Result<Card, RuleError> {
        if self.match_result().is_some() {
            return Err(RuleError::GameOver);
        }
        if self.classic.is_none() {
            return Err(RuleError::UnsupportedCard);
        }
        let player = self.seat(seat)?;
        if !player.alive {
            return Err(RuleError::InactiveSeat { seat });
        }
        player
            .hand
            .iter()
            .copied()
            .find(|c| c.id == id)
            .ok_or(RuleError::InvalidCard)
    }

    pub fn can_recast(&self, seat: SeatId, id: CardId) -> bool {
        self.recast_terms(seat, id, true).is_ok()
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
        Ok(Program::Recast(Flow::Recast(Recasting {
            seat,
            card,
            rule: rule.cloned().unwrap_or_default(),
            stage: RecastStage::Before,
            post: None,
            cancelled_before_movement: false,
        })))
    }

    pub(crate) fn give_hand_program(
        &self,
        from: SeatId,
        to: SeatId,
        id: CardId,
    ) -> Result<Program, RuleError> {
        let card = self.recast_hand_card(from, id)?;
        if from == to || !self.seat(to)?.alive {
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
        }
    }

    fn recast_alive(&self, seat: SeatId) -> bool {
        self.seat(seat).is_ok_and(|s| s.alive)
    }

    fn recast_held(&self, seat: SeatId, card: Card) -> bool {
        self.seat(seat)
            .is_ok_and(|s| s.alive && s.hand.contains(&card))
    }

    fn dispatch_recast_hook(&mut self, event: Option<crate::classic::skills::Dispatch>) -> bool {
        if let Some(event) = event {
            self.scheduler.next(Program::Dispatch(event));
            return true;
        }
        false
    }

    fn resolve_recasting(&mut self, mut recast: Recasting, child: Option<Outcome<Resolution>>) {
        use RecastStage::*;
        let (seat, card) = (recast.seat, recast.card);
        if matches!(child, Some(Outcome::Cancelled)) {
            match recast.stage {
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
        let hook =
            |game: &mut Self, name| game.queue_timing(name, Some(seat), None, None, 1, &[card]);
        loop {
            let queued = match recast.stage {
                Before => {
                    recast.stage = Begin;
                    hook(self, "recastBefore")
                }
                Begin => {
                    recast.stage = Announce;
                    hook(self, "recastBegin")
                }
                Announce => {
                    if self.recast_held(seat, card) {
                        self.known_to_all.insert(card.id);
                    }
                    recast.stage = LossFactory;
                    self.next(EventKind::RecastReveal { seat, card });
                    true
                }
                LossFactory => {
                    let pre = recast.request(self, "recastingLose");
                    let loss = (recast.rule.loss)(RecastState(self), seat, card);
                    recast.post = recast.request(self, "recastingLost");
                    recast.stage = RunLoss(loss);
                    self.dispatch_recast_hook(pre)
                }
                RunLoss(loss) => {
                    let movement = match loss {
                        RecastLoss::Discard => Movement::new(seat, MoveKind::DiscardToPile(card)),
                        RecastLoss::Give { to } if to != seat && self.recast_alive(to) => {
                            Movement::new(
                                to,
                                MoveKind::Gain {
                                    card,
                                    origin: GainOrigin::Hand { seat, public: true },
                                },
                            )
                        }
                        _ => {
                            recast.stage = Cancel(None);
                            continue;
                        }
                    };
                    recast.stage = Lost(loss);
                    self.scheduler
                        .next(Program::Recast(Flow::Movement(movement)));
                    true
                }
                Lost(kind) => {
                    let Some(Outcome::Finished(Resolution::RecastMove {
                        event, card: moved, ..
                    })) = child
                    else {
                        recast.stage = Cancel(None);
                        continue;
                    };
                    let movement = RecastMovement {
                        event,
                        kind,
                        card,
                        moved: moved == Some(card),
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
                    hook(self, "recast")
                }
                GainFactory(loss) => {
                    let (pre, gain, post) = recast.plan_gain(self, loss);
                    let movement = match gain {
                        RecastGain::Draw => Some(Movement::new(seat, MoveKind::Draw)),
                        RecastGain::FromDeck { card: id } => RecastState(self)
                            .draw_pile()
                            .iter()
                            .find(|c| c.id == id)
                            .map(|&card| {
                                Movement::new(
                                    seat,
                                    MoveKind::Gain {
                                        card,
                                        origin: GainOrigin::DrawPile,
                                    },
                                )
                            }),
                    };
                    recast.post = post;
                    recast.stage = RunGain(movement);
                    self.dispatch_recast_hook(pre)
                }
                RunGain(Some(movement)) => {
                    recast.stage = Gained;
                    self.scheduler
                        .next(Program::Recast(Flow::Movement(movement)));
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
                    hook(self, "recastEnd")
                }
                After => {
                    recast.stage = Finish;
                    hook(self, "recastAfter")
                }
                Cancel(remaining) => {
                    recast.stage = FinishCancelled(remaining);
                    if recast.cancelled_before_movement {
                        self.dispatch_recast_hook(recast.post.take())
                    } else {
                        hook(self, "recastCancelled")
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
                        let _ = recast.plan_gain(self, loss);
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
                    if !self.recast_alive(seat)
                        && !matches!(
                            movement.kind,
                            Lose {
                                zone: Zone::Expansion,
                                ..
                            }
                        )
                    {
                        movement.stage = Cancel(MoveCancellation::InvalidMovement);
                        continue;
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
                        }) && !matches!(destination, LossDestination::Transfer { to, .. } if !self.recast_alive(to)) =>
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
                        if !self.recast_alive(seat) {
                            if self.recast_alive(from) && !from_expansion {
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
mod tests;
