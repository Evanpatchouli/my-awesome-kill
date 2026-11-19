mod active_skill;
pub use active_skill::{ActiveSkillId, ActiveSkillRegistration};
mod card_use;
#[cfg(any(test, feature = "content-multi-compare"))]
pub mod compare_content;
mod group_compare;
pub use group_compare::{
    CompareManyFrame, CompareManyResult, CompareMode, GroupAnnouncement, RevealedBid,
};
mod location;
pub use location::{CardLocation, CardZone};
mod marks;
pub use marks::SkillMark;
mod classic;
pub mod classic_data;
#[cfg(any(test, feature = "content-mingdian"))]
pub mod mingdian;
mod move_choice;
#[cfg(test)]
mod move_choice_tests;
#[cfg(any(test, feature = "content-vitality"))]
pub mod vitality_content;
#[cfg(any(test, feature = "content-xunxun"))]
pub mod xunxun;
pub use move_choice::{
    MoveBot, MoveChoiceRequest, MoveChoiceView, MoveConfirmFilter, MoveFilter, MoveResult, MoveRow,
    MoveRowView, MoveStep,
};
mod replay;
pub use replay::{RecordedGame, ReplayError, ReplayLog, ReplaySetup};
#[cfg(any(test, feature = "content-phase-skills"))]
pub mod phase_content;
mod protocol;
#[cfg(any(test, feature = "content-qiaoshui"))]
pub mod qiaoshui;
#[cfg(any(test, feature = "content-rest"))]
pub mod rest_content;
mod scheduler;
mod temporary;
mod triggers;
#[cfg(any(test, feature = "content-zhongyi"))]
pub mod zhongyi;
pub use temporary::{CardEnabled, SkillExpiry, SkillRemoval, SkillRemovalEffect, TemporarySkill};
pub use triggers::{
    TriggerContext, TriggerEffect, TriggerEvent, TriggerFilter, TriggerId, TriggerIndex,
    TriggerOrder, TriggerRegistration, TriggerRole, TriggerSelection, TriggerStep,
};

use std::collections::{BTreeMap, HashMap, HashSet};

use classic::elemental::{self, CardDamage};
pub use classic::elemental::{DamageNature, DamageRecord};
use classic::equipment;
use classic::hero_integration::Judgment;
pub use classic::judge_registry::{Judgment as JudgmentEntry, JudgmentKey};
pub use classic::max_hp::{MaxHpChange, MaxHpKind, MaxHpMetrics};
pub use classic::phases::PhaseInfo;
pub use classic::pile_order::PileOrderResult;
pub use classic::recast::{
    LossRecord, RecastBatchGain, RecastBatchGainFactory, RecastBatchLossFactory,
    RecastBatchMovement, RecastBatchRule, RecastFilter, RecastGain, RecastGainFactory, RecastKey,
    RecastLoss, RecastLossFactory, RecastMovement, RecastRule, RecastSeat, RecastState,
};
pub use classic::rest::{RestClock, RestUnit};
pub use classic::skills::SkillId;
use classic::skills::{HeroAdjustment, HeroFlow};
use classic::tricks::Trick;
pub use classic::turns::{PhasePlan, QueuedTurn, TurnInfo, TurnPosition};
use classic::{Classic, MatchResult};
use classic_data::{General, Role};
pub use protocol::*;

use scheduler::{EventRef, Outcome, Scheduler, Work};

#[derive(Clone)]
enum Resolution {
    BodySkipped,
    ComparedMany {
        event: EventId,
        cancelled: bool,
    },
    CounterUse {
        seat: SeatId,
        cards: Vec<Card>,
    },
    Countered(bool),
    ExpansionDiscarded(Vec<Card>),
    EquipReplacement {
        card: Option<Card>,
        cancelled: bool,
    },
    CompareBid(Card),
    LockedBids(Vec<Card>),
    Compared(CompareResult),
    RecastMove {
        event: EventId,
        card: Option<Card>,
        cancelled: bool,
    },
    BatchMove {
        event: EventId,
        cards: Vec<Card>,
        cancelled: bool,
    },
    Complete,
    Passed,
    Dodged,
    SavePassed(SeatId),
    Selected(Selection),
    MovedCards(MoveResult),
    PileOrdered(PileOrderResult),
    MaxHpChanged {
        cancelled: bool,
    },
    DeathCancelled,
    ReviveCancelled,
    RestCancelled,
    Responded,
    Judged(Card),
    HpChanged {
        cancelled: bool,
    },
    ForcedHit,
    Hero(HeroAdjustment),
}

enum Answer {
    Card(Card),
    Pass,
    Play(Program),
    EndPlay,
    Discard(Vec<CardId>),
    Select(Selection),
    MoveCards(Option<Vec<Vec<Card>>>),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Hand,
    Equipment,
    Judgment,
    Expansion,
}

#[derive(Clone, Copy)]
enum DeathStage {
    Before,
    Begin,
    Apply,
    Announce,
    Cleanup,
    Reward,
    End,
    After,
    Finish,
    FinishCancelled,
}

#[derive(Clone)]
struct DeathFlow {
    body_skipped: bool,
    source: Option<SeatId>,
    target: SeatId,
    role: Option<Role>,
    reserved: bool,
    include_out: bool,
    excluded_marks: Vec<String>,
    stage: DeathStage,
}

