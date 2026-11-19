use crate::scheduler::{EventRef, Outcome};
use crate::{
    Card, CardId, CardKind, EventId, EventKind, Game, Program, Resolution, RuleError, SeatId,
    TriggerEvent,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

mod damage;
pub(crate) use damage::CardDamage;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DamageNature {
    Fire,
    Thunder,
}

/// A committed damage event. Both source and recipient refer to this same event.
#[derive(Clone, Debug)]
pub struct DamageRecord {
    pub event: EventId,
    pub parent: Option<EventId>,
    pub turn: Option<u64>,
    pub source: Option<SeatId>,
    pub target: SeatId,
    pub amount: u16,
    pub absorbed: u16,
    pub nature: Option<DamageNature>,
    pub card_kind: Option<CardKind>,
    pub cards: Vec<Card>,
    pub propagated: bool,
    may_spread: bool,
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) natures: BTreeMap<CardId, DamageNature>,
    history: Vec<DamageRecord>,
}

/// The original subject of a hit is preserved when the chain creates a new damage event.
#[derive(Clone, Default)]
pub(crate) struct DamageCard {
    pub(crate) kind: Option<CardKind>,
    pub(crate) cards: Vec<Card>,
    pub(crate) nature: Option<DamageNature>,
}

#[derive(Clone)]
pub(crate) enum Flow {
    Change(SeatChange),
    Hp(HealthChange),
    Tiesuo {
        seat: SeatId,
        target: SeatId,
    },
    Unlink {
        damage: usize,
        seat: SeatId,
    },
    Spread {
        damage: usize,
        seat: SeatId,
        pending: Option<VecDeque<SeatId>>,
    },
}

impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::Change(change) => change.kind(),
            Self::Hp(hp) => EventKind::ChangeHp {
                seat: hp.seat,
                before: hp.before,
                after: hp.after,
            },
            Self::Tiesuo { seat, target } => EventKind::TrickEffect {
                seat: *seat,
                target: *target,
                kind: CardKind::Tiesuo,
            },
            Self::Unlink { seat, .. } => EventKind::UnlinkAfterDamage { seat: *seat },
            Self::Spread { seat, .. } => EventKind::SpreadDamage { seat: *seat },
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Change {
    Link(bool),
    TurnOver(bool),
    Armor(i32),
}

#[derive(Clone)]
pub(crate) enum ContentAction {
    GainCreated {
        seat: SeatId,
        card: Card,
    },
    Link {
        seat: SeatId,
    },
    TurnOver {
        seat: SeatId,
    },
    Armor {
        seat: SeatId,
        delta: i32,
    },
    Damage {
        source: Option<SeatId>,
        target: SeatId,
        amount: u16,
        card: DamageCard,
    },
    LoseHp {
        seat: SeatId,
        amount: u16,
    },
    Draw {
        seat: SeatId,
    },
    DrawMany {
        seat: SeatId,
        count: u16,
    },
}

impl ContentAction {
    pub(crate) fn program(self, game: &Game) -> Option<Program> {
        let seat = match &self {
            Self::Link { seat, .. }
            | Self::TurnOver { seat, .. }
            | Self::Armor { seat, .. }
            | Self::LoseHp { seat, .. }
            | Self::Draw { seat }
            | Self::DrawMany { seat, .. }
            | Self::GainCreated { seat, .. } => *seat,
            Self::Damage { target, .. } => *target,
        };
        let player = game.seat(seat).ok().filter(|p| p.alive)?;
        match self {
            Self::Link { .. } => game.seat_change(seat, Change::Link(!player.linked)),
            Self::TurnOver { .. } => game.seat_change(seat, Change::TurnOver(!player.turned_over)),
            Self::Armor { delta, .. } => game.seat_change(seat, Change::Armor(delta)),
            Self::Damage {
                source,
                target,
                amount,
                card,
            } => Some(game.elemental_damage_program(source, target, amount, card, false)),
            Self::LoseHp { amount, .. } => Some(game.lose_hp_program(seat, amount)),
            Self::Draw { .. } => Some(game.draw_one_program(seat)),
            Self::DrawMany { count, .. } => Some(game.draw_many_program(seat, count)),
            Self::GainCreated { card, .. } => Some(game.created_gain_program(seat, card)),
        }
    }
}

