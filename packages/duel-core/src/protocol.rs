use serde::{Deserialize, Serialize};

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
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub version: u32,
    pub seat: SeatId,
    pub action: Action,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    PlaySha {
        card: CardId,
        target: SeatId,
    },
    Choose {
        event: EventId,
        card: Option<CardId>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameView {
    pub version: u32,
    pub active_seat: SeatId,
    pub seats: Vec<SeatView>,
    pub resolving: Vec<Card>,
    pub discard: Vec<Card>,
    pub waiting_for: Option<SeatId>,
    pub choice: Option<ChoiceView>,
    pub events: Vec<GameEvent>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatView {
    pub id: SeatId,
    pub hp: u16,
    pub hand_count: usize,
    pub hand: Option<Vec<Card>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceView {
    pub event: EventId,
    pub seat: SeatId,
    pub cards: Vec<CardId>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameEvent {
    pub id: EventId,
    pub parent: Option<EventId>,
    pub status: EventStatus,
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
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventKind {
    UseSha {
        seat: SeatId,
        target: SeatId,
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
    Damage {
        source: SeatId,
        target: SeatId,
        amount: u16,
    },
    Discard {
        card: Card,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum RuleError {
    NotEnoughSeats,
    DuplicateSeat { seat: SeatId },
    UnknownSeat { seat: SeatId },
    DuplicateCard { card: CardId },
    InactiveSeat { seat: SeatId },
    WrongVersion { expected: u32, received: u32 },
    VersionExhausted,
    ChoicePending,
    NoChoice,
    WrongActor { expected: SeatId },
    WrongEvent,
    InvalidCard,
    InvalidTarget,
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotEnoughSeats => formatter.write_str("the table needs at least two seats"),
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
            Self::ChoicePending => formatter.write_str("an event is waiting for a choice"),
            Self::NoChoice => formatter.write_str("no event is waiting for a choice"),
            Self::WrongActor { expected } => write!(formatter, "seat {expected} must act"),
            Self::WrongEvent => formatter.write_str("the choice references a different event"),
            Self::InvalidCard => formatter.write_str("the card is unavailable for this action"),
            Self::InvalidTarget => formatter.write_str("the target is unavailable for this action"),
        }
    }
}

impl std::error::Error for RuleError {}