#[derive(Clone)]
enum Program {
    GroupCompare(classic::multi_compare::Flow),
    Bids(classic::compare::bids::BidFlow),
    Counter(classic::counter::Flow),
    Death(DeathFlow),
    ActiveSkill(active_skill::Flow),
    MoveChoice(move_choice::Flow),
    PileOrder(classic::pile_order::Flow),
    MaxHp(classic::max_hp::Flow),
    Revive(classic::revival::Flow),
    Rest(classic::rest::Flow),
    Expansion(classic::expansion::Flow),
    BasicUse(classic::basic_use::Flow),
    Compare(classic::compare::Flow),
    Turn(classic::turns::TurnFlow),
    PhaseLoop(classic::turns::LoopFlow),
    Phase(classic::phases::Flow),
    Elemental(elemental::Flow),
    Recast(classic::recast::Flow),
    Event(EventKind),
    DiscardFrom {
        seat: SeatId,
        zone: Zone,
        card: Card,
    },
    Choose(ChoiceRequest),
    CounterChoice {
        request: ChoiceRequest,
        context: CounterContext,
    },
    Equipment(equipment::Flow),
    Trick(Trick),
    TargetTiming(classic::target_timing::TargetTiming),
    Hero(HeroFlow),
    Dispatch(classic::skills::Dispatch),
    Registered(triggers::Registered),
    JudgeArea(classic::judge_area::Flow),
    EquipArea(classic::equipment::AreaFlow),
    AddJudge(classic::add_judge::Flow),
    DelayedPlacement(classic::add_judge::DelayedPlacement),
    Judgment(Judgment),
    JudgmentCleanup(classic::hero_integration::Cleanup),
    CardDamage(CardDamage),
    DiscardGroup {
        seat: SeatId,
        cards: Vec<(Zone, Card)>,
    },
    UseCosts {
        seat: SeatId,
        cards: Vec<(Zone, Card)>,
        paid: bool,
    },
    AnnounceUseCost {
        seat: SeatId,
        card: Card,
    },
    Discarded(Card),
}

impl Program {
    fn event(kind: EventKind) -> Self {
        match kind {
            EventKind::Death {
                source,
                target,
                role,
            } => Self::Death(DeathFlow {
                source,
                target,
                role,
                reserved: false,
                include_out: false,
                body_skipped: false,
                excluded_marks: Vec::new(),
                stage: DeathStage::Before,
            }),
            EventKind::UseTao { seat, target, card } => {
                Self::BasicUse(classic::basic_use::Flow::tao(seat, target, card, false))
            }
            EventKind::UseShan { seat, card } => {
                Self::BasicUse(classic::basic_use::Flow::shan(seat, card, true))
            }
            EventKind::Turn {
                seat,
                number,
                round,
            } => Self::Turn(classic::turns::TurnFlow::new(
                seat,
                number,
                round,
                number == 1,
                None,
            )),
            EventKind::Phase { seat, phase } => {
                Self::Phase(classic::phases::Flow::new(seat, phase, None, 0))
            }
            EventKind::PhaseLoop => Self::PhaseLoop(classic::turns::LoopFlow::new()),
            _ => Self::Event(kind),
        }
    }

    fn kind(&self) -> EventKind {
        match self {
            Self::GroupCompare(flow) => flow.kind(),
            Self::Bids(flow) => flow.kind(),
            Self::Counter(flow) => flow.kind(),
            Self::Death(flow) => EventKind::Death {
                source: flow.source,
                target: flow.target,
                role: flow.role,
            },
            Self::ActiveSkill(flow) => flow.kind(),
            Self::MoveChoice(flow) => flow.kind(),
            Self::PileOrder(flow) => flow.kind(),
            Self::MaxHp(flow) => flow.kind(),
            Self::Revive(flow) => flow.kind(),
            Self::Rest(flow) => flow.kind(),
            Self::Expansion(flow) => flow.kind(),
            Self::BasicUse(flow) => flow.kind(),
            Self::Event(kind) => *kind,
            Self::Compare(flow) => flow.kind(),
            Self::Turn(flow) => flow.kind(),
            Self::PhaseLoop(flow) => flow.kind(),
            Self::Phase(flow) => flow.kind(),
            Self::Elemental(flow) => flow.kind(),
            Self::Recast(flow) => flow.kind(),
            Self::DiscardFrom { card, .. } => EventKind::Discard { card: *card },
            Self::Choose(choice)
            | Self::CounterChoice {
                request: choice, ..
            } => EventKind::ChooseContent {
                seat: choice.seat,
                target: choice.target,
            },
            Self::Equipment(flow) => flow.kind(),
            Self::Trick(flow) => flow.kind(),
            Self::TargetTiming(flow) => flow.kind(),
            Self::Hero(flow) => flow.kind(),
            Self::Dispatch(flow) => flow.kind(),
            Self::Registered(flow) => flow.kind(),
            Self::JudgeArea(flow) => flow.kind(),
            Self::EquipArea(flow) => flow.kind(),
            Self::AddJudge(flow) => flow.kind(),
            Self::DelayedPlacement(placement) => placement.kind(),
            Self::Judgment(judgment) => EventKind::Judge {
                seat: judgment.seat,
            },
            Self::JudgmentCleanup(cleanup) => cleanup.kind(),
            Self::CardDamage(damage) => damage.kind(),
            Self::DiscardGroup { seat, cards } => EventKind::DiscardGroup {
                seat: *seat,
                amount: cards.len(),
            },
            Self::UseCosts { seat, cards, .. } => EventKind::Lose {
                seat: *seat,
                amount: cards.len(),
            },
            Self::AnnounceUseCost { seat, card } => EventKind::UseCost {
                seat: *seat,
                card: *card,
            },
            Self::Discarded(card) => EventKind::Discard { card: *card },
        }
    }

