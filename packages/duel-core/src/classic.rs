pub(crate) mod add_judge;
pub(crate) mod basic_use;
#[cfg(test)]
mod combined_mount_tests;
pub(crate) mod compare;
pub(crate) mod counter;
pub(crate) mod elemental;
pub(crate) mod equipment;
#[cfg(test)]
mod equipment_tests;
pub(crate) mod expansion;
pub(crate) mod hero_integration;
pub(crate) mod judge_area;
pub(crate) mod judge_registry;
mod lifecycle;
pub(crate) mod max_hp;
pub(crate) mod multi_compare;
pub(crate) mod phases;
pub(crate) mod pile_order;
pub(crate) mod recast;
pub(crate) mod rest;
pub(crate) mod revival;
#[cfg(test)]
mod sha_cancel_tests;
pub(crate) mod skills;
pub(crate) mod target_timing;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod tricks;
#[cfg(test)]
mod tricks_tests;
pub(crate) mod turns;

use crate::classic_data::{self, Role, Side};
use crate::{
    Action, Card, CardFace, CardId, CardKind, CardZone, ChoiceKind, Command, CounterContext,
    Decision, EventKind, Game, Program, RuleError, Seat, SeatId, TurnView,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MatchResult {
    Winner(Side),
    Draw,
}

impl MatchResult {
    pub(super) fn winner(self) -> Option<Side> {
        match self {
            Self::Winner(side) => Some(side),
            Self::Draw => None,
        }
    }
}

pub(super) struct Classic {
    pub(super) human: SeatId,
    lord: SeatId,
    pub(super) deck: Vec<Card>,
    pub(super) faces: Vec<CardFace>,
    random: TableRandom,
    pub(super) turn: Option<TurnView>,
    sha_played: bool,
    hero_turn: skills::HeroTurn,
    pub(super) result: Option<MatchResult>,
    result_announced: bool,
}

struct TableRandom(u64);

pub(crate) struct PlannedUse {
    targets: Vec<SeatId>,
    program: Program,
}

impl TableRandom {
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for index in (1..items.len()).rev() {
            items.swap(index, (self.next() % (index as u64 + 1)) as usize);
        }
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }
}

impl Game {
    /// Start an ordinary classic identity table at the lord's first play phase.
    pub fn classic(
        seed: u32,
        human: SeatId,
        hero_id: &str,
        count: usize,
    ) -> Result<Self, RuleError> {
        Self::classic_with_triggers(seed, human, hero_id, count, Vec::new())
    }

    /// Start a classic table with callbacks installed before its opening event.
    pub fn classic_with_triggers(
        seed: u32,
        human: SeatId,
        hero_id: &str,
        count: usize,
        registrations: Vec<crate::TriggerRegistration>,
    ) -> Result<Self, RuleError> {
        Self::classic_with_mount_slots_and_triggers(
            seed,
            human,
            hero_id,
            count,
            crate::MountSlots::Separate,
            registrations,
        )
    }

    pub fn classic_with_mount_slots(
        seed: u32,
        human: SeatId,
        hero_id: &str,
        count: usize,
        mode: crate::MountSlots,
    ) -> Result<Self, RuleError> {
        Self::classic_with_mount_slots_and_triggers(seed, human, hero_id, count, mode, Vec::new())
    }

