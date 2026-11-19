use serde::{Deserialize, Serialize};

use crate::classic_data::{Role, Side, Suit};
use crate::SkillId;

pub type SeatId = u32;
pub type CardId = u32;
pub type EventId = u64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSetup {
    pub seats: Vec<SeatSetup>,
    pub active_seat: SeatId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatSetup {
    pub id: SeatId,
    pub hp: u16,
    pub hand: Vec<Card>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: CardId,
    pub kind: CardKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CardKind {
    Sha,
    Shan,
    Tao,
    Bagua,
    Jueying,
    Dilu,
    Zhuahuang,
    Chitu,
    Dawan,
    Zixin,
    Zhuge,
    Cixiong,
    Qinggang,
    Qinglong,
    Zhangba,
    Guanshi,
    Fangtian,
    Qilin,
    Wugu,
    Taoyuan,
    Nanman,
    Wanjian,
    Juedou,
    Wuzhong,
    Shunshou,
    Guohe,
    Jiedao,
    Tiesuo,
    Wuxie,
    Lebu,
    Shandian,
    #[serde(untagged)]
    Content(crate::content::CardKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardType {
    Basic,
    Trick,
    Delay,
    Equipment,
}

impl CardKind {
    pub fn card_type(self) -> CardType {
        use CardKind::*;
        match self {
            Sha | Shan | Tao => CardType::Basic,
            Wugu | Taoyuan | Nanman | Wanjian | Juedou | Wuzhong | Shunshou | Guohe | Jiedao
            | Tiesuo | Wuxie => CardType::Trick,
            Content(_) => CardType::Trick,
            Lebu | Shandian => CardType::Delay,
            Bagua | Jueying | Dilu | Zhuahuang | Chitu | Dawan | Zixin | Zhuge | Cixiong
            | Qinggang | Qinglong | Zhangba | Guanshi | Fangtian | Qilin => CardType::Equipment,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub version: u32,
    pub seat: SeatId,
    pub action: Action,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    /// Continue the rules from a yielded round with no participating seats.
    AdvanceRound,
    ActivateSkill {
        registration: u64,
        cards: Vec<CardId>,
        targets: Vec<SeatId>,
    },
    Recast {
        card: CardId,
    },
    PlaySha {
        card: CardId,
        target: SeatId,
    },
    Choose {
        event: EventId,
        card: Option<CardId>,
    },
    PlayCard {
        card: CardId,
        targets: Vec<SeatId>,
    },
    EndPlay,
    Discard {
        event: EventId,
        cards: Vec<CardId>,
    },
    Select {
        event: EventId,
        branch: u32,
        cards: Vec<CardId>,
        targets: Vec<SeatId>,
    },
    SelectEquipmentButtons {
        event: EventId,
        buttons: Vec<u32>,
    },
    MoveCards {
        event: EventId,
        moves: Option<Vec<crate::MoveStep>>,
    },
    PlayVirtual {
        kind: CardKind,
        cards: Vec<CardId>,
        targets: Vec<SeatId>,
    },
    UseSkill {
        skill: SkillId,
        cards: Vec<CardId>,
        targets: Vec<SeatId>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<crate::content::ContentPreview>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mount_combined: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub round_suspended: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_updates: Vec<crate::GroupAnnouncement>,
    pub version: u32,
    pub active_seat: SeatId,
    pub human_seat: Option<SeatId>,
    pub turn: Option<TurnView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_reason: Option<String>,
    pub winner: Option<Side>,
    pub drawn: bool,
    pub seats: Vec<SeatView>,
    pub deck_count: usize,
    pub resolving: Vec<Card>,
    /// Active virtual card uses and the public resolving materials they represent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolving_virtuals: Vec<ResolvingVirtualCard>,
    /// Identifiable physical cards in the unowned special zone, including transfers in flight.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_special: Vec<Card>,
    pub discard: Vec<Card>,
    pub waiting_for: Option<SeatId>,
    pub counter_window: bool,
    pub choice: Option<ChoiceView>,
    pub actions: Vec<Action>,
    pub virtual_plays: Vec<VirtualPlay>,
    pub skill_plays: Vec<SkillPlay>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_plays: Vec<ActivePlay>,
    pub private_choice_cards: Vec<Card>,
    pub card_faces: Vec<CardFace>,
    /// Elemental natures of physical Sha currently visible to this viewer.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub card_natures: std::collections::BTreeMap<CardId, crate::DamageNature>,
    /// Lifecycle records emitted since the latest accepted command, or during setup.
    pub events: Vec<GameEvent>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvingVirtualCard {
    pub event: EventId,
    pub kind: CardKind,
    pub materials: Vec<CardId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nature: Option<crate::DamageNature>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EquipmentSlot {
    Weapon,
    Armor,
    DefenseHorse,
    AttackHorse,
    Treasure,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MountSlots {
    #[default]
    Separate,
    Combined,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentSlotState {
    pub enabled: i64,
    pub empty: usize,
    pub equipable: usize,
    pub disabled: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatView {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub out: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest: Option<crate::RestClock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<crate::SkillMark>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expansions: Vec<ExpansionCard>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limited_spent: Vec<String>,
    pub id: SeatId,
    pub hp: i32,
    pub max_hp: i32,
    pub alive: bool,
    #[serde(default)]
    pub linked: bool,
    #[serde(default)]
    pub turned_over: bool,
    #[serde(default)]
    pub armor: u16,
    pub hero: Option<String>,
    pub role: Option<Role>,
    pub hand_count: usize,
    pub hand: Option<Vec<Card>>,
    /// Cards publicly known to remain in this hand.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_hand: Vec<Card>,
    pub equipment: Vec<Card>,
    /// Effective equipment whose material is outside the physical equipment area.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detached_equipment: Vec<CardKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equipment_disabled_markers: Vec<EquipmentSlot>,
    pub judges: Vec<Card>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub judge_disabled: bool,
    /// The disable operation publishes its marker after the discard child has settled.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub judge_disabled_marker: bool,
    pub judge_assignments: Vec<JudgmentAssignment>,
    /// Ordered public entries when physical cards alone do not describe the judgment area.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logical_judges: Vec<JudgmentEntryView>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgmentEntryView {
    pub kind: CardKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<CardId>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgmentAssignment {
    pub id: CardId,
    pub kind: CardKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardFace {
    pub id: CardId,
    pub suit: Suit,
    pub rank: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnView {
    pub seat: SeatId,
    pub number: u64,
    pub round: u64,
    pub phase: Phase,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Preparation,
    Judgment,
    Draw,
    Play,
    Discard,
    End,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipment_buttons: Option<EquipmentButtons>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_cards: Option<crate::MoveChoiceView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare_group: Option<CompareGroup>,
    pub event: EventId,
    pub seat: SeatId,
    pub kind: ChoiceKind,
    pub target: Option<SeatId>,
    pub min: usize,
    pub max: usize,
    pub cards: Vec<CardId>,
    pub branches: Vec<ChoiceBranch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter: Option<CounterContext>,
    /// Present only for the acting seat's choice of a face-down comparison card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare: Option<CompareParticipants>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentButtons {
    pub required: usize,
    pub options: Vec<EquipmentButton>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentButton {
    pub id: u32,
    pub kind: CardKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<CardId>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareParticipants {
    pub initiator: SeatId,
    pub target: SeatId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareGroup {
    pub initiator: SeatId,
    pub targets: Vec<SeatId>,
}

/// The original physical cards and effective numbers in initiator/target order.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareResult {
    pub event: EventId,
    pub parent: Option<EventId>,
    pub turn: Option<u64>,
    pub participants: CompareParticipants,
    pub cards: [Card; 2],
    pub numbers: [i32; 2],
    pub won: bool,
    pub tie: bool,
    pub winner: Option<SeatId>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterContext {
    pub card_kind: CardKind,
    pub source: Option<SeatId>,
    pub target: SeatId,
    pub negated: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChoiceKind {
    Shan,
    Save,
    Discard,
    Content,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceBranch {
    pub id: u32,
    pub label: String,
    pub cards: Vec<CardId>,
    pub min_cards: usize,
    pub max_cards: usize,
    pub targets: Vec<SeatId>,
    pub min_targets: usize,
    pub max_targets: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualPlay {
    pub kind: CardKind,
    pub branch: ChoiceBranch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPlay {
    pub skill: SkillId,
    pub branch: ChoiceBranch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivePlay {
    pub registration: u64,
    pub skill: String,
    pub branch: ChoiceBranch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExpansionCard {
    pub card: Card,
    pub tags: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameEvent {
    /// The event reached its end timings without applying its content to an out player.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub body_skipped: bool,
    pub id: EventId,
    pub parent: Option<EventId>,
    pub status: EventStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nature: Option<crate::DamageNature>,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EventStatus {
    Started,
    Finished,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CardTargetTiming {
    UseCardToPlayer,
    UseCardToTarget,
    UseCardToPlayered,
    UseCardToTargeted,
    UseCardToExcluded,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventKind {
    CompareMany {
        seat: SeatId,
        count: usize,
        mode: crate::CompareMode,
    },
    ComparePairCallback {
        seat: SeatId,
        target: SeatId,
        comparison: EventId,
    },
    GroupChoices {
        seat: SeatId,
        count: usize,
    },
    GroupBid {
        seat: SeatId,
        initiator: SeatId,
    },
    ShowBid {
        seat: SeatId,
    },
    CounterWindow {
        seat: SeatId,
        kind: CardKind,
    },
    ChooseToUse {
        seat: SeatId,
        kind: CardKind,
    },
    PhaseLoop,
    ActivateSkill {
        seat: SeatId,
        registration: u64,
    },
    ActiveSkillEffect {
        seat: SeatId,
        registration: u64,
    },
    AddToExpansion {
        seat: SeatId,
        amount: usize,
    },
    #[serde(rename_all = "camelCase")]
    CardTargetTiming {
        timing: CardTargetTiming,
        seat: SeatId,
        target: SeatId,
        card_kind: CardKind,
    },
    Compare {
        seat: SeatId,
        target: SeatId,
    },
    CompareChoices {
        seat: SeatId,
        target: SeatId,
    },
    CompareBid {
        seat: SeatId,
    },
    LoseAsync,
    CompareReveal {
        seat: SeatId,
        target: SeatId,
        cards: [Card; 2],
    },
    ShowCards {
        seat: SeatId,
        card: Card,
    },
    CompareResult {
        result: CompareResult,
    },
    OrderingDiscard,
    CardsDiscard {
        amount: usize,
    },
    CardsGotoOrdering {
        seat: SeatId,
        amount: usize,
    },
    CardsGotoPile {
        seat: SeatId,
        amount: usize,
    },
    DisableEquip {
        seat: SeatId,
        source: Option<SeatId>,
    },
    ChooseButton {
        seat: SeatId,
    },
    EnableEquip {
        seat: SeatId,
        source: Option<SeatId>,
    },
    ExpandEquip {
        seat: SeatId,
        source: Option<SeatId>,
    },
    DisableJudge {
        seat: SeatId,
        source: Option<SeatId>,
    },
    EnableJudge {
        seat: SeatId,
        source: Option<SeatId>,
    },
    AddJudge {
        seat: SeatId,
    },
    Link {
        seat: SeatId,
        linked: bool,
    },
    #[serde(rename_all = "camelCase")]
    TurnOver {
        seat: SeatId,
        turned_over: bool,
    },
    ChangeArmor {
        seat: SeatId,
        delta: i32,
    },
    TrickEffect {
        seat: SeatId,
        target: SeatId,
        kind: CardKind,
    },
    UnlinkAfterDamage {
        seat: SeatId,
    },
    SpreadDamage {
        seat: SeatId,
    },
    Recast {
        seat: SeatId,
    },
    RecastReveal {
        seat: SeatId,
        card: Card,
    },
    LoseToDiscardpile {
        seat: SeatId,
        amount: usize,
    },
    Lose {
        seat: SeatId,
        amount: usize,
    },
    Gain {
        seat: SeatId,
        source: Option<SeatId>,
        amount: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        card: Option<Card>,
    },
    Opening {
        lord: SeatId,
    },
    Turn {
        seat: SeatId,
        number: u64,
        round: u64,
    },
    Phase {
        seat: SeatId,
        phase: Phase,
    },
    Draw {
        seat: SeatId,
        amount: u16,
        reason: DrawReason,
    },
    UseSha {
        seat: SeatId,
        target: SeatId,
        card: Card,
    },
    UseShaTargets {
        seat: SeatId,
        card: Card,
    },
    UseVirtual {
        seat: SeatId,
        kind: CardKind,
    },
    UseCost {
        seat: SeatId,
        card: Card,
    },
    UseResponseCard {
        seat: SeatId,
        card: Card,
    },
    ShaEffect {
        seat: SeatId,
        target: SeatId,
    },
    ChooseShan {
        seat: SeatId,
    },
    UseShan {
        seat: SeatId,
        card: Card,
    },
    UseTao {
        seat: SeatId,
        target: SeatId,
        card: Card,
    },
    UseTrick {
        seat: SeatId,
        card: Card,
        target: Option<SeatId>,
    },
    Equip {
        seat: SeatId,
        card: Card,
    },
    EquipPending {
        seat: SeatId,
    },
    #[serde(rename_all = "camelCase")]
    ReplaceEquip {
        seat: SeatId,
        card_kind: CardKind,
    },
    EquipmentEffect {
        seat: SeatId,
        target: SeatId,
        kind: CardKind,
    },
    Respond {
        seat: SeatId,
        kind: CardKind,
    },
    ChooseContent {
        seat: SeatId,
        target: Option<SeatId>,
    },
    ChooseToMove {
        seat: SeatId,
    },
    ChoosePileOrder {
        seat: SeatId,
        amount: u16,
    },
    Judge {
        seat: SeatId,
    },
    Reveal {
        seat: SeatId,
        card: Card,
    },
    Damage {
        source: Option<SeatId>,
        target: SeatId,
        amount: u16,
    },
    ChangeHp {
        seat: SeatId,
        before: i32,
        after: i32,
    },
    LoseHp {
        seat: SeatId,
        amount: u16,
    },
    Skill {
        seat: SeatId,
        skill: SkillId,
    },
    /// A registered skill is being evaluated and may offer a choice.
    SkillTrigger {
        seat: SeatId,
        skill: SkillId,
    },
    Trigger {
        seat: SeatId,
    },
    RegisteredTrigger {
        seat: SeatId,
        registration: u64,
    },
    DiscardGroup {
        seat: SeatId,
        amount: usize,
    },
    Recover {
        seat: SeatId,
        target: SeatId,
        amount: u16,
    },
    Dying {
        source: Option<SeatId>,
        target: SeatId,
    },
    ChooseSave {
        seat: SeatId,
        target: SeatId,
    },
    Rest {
        seat: SeatId,
        clock: crate::RestClock,
    },
    RestEnd {
        seat: SeatId,
        hp: i32,
    },
    Revive {
        seat: SeatId,
        hp: i32,
    },
    Death {
        source: Option<SeatId>,
        target: SeatId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<Role>,
    },
    GainMaxHp {
        seat: SeatId,
        source: Option<SeatId>,
        num: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<i32>,
    },
    LoseMaxHp {
        seat: SeatId,
        source: Option<SeatId>,
        num: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<i32>,
    },
    Penalty {
        lord: SeatId,
    },
    Discard {
        card: Card,
    },
    GameOver {
        winner: Option<Side>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DrawReason {
    Opening,
    Phase,
    RebelReward,
    CardEffect,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum RuleError {
    NotEnoughSeats,
    UnsupportedSeatCount { count: usize },
    UnknownHero,
    InvalidContent { reason: String },
    DuplicateSeat { seat: SeatId },
    UnknownSeat { seat: SeatId },
    DuplicateCard { card: CardId },
    InactiveSeat { seat: SeatId },
    WrongVersion { expected: u32, received: u32 },
    VersionExhausted,
    GameOver,
    ChoicePending,
    NoChoice,
    WrongActor { expected: SeatId },
    WrongEvent,
    InvalidCard,
    InvalidTarget,
    InvalidSelection,
    WrongPhase,
    UnsupportedCard,
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotEnoughSeats => formatter.write_str("the table needs at least two seats"),
            Self::UnsupportedSeatCount { count } => {
                write!(formatter, "unsupported table size {count}")
            }
            Self::UnknownHero => formatter.write_str("the selected hero is absent from the roster"),
            Self::InvalidContent { reason } => write!(formatter, "内容包无效：{reason}"),
            Self::DuplicateSeat { seat } => write!(formatter, "seat {seat} appears twice"),
            Self::UnknownSeat { seat } => write!(formatter, "seat {seat} is absent"),
            Self::DuplicateCard { card } => write!(formatter, "card {card} appears twice"),
            Self::InactiveSeat { seat } => write!(formatter, "seat {seat} has no remaining health"),
            Self::WrongVersion { expected, received } => {
                write!(
                    formatter,
                    "expected version {expected}, received {received}"
                )
            }
            Self::VersionExhausted => formatter.write_str("the command version is exhausted"),
            Self::GameOver => formatter.write_str("the match has finished"),
            Self::ChoicePending => formatter.write_str("an event is waiting for a choice"),
            Self::NoChoice => formatter.write_str("no event is waiting for a choice"),
            Self::WrongActor { expected } => write!(formatter, "seat {expected} must act"),
            Self::WrongEvent => formatter.write_str("the choice references a different event"),
            Self::InvalidCard => formatter.write_str("the card is unavailable for this action"),
            Self::InvalidTarget => formatter.write_str("the target is unavailable for this action"),
            Self::InvalidSelection => {
                formatter.write_str("the selected cards do not satisfy the choice")
            }
            Self::WrongPhase => formatter.write_str("the table is not waiting for a play action"),
            Self::UnsupportedCard => formatter.write_str("this card's effects are not available"),
        }
    }
}

impl std::error::Error for RuleError {}