impl crate::TriggerContext<'_> {
    /// Toggle when the desired state is omitted. Repeating an explicit state is a no-op.
    pub fn link(&mut self, seat: SeatId, linked: Option<bool>) -> Result<(), RuleError> {
        let player = self.game().seat(seat)?;
        if !player.alive {
            return Err(RuleError::InactiveSeat { seat });
        }
        if linked == Some(player.linked) {
            return Ok(());
        }
        self.enqueue_elemental(seat, ContentAction::Link { seat })
    }

    pub fn turn_over(&mut self, seat: SeatId, turned_over: Option<bool>) -> Result<(), RuleError> {
        let player = self.game().seat(seat)?;
        if !player.alive {
            return Err(RuleError::InactiveSeat { seat });
        }
        if turned_over == Some(player.turned_over) {
            return Ok(());
        }
        self.enqueue_elemental(seat, ContentAction::TurnOver { seat })
    }

    pub fn change_armor(&mut self, seat: SeatId, delta: i32) -> Result<(), RuleError> {
        self.enqueue_elemental(seat, ContentAction::Armor { seat, delta })
    }

    /// Queue a hit with an optional effective card and its physical materials.
    pub fn damage(
        &mut self,
        source: Option<SeatId>,
        target: SeatId,
        amount: u16,
        nature: Option<DamageNature>,
        kind: Option<CardKind>,
        cards: &[Card],
    ) -> Result<(), RuleError> {
        if let Some(source) = source {
            self.game().seat(source)?;
        }
        let card = DamageCard {
            nature,
            kind,
            cards: cards.to_vec(),
        };
        self.enqueue_elemental(
            target,
            ContentAction::Damage {
                source,
                target,
                amount,
                card,
            },
        )
    }

    pub fn lose_hp(&mut self, seat: SeatId, amount: u16) -> Result<(), RuleError> {
        self.enqueue_elemental(seat, ContentAction::LoseHp { seat, amount })
    }

    /// Draw one card through the draw and private gain events.
    pub fn draw(&mut self, seat: SeatId) -> Result<(), RuleError> {
        if self.game().classic.is_none() {
            return Err(RuleError::InvalidSelection);
        }
        self.enqueue_elemental(seat, ContentAction::Draw { seat })
    }

    /// Draw the requested number in one draw and one private gain.
    pub fn draw_cards(&mut self, seat: SeatId, count: u16) -> Result<(), RuleError> {
        if self.game().classic.is_none() {
            return Err(RuleError::InvalidSelection);
        }
        self.enqueue_elemental(seat, ContentAction::DrawMany { seat, count })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SeatChange {
    seat: SeatId,
    change: Change,
    stage: Stage,
}

#[derive(Clone, Copy)]
enum Stage {
    Before,
    Begin,
    Apply,
    End,
    After,
    Finish,
    Cancel,
    FinishCancelled,
}

impl SeatChange {
    fn kind(self) -> EventKind {
        let seat = self.seat;
        match self.change {
            Change::Link(linked) => EventKind::Link { seat, linked },
            Change::TurnOver(turned_over) => EventKind::TurnOver { seat, turned_over },
            Change::Armor(delta) => EventKind::ChangeArmor { seat, delta },
        }
    }

    fn timing(self) -> &'static str {
        let names = match self.change {
            Change::Link(_) => [
                "linkBefore",
                "linkBegin",
                "linkEnd",
                "linkAfter",
                "linkCancelled",
            ],
            Change::TurnOver(_) => [
                "turnOverBefore",
                "turnOverBegin",
                "turnOverEnd",
                "turnOverAfter",
                "turnOverCancelled",
            ],
            Change::Armor(_) => [
                "changeHujiaBefore",
                "changeHujiaBegin",
                "changeHujiaEnd",
                "changeHujiaAfter",
                "changeHujiaCancelled",
            ],
        };
        names[match self.stage {
            Stage::Before => 0,
            Stage::Begin => 1,
            Stage::End => 2,
            Stage::After => 3,
            Stage::Cancel => 4,
            _ => unreachable!("only lifecycle stages request a timing"),
        }]
    }
}