    fn respond(seat: SeatId, kind: CardKind) -> Self {
        Self::Equipment(equipment::Flow::respond(seat, kind))
    }
}

struct Seat {
    id: SeatId,
    hp: i32,
    max_hp: i32,
    alive: bool,
    out: bool,
    rest_clock: Option<RestClock>,
    rest_mark_shown: bool,
    rest_undist: bool,
    rest_trigger: Option<TriggerId>,
    linked: bool,
    turned_over: bool,
    armor: u16,
    hero: Option<&'static General>,
    role: Option<Role>,
    role_shown: bool,
    turn_kills: u64,
    hand: Vec<Card>,
    equipment: Vec<Card>,
    detached_equipment: Vec<Card>,
    judges: Vec<Card>,
    equipment_expanded: [usize; 5],
    equipment_disabled: [usize; 5],
    equipment_markers: [usize; 5],
    judge_disabled: bool,
    judge_disabled_marker: bool,
    expansions: Vec<Card>,
}

impl Seat {
    fn is_in(&self) -> bool {
        self.alive && !self.out
    }

    fn zone(&self, zone: Zone) -> &[Card] {
        match zone {
            Zone::Hand => &self.hand,
            Zone::Equipment => &self.equipment,
            Zone::Judgment => &self.judges,
            Zone::Expansion => &self.expansions,
        }
    }
    fn zone_mut(&mut self, zone: Zone) -> &mut Vec<Card> {
        match zone {
            Zone::Hand => &mut self.hand,
            Zone::Equipment => &mut self.equipment,
            Zone::Judgment => &mut self.judges,
            Zone::Expansion => &mut self.expansions,
        }
    }
}

impl From<SeatSetup> for Seat {
    fn from(setup: SeatSetup) -> Self {
        Self {
            id: setup.id,
            hp: i32::from(setup.hp),
            max_hp: i32::from(setup.hp),
            alive: setup.hp > 0,
            out: false,
            rest_clock: None,
            rest_mark_shown: false,
            rest_undist: false,
            rest_trigger: None,
            linked: false,
            turned_over: false,
            armor: 0,
            hero: None,
            role: None,
            role_shown: false,
            turn_kills: 0,
            hand: setup.hand,
            equipment: Vec::new(),
            detached_equipment: Vec::new(),
            judges: Vec::new(),
            equipment_expanded: [0; 5],
            equipment_disabled: [0; 5],
            equipment_markers: [0; 5],
            judge_disabled: false,
            judge_disabled_marker: false,
            expansions: Vec::new(),
        }
    }
}