    pub fn classic_with_mount_slots_and_triggers(
        seed: u32,
        human: SeatId,
        hero_id: &str,
        count: usize,
        mode: crate::MountSlots,
        registrations: Vec<crate::TriggerRegistration>,
    ) -> Result<Self, RuleError> {
        let mut roles =
            classic_data::deal_roles(count).ok_or(RuleError::UnsupportedSeatCount { count })?;
        if human as usize >= count {
            return Err(RuleError::UnknownSeat { seat: human });
        }
        let roster = classic_data::roster();
        let hero = roster
            .iter()
            .find(|hero| hero.id == hero_id)
            .ok_or(RuleError::UnknownHero)?;

        let mut random = TableRandom(u64::from(seed));
        random.shuffle(&mut roles);
        let lord = roles
            .iter()
            .position(|role| *role == Role::Lord)
            .expect("the table has a lord") as SeatId;
        let mut others: Vec<_> = roster
            .iter()
            .filter(|candidate| candidate.id != hero_id)
            .collect();
        random.shuffle(&mut others);
        let seats = roles
            .into_iter()
            .enumerate()
            .map(|(index, role)| {
                let id = index as SeatId;
                let general = if id == human {
                    hero
                } else {
                    others.pop().expect("there are enough generals")
                };
                let max_hp = i32::from(general.max_hp) + i32::from(role == Role::Lord && count > 4);
                Seat {
                    id,
                    hp: max_hp,
                    max_hp,
                    alive: true,
                    out: false,
                    rest_clock: None,
                    rest_mark_shown: false,
                    rest_undist: false,
                    rest_trigger: None,
                    qinggang_uses: Vec::new(),
                    linked: false,
                    turned_over: false,
                    armor: 0,
                    hero: Some(general),
                    role: Some(role),
                    role_shown: role == Role::Lord,
                    turn_kills: 0,
                    hand: Vec::new(),
                    equipment: crate::equipment_registry::EquipmentArea::new(id),
                    judges: Vec::new(),
                    equipment_expanded: [0; 5],
                    equipment_disabled: [0; 5],
                    equipment_markers: [0; 5],
                    judge_disabled: false,
                    judge_disabled_marker: false,
                    expansions: Vec::new(),
                }
            })
            .collect();

        let raw_deck = classic_data::standard_deck();
        let mut faces = Vec::with_capacity(raw_deck.len());
        let mut deck: Vec<Card> = raw_deck
            .into_iter()
            .map(|physical| {
                faces.push(CardFace {
                    id: physical.id,
                    suit: physical.suit,
                    rank: physical.rank,
                });
                let kind = serde_json::from_value(serde_json::Value::String(physical.kind))
                    .expect("each bundled card kind has a protocol identifier");
                Card {
                    id: physical.id,
                    kind,
                }
            })
            .collect();
        random.shuffle(&mut deck);

        let mut game = Self::with_seats(seats, lord);
        game.mount_slots = mode;
        game.classic = Some(Classic {
            human,
            lord,
            deck,
            faces,
            random,
            turn: None,
            sha_played: false,
            hero_turn: skills::HeroTurn::default(),
            result: None,
            result_announced: false,
        });
        for registration in registrations {
            game.register_trigger(registration)?;
        }
        game.scheduler
            .start(Program::event(EventKind::Opening { lord }));
        game.run(None);
        Ok(game)
    }

    pub(super) fn match_result(&self) -> Option<MatchResult> {
        self.classic.as_ref().and_then(|state| state.result)
    }

    fn face(&self, id: CardId) -> Option<CardFace> {
        self.classic
            .as_ref()?
            .faces
            .iter()
            .find(|face| face.id == id)
            .copied()
    }

    fn draw_from_piles(&mut self, amount: usize) -> Option<Vec<Card>> {
        let state = self.classic.as_mut().expect("the table is classic");
        if state.deck.len() + self.discard.len() < amount {
            state.result = Some(MatchResult::Draw);
            return None;
        }
        let mut cards = Vec::with_capacity(amount);
        for _ in 0..amount {
            if state.deck.is_empty() {
                for card in &self.discard {
                    self.known_to_all.remove(&card.id);
                    self.known_to_seats.remove(&card.id);
                }
                state.deck.append(&mut self.discard);
                state.random.shuffle(&mut state.deck);
            }
            cards.push(
                state
                    .deck
                    .pop()
                    .expect("the draw was checked against both piles"),
            );
        }
        Some(cards)
    }

    pub(crate) fn random_pile_card(
        &mut self,
        mut filter: impl FnMut(Card) -> bool,
    ) -> Option<(CardZone, Card)> {
        let state = self.classic.as_mut()?;
        if !state.deck.is_empty() {
            let len = state.deck.len();
            let start = (state.random.next() % len as u64) as usize;
            if let Some(card) = state
                .deck
                .iter()
                .rev()
                .cycle()
                .skip(start)
                .take(len)
                .copied()
                .find(|&card| filter(card))
            {
                return Some((CardZone::DrawPile, card));
            }
        }
        let len = self.discard.len();
        if len == 0 {
            return None;
        }
        let start = (state.random.next() % len as u64) as usize;
        self.discard
            .iter()
            .cycle()
            .skip(start)
            .take(len)
            .copied()
            .find(|&card| filter(card))
            .map(|card| (CardZone::DiscardPile, card))
    }

    pub(super) fn reveal_cards(&mut self, amount: usize) -> Option<Vec<Card>> {
        let cards = self.draw_from_piles(amount)?;
        self.resolving.extend(&cards);
        Some(cards)
    }

    fn random_hand_card(&mut self, seat: SeatId) -> Option<Card> {
        let len = self.seat(seat).ok()?.hand.len();
        if len == 0 {
            return None;
        }
        let index = (self.classic.as_mut()?.random.next() % len as u64) as usize;
        self.seat(seat).ok()?.hand.get(index).copied()
    }

    fn skip_play_phase(&mut self, seat: SeatId) {
        self.skip_next_phase(seat, crate::Phase::Play)
            .expect("judged seat exists");
    }