#[derive(Clone)]
pub(crate) struct HealthChange {
    seat: SeatId,
    before: i32,
    after: i32,
    delta: i64,
    damage: Option<usize>,
    stage: Stage,
    awaiting_timing: bool,
}

impl HealthChange {
    fn values(&self, game: &Game) -> (i32, i32, i64, u16) {
        let player = game.seat(self.seat).expect("the changed seat exists");
        let absorbed = if self.damage.is_some() && self.delta < 0 {
            self.delta.unsigned_abs().min(u64::from(player.armor)) as u16
        } else {
            0
        };
        let delta = self.delta + i64::from(absorbed);
        let signed = i64::from(player.hp)
            .saturating_add(delta)
            .clamp(i64::from(i32::MIN), i64::from(player.max_hp)) as i32;
        let after = if game.classic.is_some() || !player.alive {
            signed
        } else {
            signed.max(0)
        };
        (player.hp, after, delta, absorbed)
    }

    pub(crate) fn announced(&self, game: &Game) -> EventKind {
        // The started record exposes the expected transition. Before/Begin content may
        // intervene before the engine computes the actual transition for the finished record.
        let (before, after, _, _) = self.values(game);
        EventKind::ChangeHp {
            seat: self.seat,
            before,
            after,
        }
    }
}

/// Built-in rules share the ordered candidate list with content registrations.
#[derive(Clone, Copy)]
pub(crate) enum Rule {
    Unlink(usize),
    Spread(usize),
}

impl Rule {
    pub(crate) fn skill(self) -> &'static str {
        match self {
            Self::Unlink(_) => "_lianhuan4",
            Self::Spread(_) => "_lianhuan",
        }
    }

    pub(crate) fn priority(self) -> i32 {
        match self {
            Self::Unlink(_) => -10,
            Self::Spread(_) => 0,
        }
    }

    pub(crate) fn owner(self, game: &Game) -> SeatId {
        game.elemental.history[self.index()].target
    }

    fn index(self) -> usize {
        match self {
            Self::Unlink(index) | Self::Spread(index) => index,
        }
    }

    pub(crate) fn eligible(self, game: &Game) -> bool {
        let record = &game.elemental.history[self.index()];
        match self {
            Self::Unlink(_) => {
                record.nature.is_some() && game.seat(record.target).is_ok_and(|s| s.linked)
            }
            Self::Spread(_) => record.may_spread,
        }
    }

    pub(crate) fn program(self, game: &Game) -> Program {
        let seat = self.owner(game);
        Program::Elemental(match self {
            Self::Unlink(damage) => Flow::Unlink { damage, seat },
            Self::Spread(damage) => Flow::Spread {
                damage,
                seat,
                pending: None,
            },
        })
    }
}

impl Game {
    pub(crate) fn physical_cards(&self) -> impl Iterator<Item = &Card> {
        self.seats
            .iter()
            .flat_map(|s| {
                s.hand
                    .iter()
                    .chain(&s.equipment)
                    .chain(&s.judges)
                    .chain(&s.expansions)
            })
            .chain(self.classic.iter().flat_map(|table| &table.deck))
            .chain(&self.resolving)
            .chain(&self.discard)
            .chain(&self.transferring)
            .chain(&self.reserved_draws)
    }