pub struct Game {
    group_compares: classic::multi_compare::State,
    group_updates: Vec<GroupAnnouncement>,
    ordering_owners: HashMap<CardId, EventId>,
    private_ordering: HashSet<EventId>,
    marks: marks::State,
    indexed_triggers: BTreeMap<TriggerId, triggers::TriggerIndex>,
    force_die_triggers: HashSet<TriggerId>,
    force_out_triggers: HashSet<TriggerId>,
    turns: classic::turns::State,
    phase_state: classic::phases::State,
    active_skills: active_skill::State,
    expansion_tags: HashMap<CardId, Vec<String>>,
    teams: HashMap<SeatId, u32>,
    temporary_skills: Vec<temporary::Grant>,
    compares: classic::compare::State,
    #[cfg(test)]
    timing_observer: Option<triggers::TimingObserver>,
    elemental: elemental::State,
    recast_rules: Vec<(RecastKey, RecastRule)>,
    loss_history: Vec<LossRecord>,
    reserved_draws: Vec<Card>,
    transferring: Vec<Card>,
    known_to_all: HashSet<CardId>,
    known_to_seats: HashMap<CardId, Vec<SeatId>>,
    seats: Vec<Seat>,
    living_seats: Vec<SeatId>,
    dead_seats: Vec<SeatId>,
    active_seat: SeatId,
    version: u32,
    resolving: Vec<Card>,
    discard: Vec<Card>,
    judgment_registry: classic::judge_registry::Registry,
    decision: Option<Decision>,
    scheduler: Scheduler<Program, Resolution>,
    events: Vec<GameEvent>,
    classic: Option<Classic>,
    trigger_registrations: HashMap<TriggerId, TriggerRegistration>,
    next_trigger_id: TriggerId,
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
        let living_seats = seats.iter().filter(|s| s.alive).map(|s| s.id).collect();
        let dead_seats = seats.iter().filter(|s| !s.alive).map(|s| s.id).collect();
        Self {
            group_compares: classic::multi_compare::State::default(),
            group_updates: Vec::new(),
            ordering_owners: HashMap::new(),
            private_ordering: HashSet::new(),
            marks: marks::State::default(),
            indexed_triggers: BTreeMap::new(),
            force_die_triggers: HashSet::new(),
            force_out_triggers: HashSet::new(),
            turns: classic::turns::State::default(),
            phase_state: classic::phases::State::default(),
            active_skills: active_skill::State::default(),
            expansion_tags: HashMap::new(),
            teams: HashMap::new(),
            temporary_skills: Vec::new(),
            #[cfg(test)]
            timing_observer: None,
            elemental: elemental::State::default(),
            compares: classic::compare::State::default(),
            recast_rules: Vec::new(),
            loss_history: Vec::new(),
            reserved_draws: Vec::new(),
            transferring: Vec::new(),
            known_to_all: HashSet::new(),
            known_to_seats: HashMap::new(),
            seats,
            living_seats,
            dead_seats,
            active_seat,
            version: 0,
            resolving: Vec::new(),
            discard: Vec::new(),
            judgment_registry: classic::judge_registry::Registry::default(),
            decision: None,
            scheduler: Scheduler::new(),
            events: Vec::new(),
            classic: None,
            trigger_registrations: HashMap::new(),
            next_trigger_id: 0,
        }
    }

    /// Assign a mode-defined team. Classic identity games have no explicit teams.
    pub fn set_team(&mut self, seat: SeatId, team: u32) -> Result<(), RuleError> {
        self.seat(seat)?;
        self.teams.insert(seat, team);
        Ok(())
    }

    pub fn is_friend_of(&self, seat: SeatId, other: SeatId) -> bool {
        self.seat(seat).is_ok()
            && (seat == other
                || self
                    .teams
                    .get(&seat)
                    .is_some_and(|team| self.teams.get(&other) == Some(team)))
    }

    /// Held physical cards of a seat for trusted skill costs.
    pub fn hand_and_equipment(
        &self,
        seat: SeatId,
    ) -> Result<impl Iterator<Item = &Card>, RuleError> {
        let player = self.seat(seat)?;
        Ok(player.hand.iter().chain(&player.equipment))
    }

    pub fn is_alive(&self, seat: SeatId) -> bool {
        self.seat(seat).is_ok_and(|player| player.alive)
    }

    /// Living players in membership order; revival appends a previously dead player.
    pub fn living_seats(&self) -> &[SeatId] {
        &self.living_seats
    }

    /// Dead players in the order in which they died.
    pub fn dead_seats(&self) -> &[SeatId] {
        &self.dead_seats
    }

    /// Living players in physical seat order, starting from the given seat.
    pub fn living_seat_ring(&self, start: SeatId) -> Result<Vec<SeatId>, RuleError> {
        Ok(self
            .seat_order_from(start)?
            .into_iter()
            .filter(|&id| self.is_alive(id))
            .collect())
    }

    /// Kills attributed to a seat during the current turn.
    pub fn turn_kills(&self, seat: SeatId) -> Result<u64, RuleError> {
        Ok(self.seat(seat)?.turn_kills)
    }

    pub fn health(&self, seat: SeatId) -> Option<(i32, i32)> {
        self.seat(seat)
            .ok()
            .map(|player| (player.hp, player.max_hp))
    }

    /// The unowned special zone, including material that an interrupted recipient could not take.
    pub fn special_cards(&self) -> &[Card] {
        &self.transferring
    }

    /// Knowledge of a physical identity. An expansion's public mark may separately show its face.
    pub fn is_known_by(&self, card: CardId, viewer: Option<SeatId>) -> bool {
        self.is_known_hand_identity(card, viewer)
            || self.seats.iter().any(|seat| {
                seat.equipment
                    .iter()
                    .chain(&seat.judges)
                    .any(|held| held.id == card)
                    || viewer == Some(seat.id)
                        && seat
                            .hand
                            .iter()
                            .chain(&seat.expansions)
                            .any(|held| held.id == card)
            })
    }

    fn is_known_hand_identity(&self, card: CardId, viewer: Option<SeatId>) -> bool {
        self.known_to_all.contains(&card)
            || viewer.is_some_and(|viewer| {
                self.known_to_seats
                    .get(&card)
                    .is_some_and(|seats| seats.contains(&viewer))
            })
    }

    fn mark_known_to(&mut self, card: CardId, viewer: SeatId) {
        let seats = self.known_to_seats.entry(card).or_default();
        if !seats.contains(&viewer) {
            seats.push(viewer);
        }
    }

    /// Walk the table once, starting at `start` and following the setup order.
    pub fn seat_order_from(&self, start: SeatId) -> Result<Vec<SeatId>, RuleError> {
        Ok(self
            .seat_ring_from(self.seat_index(start)?)
            .map(|seat| seat.id)
            .collect())
    }

    fn seat_index(&self, id: SeatId) -> Result<usize, RuleError> {
        self.seats
            .iter()
            .position(|seat| seat.id == id)
            .ok_or(RuleError::UnknownSeat { seat: id })
    }

    fn seat_ring_from(&self, index: usize) -> impl Iterator<Item = &Seat> {
        let (before, from_start) = self.seats.split_at(index);
        from_start.iter().chain(before)
    }

    /// Count the shorter direction around the table, skipping seats omitted from distance.
    pub fn distance(&self, from: SeatId, to: SeatId) -> Result<usize, RuleError> {
        self.distance_without(from, to, None)
    }

    fn distance_without(
        &self,
        from: SeatId,
        to: SeatId,
        ignored: Option<CardId>,
    ) -> Result<usize, RuleError> {
        let from_index = self.seat_index(from)?;
        let rider = &self.seats[from_index];
        if !rider.is_in() {
            return Err(RuleError::InactiveSeat { seat: from });
        }
        if from == to {
            return Ok(0);
        }
        let target = self.seat(to)?;
        if !target.is_in() {
            return Err(RuleError::InactiveSeat { seat: to });
        }

        let mut clockwise = 1;
        let mut length = 0;
        for seat in self.seat_ring_from(from_index).skip(1) {
            if seat.id == to {
                // Reaching the target costs one step even if it has undist.
                clockwise = 1 + length;
            }
            if seat.is_in() && !self.seat_has_skill(seat, "undist") {
                length += 1;
            }
        }
        length += i32::from(!self.seat_has_skill(rider, "undist"));
        let raw = clockwise.min(length - clockwise);
        let adjustment = i32::from(equipment::has_slot(
            target,
            equipment::Slot::DefenseHorse,
            None,
        )) - i32::from(equipment::has_slot(
            rider,
            equipment::Slot::AttackHorse,
            ignored,
        )) + self.hero_distance_modifier(rider);
        Ok((raw + adjustment).max(1) as usize)
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
                    own || seat.role_shown || seat.role == Some(Role::Lord) || result.is_some();
                SeatView {
                    marks: self.skill_marks(seat.id),
                    expansions: seat
                        .expansions
                        .iter()
                        .map(|&card| ExpansionCard {
                            card,
                            tags: self.expansion_tags(card.id).to_vec(),
                        })
                        .collect(),
                    limited_spent: self.limited_spent(seat.id),
                    id: seat.id,
                    hp: seat.hp,
                    max_hp: seat.max_hp,
                    alive: seat.alive,
                    out: seat.out,
                    rest: seat
                        .rest_mark_shown
                        .then(|| seat.rest_clock.unwrap_or_default()),
                    linked: seat.linked,
                    turned_over: seat.turned_over,
                    armor: seat.armor,
                    hero: seat.hero.map(|hero| hero.id.clone()),
                    role: seat.role.filter(|_| role_visible),
                    hand_count: seat.hand.len(),
                    hand: own.then(|| seat.hand.clone()),
                    known_hand: seat
                        .hand
                        .iter()
                        .filter(|c| self.is_known_hand_identity(c.id, viewer))
                        .copied()
                        .collect(),
                    equipment: seat.equipment.clone(),
                    detached_equipment: seat
                        .detached_equipment
                        .iter()
                        .map(|card| card.kind)
                        .collect(),
                    judges: seat.judges.clone(),
                    equipment_disabled_markers: equipment::markers(seat),
                    judge_disabled: seat.judge_disabled,
                    judge_disabled_marker: seat.judge_disabled_marker,
                    judge_assignments: self.judgment_assignments(seat.id),
                    logical_judges: self.logical_judgment_view(seat.id),
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
        let private_choice_cards = choice
            .as_ref()
            .map(|choice| {
                if choice.move_cards.is_some() {
                    self.move_choice_cards(choice.event)
                } else {
                    self.hero_private_choice_cards(choice)
                }
            })
            .unwrap_or_default();
        let resolving: Vec<_> = self
            .resolving
            .iter()
            .filter(|c| {
                !self.compares.hidden(c.id)
                    && !self
                        .ordering_owners
                        .get(&c.id)
                        .is_some_and(|event| self.private_ordering.contains(event))
            })
            .copied()
            .collect();
        let known_special: Vec<_> = self
            .transferring
            .iter()
            .copied()
            .filter(|card| self.is_known_by(card.id, viewer))
            .collect();
        let events: Vec<_> = self
            .events
            .iter()
            .map(|&event| {
                if let EventKind::Equip { seat, card } = event.kind {
                    if !self.is_known_by(card.id, viewer)
                        && !resolving.contains(&card)
                        && !self.discard.contains(&card)
                    {
                        return GameEvent {
                            kind: EventKind::EquipPending { seat },
                            ..event
                        };
                    }
                }
                event
            })
            .collect();
        let mut visible = self.visible_card_ids(&seats, &private_choice_cards, &resolving, &events);
        for update in &self.group_updates {
            if let GroupAnnouncement::Reveal { bids, .. } = update {
                visible.extend(bids.iter().map(|bid| bid.card.id));
            }
        }
        visible.extend(known_special.iter().map(|card| card.id));
        let card_faces = state
            .iter()
            .flat_map(|state| &state.faces)
            .filter(|face| visible.contains(&face.id))
            .copied()
            .collect();
        let card_natures = self
            .elemental
            .natures
            .iter()
            .filter(|(id, _)| visible.contains(id))
            .map(|(&id, &nature)| (id, nature))
            .collect();
        Ok(GameView {
            round_suspended: self.round_suspended(),
            group_updates: self.group_updates.clone(),
            turn_reason: self
                .scheduler
                .active_programs()
                .find_map(|program| match program {
                    Program::Turn(flow) => Some(flow.reason()),
                    _ => None,
                })
                .flatten()
                .map(str::to_owned),
            phase_reason: self
                .scheduler
                .active_programs()
                .find_map(|program| match program {
                    Program::Phase(flow) => Some(flow.reason()),
                    _ => None,
                })
                .flatten()
                .map(str::to_owned),
            version: self.version,
            active_seat: self.active_seat,
            human_seat: state.map(|state| state.human),
            turn: state.and_then(|state| state.turn),
            winner: result.and_then(MatchResult::winner),
            drawn: result == Some(MatchResult::Draw),
            seats,
            deck_count: state.map_or(0, |state| state.deck.len()),
            resolving,
            known_special,
            discard: self.discard.clone(),
            waiting_for: self.decision.as_ref().map(Decision::seat),
            counter_window: matches!(&self.decision, Some(Decision::Cards(choice)) if choice.counter.is_some()),
            choice,
            actions,
            virtual_plays: match self.decision.as_ref() {
                Some(Decision::Play(seat)) if viewer == Some(*seat) => self.virtual_plays(*seat),
                _ => vec![],
            },
            skill_plays: match self.decision.as_ref() {
                Some(Decision::Play(seat)) if viewer == Some(*seat) => self.hero_skill_plays(*seat),
                _ => vec![],
            },
            active_plays: match self.decision.as_ref() {
                Some(Decision::Play(seat)) if viewer == Some(*seat) => self.active_plays(*seat),
                _ => Vec::new(),
            },
            private_choice_cards,
            card_faces,
            card_natures,
            events,
        })
    }

    fn visible_card_ids(
        &self,
        seats: &[SeatView],
        private_choice_cards: &[Card],
        resolving: &[Card],
        events: &[GameEvent],
    ) -> HashSet<CardId> {
        use EventKind::{
            Discard, Equip, Reveal, UseCost, UseResponseCard, UseSha, UseShaTargets, UseShan,
            UseTao, UseTrick,
        };

        let public_zones = resolving.iter().chain(&self.discard);
        let seat_zones = seats.iter().flat_map(|seat| {
            seat.hand
                .as_deref()
                .unwrap_or_default()
                .iter()
                .chain(&seat.known_hand)
                .chain(&seat.equipment)
                .chain(&seat.judges)
                .chain(seat.expansions.iter().map(|item| &item.card))
        });
        let revealed_events = events.iter().filter_map(|event| match event.kind {
            UseSha { card, .. }
            | UseShaTargets { card, .. }
            | UseShan { card, .. }
            | UseTao { card, .. }
            | UseTrick { card, .. }
            | UseCost { card, .. }
            | UseResponseCard { card, .. }
            | EventKind::RecastReveal { card, .. }
            | EventKind::ShowCards { card, .. }
            | EventKind::Gain {
                card: Some(card), ..
            }
            | Equip { card, .. }
            | Reveal { card, .. }
            | Discard { card } => Some(card.id),
            _ => None,
        });
        let compare_faces = events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::CompareReveal { cards, .. } => Some(cards),
                EventKind::CompareResult { result } => Some(result.cards),
                _ => None,
            })
            .flatten()
            .map(|c| c.id);
        public_zones
            .chain(seat_zones)
            .chain(private_choice_cards)
            .map(|card| card.id)
            .chain(revealed_events)
            .chain(compare_faces)
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
            Action::AdvanceRound => {
                if !self.round_suspended() {
                    return Err(RuleError::WrongPhase);
                }
                let host = self
                    .classic
                    .as_ref()
                    .expect("only a phase loop can yield")
                    .human;
                if command.seat != host {
                    return Err(RuleError::WrongActor { expected: host });
                }
                self.resume_suspended_round();
                None
            }
            Action::ActivateSkill {
                registration,
                cards,
                targets,
            } => Some(Answer::Play(self.plan_active_skill(
                command.seat,
                registration,
                cards,
                targets,
            )?)),
            Action::Recast { card } => Some(Answer::Play(self.plan_recast(command.seat, card)?)),
            Action::PlaySha { card, target } if self.classic.is_none() => {
                let use_sha = self.plan_sha(command.seat, card, target)?;
                self.scheduler.start(use_sha);
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
            Action::MoveCards { event, moves } => {
                Some(self.validate_move_choice(command.seat, event, moves)?)
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
            Action::UseSkill {
                skill,
                cards,
                targets,
            } => Some(Answer::Play(self.plan_hero_skill(
                command.seat,
                skill,
                cards,
                targets,
            )?)),
        };
        self.decision = None;
        self.events.clear();
        self.group_updates.clear();
        self.run(answer);
        self.version = next_version;
        Ok(())
    }

    fn plan_sha(&self, seat: SeatId, card: CardId, target: SeatId) -> Result<Program, RuleError> {
        if self.decision.is_some() {
            return Err(RuleError::ChoicePending);
        }
        if self.active_seat != seat {
            return Err(RuleError::WrongActor {
                expected: self.active_seat,
            });
        }
        if target == seat || !self.seat(target)?.is_in() {
            return Err(RuleError::InvalidTarget);
        }
        let card = self.card_in_hand(seat, card, CardKind::Sha)?;
        if !self.card_enabled(seat, CardKind::Sha) {
            return Err(RuleError::InvalidSelection);
        }
        Ok(self.attack_program(seat, vec![card], vec![target]))
    }

    fn checked_play_seat(&self, seat: SeatId) -> Result<(), RuleError> {
        match self.decision.as_ref() {
            Some(Decision::Play(actor)) if *actor == seat && !self.is_in(seat) => {
                Err(RuleError::InactiveSeat { seat })
            }
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
                    self.record(event, &program, EventStatus::Started, false)
                }
                Work::Run(event, program, child) => {
                    self.resolve(event, program, child, &mut answer)
                }
                Work::Ended(event, program, outcome) => {
                    let body_skipped =
                        matches!(outcome, Outcome::Finished(Resolution::BodySkipped));
                    let status = match outcome {
                        Outcome::Cancelled
                        | Outcome::Finished(Resolution::ComparedMany {
                            cancelled: true, ..
                        })
                        | Outcome::Finished(Resolution::HpChanged { cancelled: true })
                        | Outcome::Finished(Resolution::MaxHpChanged { cancelled: true })
                        | Outcome::Finished(Resolution::DeathCancelled)
                        | Outcome::Finished(Resolution::PileOrdered(PileOrderResult {
                            cancelled: true,
                            ..
                        }))
                        | Outcome::Finished(Resolution::ReviveCancelled)
                        | Outcome::Finished(Resolution::EquipReplacement {
                            cancelled: true, ..
                        })
                        | Outcome::Finished(Resolution::RestCancelled)
                        | Outcome::Finished(Resolution::MovedCards(MoveResult {
                            cancelled: true,
                            ..
                        }))
                        | Outcome::Finished(Resolution::BatchMove {
                            cancelled: true, ..
                        })
                        | Outcome::Finished(Resolution::RecastMove {
                            cancelled: true, ..
                        }) => EventStatus::Cancelled,
                        Outcome::Finished(_) => EventStatus::Finished,
                    };
                    self.record(event, &program, status, body_skipped);
                }
            }
            if self.decision.is_some() || self.round_suspended() {
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
        use Resolution::{Dodged, Passed};

        let kind = program.kind();
        let program = match program {
            Program::GroupCompare(flow) => return self.resolve_group_compare(event, flow, child),
            Program::Bids(flow) => return self.resolve_bids(event, flow, child, answer),
            Program::Counter(flow) => return self.resolve_counter_window(flow, child),
            Program::Death(flow) => return self.resolve_death(flow, child),
            Program::MaxHp(flow) => return self.resolve_max_hp(event, flow, child),
            Program::Revive(flow) => return self.resolve_revive(flow, child),
            Program::Rest(flow) => return self.resolve_rest(flow, child),
            Program::ActiveSkill(flow) => return self.resolve_active_skill(flow, child),
            Program::PileOrder(flow) => return self.resolve_pile_order(event, flow, child),
            Program::MoveChoice(flow) => {
                return self.resolve_move_choice(event, flow, child, answer)
            }
            Program::Expansion(flow) => return self.resolve_expansion(event, flow, child),
            Program::Compare(flow) => return self.resolve_compare(event, flow, child, answer),
            Program::Turn(flow) => return self.resolve_turn_flow(flow, child),
            Program::PhaseLoop(flow) => return self.resolve_phase_loop(flow, child),
            Program::Phase(flow) => return self.resolve_phase_flow(event, flow, child, answer),
            Program::Elemental(flow) => return self.resolve_elemental(flow, child),
            Program::Recast(flow) => return self.resolve_recast(event, flow, child),
            Program::Equipment(flow) => return self.resolve_equipment(event, flow, child),
            Program::Trick(flow) => return self.resolve_trick(event, flow, child),
            Program::TargetTiming(flow) => return self.resolve_card_target_timing(flow, child),
            Program::Hero(flow) => return self.resolve_hero(event, flow, child),
            Program::Dispatch(flow) => return self.resolve_dispatch(flow, child),
            Program::Registered(flow) => return self.resolve_registered(flow, child),
            Program::Choose(choice) => return self.resolve_selection(event, choice, None, answer),
            Program::CounterChoice { request, context } => {
                return self.resolve_selection(event, request, Some(context), answer)
            }
            Program::JudgeArea(flow) => return self.resolve_judge_area(flow, child),
            Program::EquipArea(flow) => return self.resolve_equip_area(flow, child),
            Program::AddJudge(flow) => return self.resolve_add_judge(flow, child),
            Program::DelayedPlacement(placement) => {
                return self.resolve_delayed_placement(placement)
            }
            Program::Judgment(judgment) => return self.resolve_judgment(judgment, child),
            Program::JudgmentCleanup(cleanup) => {
                return self.resolve_judgment_cleanup(cleanup, child)
            }
            Program::CardDamage(damage) => return self.resolve_card_damage(event, damage, child),
            Program::DiscardGroup { seat, cards } => {
                return self.resolve_discard_group(seat, cards)
            }
            Program::UseCosts { seat, cards, paid } => {
                return self.resolve_use_costs(seat, cards, paid)
            }
            Program::AnnounceUseCost { .. } => return self.finish(),
            Program::Discarded(_) => return self.finish(),
            Program::BasicUse(flow) => return self.resolve_basic_use(flow, child),
            program => program,
        };
        match (kind, child) {
            (ChooseShan { seat }, None) => match answer.take() {
                Some(Answer::Pass) => self.scheduler.end(Finished(Passed)),
                Some(Answer::Card(card)) => {
                    let uses_card = event
                        .parent
                        .and_then(|id| self.scheduler.active_frame(id))
                        .and_then(|(_, program)| match program {
                            Program::Equipment(flow) => flow.response_uses_card(),
                            _ => None,
                        })
                        .unwrap_or(true);
                    self.scheduler
                        .next(Program::BasicUse(classic::basic_use::Flow::shan(
                            seat, card, uses_card,
                        )));
                }
                None => self.ask_for_card(event, seat, ChoiceKind::Shan, None, CardKind::Shan),
                _ => unreachable!("a shan choice receives one card or a pass"),
            },
            (ChooseShan { .. }, Some(Finished(Dodged))) => self.scheduler.end(Finished(Dodged)),
            (ChooseShan { .. }, Some(_)) => self.scheduler.end(Finished(Passed)),
            (Recover { .. }, None) if self.match_result().is_some() => {
                self.scheduler.end(Cancelled)
            }
            (Recover { target, amount, .. }, None) => {
                self.queue_health_change(target, i32::from(amount), None)
            }
            (Recover { .. }, Some(_)) => self.finish(),
            (
                Damage {
                    source,
                    target,
                    amount,
                },
                None,
            ) => {
                let hit = self.elemental_damage_program(
                    source,
                    target,
                    amount,
                    Default::default(),
                    false,
                );
                self.scheduler.replace_current(hit);
            }
            (
                Reveal { .. } | RecastReveal { .. } | CompareReveal { .. } | CompareResult { .. },
                None,
            ) => self.finish(),
            (UseCost { seat, card }, None) => {
                self.move_to_resolving(seat, card.id);
                self.finish();
            }
            (Discard { card }, None) => {
                match program {
                    Program::DiscardFrom { seat, zone, .. } => {
                        let card = self.take_card(seat, zone, card.id);
                        self.discard.push(card);
                        self.queue_hero_losses(seat, vec![(zone, card)]);
                    }
                    _ if self.resolving.contains(&card) => {
                        let card = self.take_resolving(card.id);
                        self.discard.push(card);
                    }
                    _ => {
                        self.scheduler.end(Cancelled);
                        return;
                    }
                }
                self.finish();
            }
            (kind, child) => self.resolve_classic(event, kind, child, answer),
        }
    }

    fn resolve_selection(
        &mut self,
        event: EventRef,
        choice: ChoiceRequest,
        counter: Option<CounterContext>,
        answer: &mut Option<Answer>,
    ) {
        match answer.take() {
            Some(Answer::Select(selected)) => self
                .scheduler
                .end(Outcome::Finished(Resolution::Selected(selected))),
            None => {
                self.decision = Some(Decision::Cards(ChoiceView {
                    move_cards: None,
                    event: event.id,
                    seat: choice.seat,
                    kind: ChoiceKind::Content,
                    target: choice.target,
                    min: 0,
                    max: 0,
                    cards: vec![],
                    branches: choice.branches,
                    counter,
                    compare: None,
                    compare_group: None,
                }));
            }
            _ => unreachable!("a content choice receives a selection"),
        }
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
            move_cards: None,
            event: event.id,
            seat,
            kind,
            target,
            min: 0,
            max: 1,
            cards,
            branches: vec![],
            counter: None,
            compare: None,
            compare_group: None,
        }));
    }

    fn next(&mut self, event: EventKind) {
        self.scheduler.next(Program::event(event));
    }

    fn finish(&mut self) {
        self.scheduler.end(Outcome::Finished(Resolution::Complete));
    }

    fn record(
        &mut self,
        event: EventRef,
        program: &Program,
        status: EventStatus,
        body_skipped: bool,
    ) {
        self.events.push(GameEvent {
            id: event.id,
            parent: event.parent,
            status,
            body_skipped,
            kind: match (status, program) {
                (EventStatus::Started, Program::Elemental(elemental::Flow::Hp(hp))) => {
                    hp.announced(self)
                }
                (_, Program::Death(flow))
                    if flow.reserved
                        || (!self.seat(flow.target).is_ok_and(|seat| seat.role_shown)
                            && self.match_result().is_none()) =>
                {
                    EventKind::Death {
                        source: flow.source,
                        target: flow.target,
                        role: None,
                    }
                }
                _ => program.kind(),
            },
            nature: match program {
                Program::CardDamage(hit) => hit.card.nature,
                _ => None,
            },
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
        self.in_order_from(start)
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
        if matches!(zone, Zone::Hand) {
            self.known_to_all.remove(&id);
            self.known_to_seats.remove(&id);
        }
        if matches!(zone, Zone::Judgment) {
            self.judgment_registry.material_left(seat, id);
        }
        if matches!(zone, Zone::Expansion) {
            self.expansion_tags.remove(&id);
        }
        card
    }

    fn take_resolving(&mut self, id: CardId) -> Card {
        self.ordering_owners.remove(&id);
        self.compares.forget(id);
        Self::remove_card(&mut self.resolving, id)
    }

    fn put_in_hand(&mut self, seat: SeatId, card: Card) {
        self.seat_mut(seat).hand.push(card);
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