    pub(super) fn play_choices(&self, seat: SeatId) -> Vec<Action> {
        let player = self.seat(seat).expect("the acting seat exists");
        let mut actions = vec![Action::EndPlay];
        for card in &player.hand {
            if (card.kind == CardKind::Tiesuo || !self.recast_rules.is_empty())
                && self.can_recast(seat, card.id)
            {
                actions.push(Action::Recast { card: card.id });
            }
            if let Some(uses) = self.card_uses(player, *card) {
                actions.extend(uses.into_iter().map(|planned| Action::PlayCard {
                    card: card.id,
                    targets: planned.targets,
                }));
            }
        }
        actions
    }

    fn card_uses(&self, player: &Seat, card: Card) -> Option<Vec<PlannedUse>> {
        let seat = player.id;
        if !self.card_enabled(seat, card.kind) {
            return Some(Vec::new());
        }
        let uses = match card.kind {
            CardKind::Sha => self.physical_sha_uses(player, card),
            CardKind::Tao if player.hp < player.max_hp => vec![PlannedUse {
                targets: vec![seat],
                program: Program::event(EventKind::UseTao {
                    seat,
                    target: seat,
                    card,
                }),
            }],
            CardKind::Shan | CardKind::Tao => Vec::new(),
            _ => {
                return self
                    .equipment_uses(player, card)
                    .or_else(|| self.trick_uses(player, card))
            }
        };
        Some(uses)
    }

    pub(super) fn plan_play(
        &self,
        seat: SeatId,
        card: CardId,
        targets: Vec<SeatId>,
    ) -> Result<Program, RuleError> {
        self.checked_play_seat(seat)?;
        let player = self.seat(seat)?;
        let card = player
            .hand
            .iter()
            .find(|held| held.id == card)
            .copied()
            .ok_or(RuleError::InvalidCard)?;
        self.card_uses(player, card)
            .ok_or(RuleError::UnsupportedCard)?
            .into_iter()
            .find(|planned| {
                planned.targets == targets
                    || matches!(card.kind, CardKind::Sha | CardKind::Tiesuo)
                        && planned.targets.len() == targets.len()
                        && planned
                            .targets
                            .iter()
                            .all(|target| targets.contains(target))
            })
            .map(|planned| planned.program)
            .ok_or(RuleError::InvalidTarget)
    }

    /// Choose a single legal command for the next non-human seat.
    pub fn bot_command(&self) -> Option<Command> {
        let state = self.classic.as_ref()?;
        if self.round_suspended() {
            return Some(Command {
                version: self.version,
                seat: state.human,
                action: Action::AdvanceRound,
            });
        }
        let decision = self.decision.as_ref()?;
        let seat = decision.seat();
        if seat == state.human {
            return None;
        }
        let player = self.seat(seat).ok()?;
        let action = match decision {
            Decision::Play(_) => {
                let action = self.bot_play(player);
                if action == Action::EndPlay {
                    self.bot_hero_play(seat)
                        .or_else(|| self.bot_active_skill(seat))
                        .unwrap_or(action)
                } else {
                    action
                }
            }
            Decision::Cards(choice) => match choice.kind {
                ChoiceKind::Discard => {
                    let mut cards: Vec<_> = player
                        .hand
                        .iter()
                        .filter(|card| choice.cards.contains(&card.id))
                        .collect();
                    cards.sort_by_key(|card| match card.kind {
                        CardKind::Tao => 3,
                        CardKind::Shan => 2,
                        CardKind::Sha => 1,
                        _ => 0,
                    });
                    Action::Discard {
                        event: choice.event,
                        cards: cards
                            .into_iter()
                            .take(choice.min)
                            .map(|card| card.id)
                            .collect(),
                    }
                }
                ChoiceKind::Save => {
                    let save_lord = choice.target == Some(state.lord)
                        && (player.role == Some(Role::Loyalist)
                            || (player.role == Some(Role::Renegade)
                                && self.seats.iter().filter(|seat| seat.is_in()).count() > 2));
                    let card = if choice.target == Some(seat) || save_lord {
                        choice.cards.first().copied()
                    } else {
                        None
                    };
                    Action::Choose {
                        event: choice.event,
                        card,
                    }
                }
                ChoiceKind::Shan => Action::Choose {
                    event: choice.event,
                    card: choice.cards.first().copied(),
                },
                ChoiceKind::Content if choice.equipment_buttons.is_some() => {
                    let buttons = choice.equipment_buttons.as_ref()?;
                    Action::SelectEquipmentButtons {
                        event: choice.event,
                        buttons: buttons
                            .options
                            .iter()
                            .take(buttons.required)
                            .map(|option| option.id)
                            .collect(),
                    }
                }
                ChoiceKind::Content if choice.move_cards.is_some() => {
                    self.bot_move_choice(choice.event)?
                }
                ChoiceKind::Content
                    if choice.compare.is_some() || choice.compare_group.is_some() =>
                {
                    let bid = choice.branches.first()?;
                    let card = bid.cards.iter().min_by_key(|&&id| {
                        std::cmp::Reverse(self.face(id).map_or(0, |face| face.rank))
                    })?;
                    Action::Select {
                        event: choice.event,
                        branch: bid.id,
                        cards: vec![*card],
                        targets: vec![],
                    }
                }
                ChoiceKind::Content => {
                    let counter = choice
                        .counter
                        .is_some_and(|context| self.bot_wants_counter(player, context, state.lord));
                    let branch = choice
                        .branches
                        .iter()
                        .filter(|branch| {
                            (choice.counter.is_none() || counter || branch.id == 0)
                                && branch.min_cards <= branch.max_cards
                                && branch.min_cards <= branch.cards.len()
                                && branch.min_targets <= branch.max_targets
                                && branch.min_targets <= branch.targets.len()
                        })
                        .min_by_key(|branch| branch.id == 0)?;
                    Action::Select {
                        event: choice.event,
                        branch: branch.id,
                        cards: branch
                            .cards
                            .iter()
                            .take(branch.min_cards)
                            .copied()
                            .collect(),
                        targets: branch
                            .targets
                            .iter()
                            .take(branch.min_targets)
                            .copied()
                            .collect(),
                    }
                }
            },
        };
        Some(Command {
            version: self.version,
            seat,
            action,
        })
    }