    pub(crate) fn create_content_card(
        &mut self,
        kind: CardKind,
        suit: crate::classic_data::Suit,
        rank: u8,
        nature: Option<DamageNature>,
    ) -> Result<Card, RuleError> {
        if !(1..=13).contains(&rank) || (nature.is_some() && kind != CardKind::Sha) {
            return Err(RuleError::InvalidCard);
        }
        let table = self.classic.as_ref().ok_or(RuleError::InvalidSelection)?;
        // Faces also reserve the identities of cards created by other pending gains.
        let used: std::collections::HashSet<_> = self
            .physical_cards()
            .map(|c| c.id)
            .chain(table.faces.iter().map(|face| face.id))
            .collect();
        let highest = used.iter().max().copied().unwrap_or(0);
        let id = highest
            .checked_add(1)
            .or_else(|| (0..highest).find(|id| !used.contains(id)))
            .ok_or(RuleError::InvalidCard)?;
        self.classic
            .as_mut()
            .unwrap()
            .faces
            .push(crate::CardFace { id, suit, rank });
        if let Some(nature) = nature {
            self.elemental.natures.insert(id, nature);
        }
        Ok(Card { id, kind })
    }

    /// Set the physical Sha's nature for trusted content. A normal Sha has no nature.
    pub fn set_sha_nature(
        &mut self,
        card: CardId,
        nature: Option<DamageNature>,
    ) -> Result<(), RuleError> {
        if !self
            .physical_cards()
            .any(|c| c.id == card && c.kind == CardKind::Sha)
        {
            return Err(RuleError::InvalidCard);
        }
        if let Some(nature) = nature {
            self.elemental.natures.insert(card, nature);
        } else {
            self.elemental.natures.remove(&card);
        }
        Ok(())
    }

    pub fn sha_nature(&self, card: CardId) -> Option<DamageNature> {
        self.elemental.natures.get(&card).copied()
    }

    pub fn damage_history(&self) -> &[DamageRecord] {
        &self.elemental.history
    }

    pub fn damage_dealt(&self, seat: SeatId) -> impl Iterator<Item = &DamageRecord> {
        self.damage_history()
            .iter()
            .filter(move |hit| hit.source == Some(seat))
    }

    pub fn damage_received(&self, seat: SeatId) -> impl Iterator<Item = &DamageRecord> {
        self.damage_history()
            .iter()
            .filter(move |hit| hit.target == seat)
    }

