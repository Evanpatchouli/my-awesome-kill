use crate::{Card, CompareGroup, EventId, SeatId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CompareMode {
    Pairwise,
    Simultaneous,
}

/// The initialized result of a sealed group bid. Cards are in initiator/target order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareManyResult {
    /// The participants could not bid. A later cancellation of an initialized root is recorded on its frame.
    pub selection_cancelled: bool,
    pub cards: Vec<Card>,
    pub numbers: Vec<[i32; 2]>,
    /// Simultaneous comparison records its unique highest bidder during settlement.
    pub winner: Option<SeatId>,
}

/// Trusted state of a group comparison, including its working scores and committed result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareManyFrame {
    pub event: EventId,
    pub parent: Option<EventId>,
    pub turn: Option<u64>,
    pub mode: CompareMode,
    pub group: CompareGroup,
    pub index: usize,
    pub player: Option<SeatId>,
    pub target: Option<SeatId>,
    pub cards: [Option<Card>; 2],
    pub numbers: [Option<i32>; 2],
    pub winner: Option<SeatId>,
    pub result: Option<CompareManyResult>,
    pub cancelled: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RevealedBid {
    pub seat: SeatId,
    pub card: Card,
}

/// Only announcements already presented to the table are included in a seat view.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum GroupAnnouncement {
    Reveal {
        comparison: EventId,
        mode: CompareMode,
        bids: Vec<RevealedBid>,
    },
    Pair {
        comparison: EventId,
        initiator: SeatId,
        target: SeatId,
        numbers: [i32; 2],
        winner: Option<SeatId>,
    },
    Result {
        comparison: EventId,
        mode: CompareMode,
        group: CompareGroup,
        numbers: Vec<[i32; 2]>,
        winner: Option<SeatId>,
    },
}