    fn bot_play(&self, player: &Seat) -> Action {
        let state = self.classic.as_ref().expect("the table is classic");
        let lord = state.lord;
        self.play_choices(player.id)
            .into_iter()
            .max_by_key(|action| match action {
                Action::PlayCard { card, targets } => {
                    let held = player
                        .hand
                        .iter()
                        .find(|held| held.id == *card)
                        .expect("the option uses a held card");
                    if held.kind == CardKind::Tao {
                        (6, 0)
                    } else if equipment::is_equipment(held.kind) {
                        (5, 0)
                    } else {
                        self.bot_card_priority(player, held.kind, targets, lord)
                    }
                }
                // Cycle one card as a fallback. A cancelled attempt must not keep the turn alive.
                Action::Recast { .. } if !state.hero_turn.recast_attempted => (1, 1),
                Action::Recast { .. } => (0, 0),
                _ => (1, 0),
            })
            .expect("ending play is always available")
    }

    fn bot_wants_counter(&self, actor: &Seat, context: CounterContext, lord: SeatId) -> bool {
        use CardKind::*;
        let attitude = |target| {
            if target == actor.id {
                return 2;
            }
            if target != lord {
                return 0;
            }
            match actor.role {
                Some(Role::Lord | Role::Loyalist) => 2,
                Some(Role::Rebel) => -2,
                Some(Role::Renegade)
                    if self.seats.iter().filter(|seat| seat.is_in()).count() == 2 =>
                {
                    -2
                }
                Some(Role::Renegade) if self.seat(lord).is_ok_and(|seat| seat.hp <= 2) => 1,
                _ => 0,
            }
        };
        let target_attitude = attitude(context.target);
        let source_attitude = context.source.map_or(0, attitude);
        let value = match context.card_kind {
            Wuzhong | Wugu => target_attitude,
            Taoyuan
                if self
                    .seat(context.target)
                    .is_ok_and(|seat| seat.hp < seat.max_hp) =>
            {
                target_attitude
            }
            Juedou | Shunshou | Guohe | Jiedao => -2 * target_attitude + source_attitude,
            Nanman | Wanjian | Lebu | Shandian => -target_attitude,
            _ => 0,
        };
        if context.negated {
            value > 0
        } else {
            value < 0
        }
    }

    fn bot_card_priority(
        &self,
        actor: &Seat,
        kind: CardKind,
        targets: &[SeatId],
        lord: SeatId,
    ) -> (u8, i64) {
        use CardKind::*;
        let court = matches!(actor.role, Some(Role::Lord | Role::Loyalist));
        let targets_lord = targets.contains(&lord);
        match kind {
            Wuzhong => (5, 0),
            Sha | Juedou | Guohe | Shunshou | Lebu | Jiedao => {
                let target = if kind == Jiedao {
                    targets.get(1)
                } else {
                    targets.first()
                };
                let priority = match target.copied() {
                    Some(target) if target == actor.id || (target == lord && court) => 0,
                    Some(target) if target == lord => 4,
                    _ => 3,
                };
                let hp = target
                    .and_then(|target| self.seat(*target).ok())
                    .map_or(0, |target| target.hp);
                (priority, -i64::from(hp))
            }
            Nanman | Wanjian if targets_lord && court => (0, 0),
            Taoyuan
                if targets_lord
                    && !court
                    && self.seat(lord).is_ok_and(|lord| lord.hp < lord.max_hp) =>
            {
                (0, 0)
            }
            _ => (2, 0),
        }
    }
}