    /// Remaining candidates on the active chain containing this event. Each is
    /// checked for life and link state again after all earlier damage resolves.
    pub fn pending_linked_targets(
        &self,
        mut subject: EventId,
    ) -> Option<impl Iterator<Item = SeatId> + '_> {
        loop {
            let (event, program) = self.scheduler.active_frame(subject)?;
            if let Program::Elemental(Flow::Spread {
                pending: Some(pending),
                ..
            }) = program
            {
                return Some(pending.iter().copied());
            }
            subject = event.parent?;
        }
    }

    pub(crate) fn physical_damage_card(
        &self,
        kind: Option<CardKind>,
        cards: Vec<Card>,
    ) -> DamageCard {
        let nature = match (kind, cards.as_slice()) {
            (Some(CardKind::Sha), [card]) if card.kind == CardKind::Sha => self.sha_nature(card.id),
            (Some(CardKind::Shandian), _) => Some(DamageNature::Thunder),
            _ => None,
        };
        DamageCard {
            kind,
            cards,
            nature,
        }
    }

    pub(crate) fn commit_damage(
        &mut self,
        event: EventRef,
        source: Option<SeatId>,
        target: SeatId,
        amount: u16,
        card: &DamageCard,
        propagated: bool,
    ) -> usize {
        let index = self.elemental.history.len();
        self.elemental.history.push(DamageRecord {
            event: event.id,
            parent: event.parent,
            turn: self
                .classic
                .as_ref()
                .and_then(|state| state.turn)
                .map(|turn| turn.number),
            source,
            target,
            amount,
            absorbed: 0,
            nature: card.nature,
            card_kind: card.kind,
            cards: card.cards.clone(),
            propagated,
            may_spread: false,
        });
        index
    }

    pub(crate) fn seat_change(&self, seat: SeatId, change: Change) -> Option<Program> {
        let player = self.seat(seat).ok()?;
        if !player.alive
            || matches!(change, Change::Link(linked) if linked == player.linked)
            || matches!(change, Change::TurnOver(turned_over) if turned_over == player.turned_over)
        {
            return None;
        }
        Some(Program::Elemental(Flow::Change(SeatChange {
            seat,
            change,
            stage: Stage::Before,
        })))
    }

    pub(crate) fn queue_health_change(
        &mut self,
        seat: SeatId,
        delta: impl Into<i64>,
        damage: Option<usize>,
    ) {
        let player = self.seat(seat).expect("the health change target exists");
        let before = player.hp;
        self.scheduler
            .next(Program::Elemental(Flow::Hp(HealthChange {
                seat,
                before,
                after: before,
                delta: delta.into(),
                damage,
                stage: Stage::Before,
                awaiting_timing: false,
            })));
    }

    pub(crate) fn resolve_elemental(&mut self, flow: Flow, child: Option<Outcome<Resolution>>) {
        match flow {
            Flow::Change(change) => self.resolve_seat_change(change, child),
            Flow::Hp(hp) => self.resolve_health_change(hp, child),
            Flow::Tiesuo { target, .. } => {
                if let Ok(player) = self.seat(target) {
                    if let Some(change) = self.seat_change(target, Change::Link(!player.linked)) {
                        self.scheduler.next(change);
                    }
                }
                self.finish();
            }
            Flow::Unlink { damage, seat } => {
                let record = &mut self.elemental.history[damage];
                record.may_spread = !record.propagated;
                if let Some(change) = self.seat_change(seat, Change::Link(false)) {
                    self.scheduler.next(change);
                }
                self.finish();
            }
            Flow::Spread {
                damage,
                seat,
                pending,
            } => {
                if self.match_result().is_some() {
                    self.finish();
                    return;
                }
                let mut pending = pending.unwrap_or_else(|| {
                    self.living_order_from(self.active_seat)
                        .into_iter()
                        .filter(|&other| other != seat && self.seat(other).is_ok_and(|s| s.linked))
                        .collect()
                });
                while let Some(target) = pending.pop_front() {
                    if self.seat(target).is_ok_and(|s| s.linked && s.alive) {
                        let original = &self.elemental.history[damage];
                        let card = DamageCard {
                            kind: original.card_kind,
                            nature: original.nature,
                            cards: original.cards.clone(),
                        };
                        let next = self.elemental_damage_program(
                            original.source,
                            target,
                            original.amount,
                            card,
                            true,
                        );
                        self.scheduler
                            .replace_current(Program::Elemental(Flow::Spread {
                                damage,
                                seat,
                                pending: Some(pending),
                            }));
                        self.scheduler.next(next);
                        return;
                    }
                }
                self.finish();
            }
        }
    }

    fn resolve_seat_change(&mut self, mut change: SeatChange, child: Option<Outcome<Resolution>>) {
        use Stage::*;
        if matches!(child, Some(Outcome::Cancelled)) && !matches!(change.stage, FinishCancelled) {
            change.stage = Cancel;
        }
        loop {
            match change.stage {
                Apply => {
                    let seat = self.seat_mut(change.seat);
                    match change.change {
                        Change::Link(_) => {
                            seat.linked = !seat.linked;
                            change.change = Change::Link(seat.linked);
                        }
                        Change::TurnOver(_) => {
                            seat.turned_over = !seat.turned_over;
                            change.change = Change::TurnOver(seat.turned_over);
                        }
                        Change::Armor(delta) => {
                            let after = i32::from(seat.armor)
                                .saturating_add(delta)
                                .clamp(0, i32::from(u16::MAX))
                                as u16;
                            change.change = Change::Armor(i32::from(after) - i32::from(seat.armor));
                            seat.armor = after;
                        }
                    }
                    change.stage = End;
                }
                Finish => {
                    self.scheduler
                        .replace_current(Program::Elemental(Flow::Change(change)));
                    self.finish();
                    return;
                }
                FinishCancelled => {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                stage => {
                    let timing = change.timing();
                    let delta = match change.change {
                        Change::Armor(delta) => Some(i64::from(delta)),
                        _ => None,
                    };
                    change.stage = match stage {
                        Before => Begin,
                        Begin => Apply,
                        End => After,
                        After => Finish,
                        Cancel => FinishCancelled,
                        _ => unreachable!(),
                    };
                    if let Some(dispatch) = self.state_timing(timing, change.seat, delta, None) {
                        self.scheduler
                            .replace_current(Program::Elemental(Flow::Change(change)));
                        self.scheduler.next(dispatch);
                        return;
                    }
                }
            }
        }
    }

    fn resolve_health_change(&mut self, mut hp: HealthChange, child: Option<Outcome<Resolution>>) {
        use Stage::*;
        if hp.awaiting_timing
            && matches!(child, Some(Outcome::Cancelled))
            && !matches!(hp.stage, FinishCancelled)
        {
            hp.stage = Cancel;
        }
        loop {
            let stage = hp.stage;
            let (timing, rule, movement) = match stage {
                Before => {
                    hp.stage = Begin;
                    ("changeHpBefore", None, None)
                }
                Begin => {
                    hp.stage = Apply;
                    ("changeHpBegin", None, None)
                }
                Apply => {
                    let (before, after, delta, absorbed) = hp.values(self);
                    hp.before = before;
                    hp.after = after;
                    hp.delta = delta;
                    let movement = if absorbed > 0 {
                        self.seat_change(hp.seat, Change::Armor(-i32::from(absorbed)))
                    } else {
                        None
                    };
                    let classic = self.classic.is_some();
                    let player = self.seat_mut(hp.seat);
                    player.hp = after;
                    if !classic {
                        player.alive = after > 0;
                    }
                    let rule = hp.damage.and_then(|index| {
                        let record = &mut self.elemental.history[index];
                        record.absorbed = absorbed;
                        record.nature.map(|_| Rule::Unlink(index))
                    });
                    // HP reflects absorption immediately. The armor child settles before
                    // the callbacks of the already requested changeHp timing.
                    hp.stage = End;
                    ("changeHp", rule, movement)
                }
                End => {
                    hp.stage = After;
                    ("changeHpEnd", None, None)
                }
                After => {
                    hp.stage = Finish;
                    ("changeHpAfter", None, None)
                }
                Cancel => {
                    hp.stage = FinishCancelled;
                    ("changeHpCancelled", None, None)
                }
                Finish | FinishCancelled => {
                    self.scheduler
                        .replace_current(Program::Elemental(Flow::Hp(hp)));
                    self.scheduler.end(Outcome::Finished(Resolution::HpChanged {
                        cancelled: matches!(stage, FinishCancelled),
                    }));
                    return;
                }
            };
            let has_movement = movement.is_some();
            if let Some(movement) = movement {
                self.scheduler.next(movement);
            }
            let callbacks = self.state_timing(timing, hp.seat, Some(hp.delta), rule);
            hp.awaiting_timing = callbacks.is_some();
            if let Some(callbacks) = callbacks {
                self.scheduler.next(callbacks);
            }
            if hp.awaiting_timing || has_movement {
                self.scheduler
                    .replace_current(Program::Elemental(Flow::Hp(hp)));
                return;
            }
        }
    }

    fn state_timing(
        &mut self,
        timing: &'static str,
        seat: SeatId,
        delta: Option<i64>,
        rule: Option<Rule>,
    ) -> Option<Program> {
        if rule.is_none() && !self.wants_timing(timing) {
            return None;
        }
        let mut event = self.subject_timing(timing, Some(seat), None, None, 0, &[]);
        event.delta = delta;
        self.timing_program(event, rule)
    }

    pub(crate) fn timing_program(
        &mut self,
        event: TriggerEvent,
        rule: Option<Rule>,
    ) -> Option<Program> {
        if !self.begin_timing_request(&event) && rule.is_none() {
            return None;
        }
        Some(Program::Dispatch(
            self.dispatch_for_timing(event).with_elemental_rule(rule),
        ))
    }
}

#[cfg(test)]
mod tests;
