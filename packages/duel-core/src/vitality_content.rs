//! Weizhong draws when its owner's maximum HP changes.
use crate::{SeatId, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep};
use std::sync::Arc;

pub fn weizhong(owner: SeatId) -> [TriggerRegistration; 2] {
    ["gainMaxHpEnd", "loseMaxHpEnd"].map(|timing| TriggerRegistration {
        owner,
        skill: "weizhong".into(),
        timing: timing.into(),
        role: TriggerRole::Player,
        order: TriggerOrder::Normal,
        priority: 0,
        label: Some("威重".into()),
        silent: false,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|ctx, _, _| {
            let owner = ctx.owner();
            let game = ctx.game();
            let count = game.seat(owner).map(|seat| seat.hand.len()).unwrap_or(0);
            let fewest = game
                .seats
                .iter()
                .filter(|seat| game.is_in(seat.id))
                .all(|seat| count <= seat.hand.len());
            let _ = ctx.draw_cards(owner, if fewest { 2 } else { 1 });
            TriggerStep::Done
        }),
    })
}
