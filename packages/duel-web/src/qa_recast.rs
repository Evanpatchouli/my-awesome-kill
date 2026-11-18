use super::Duel;
use duel_core::{CardKind, RecastGain, RecastKey, RecastRule, SeatId};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// Exercise a dual-purpose Sha and a recast-only Shan in the browser smoke test.
#[wasm_bindgen(js_name = classicRecastFixture)]
pub fn classic_recast_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    let mut duel = Duel::classic(seed, seat, hero, count)?;
    duel.game
        .register_recast(RecastKey::Kind(CardKind::Shan), RecastRule::default());
    // Compare an ordinary private draw with a face-up gain of the top card.
    let public_gain = RecastRule {
        gain: Arc::new(|state, _, _, _| {
            state
                .draw_pile()
                .last()
                .map_or(RecastGain::Draw, |card| RecastGain::FromDeck {
                    card: card.id,
                })
        }),
        ..RecastRule::default()
    };
    duel.game
        .register_recast(RecastKey::Kind(CardKind::Sha), public_gain);
    Ok(duel)
}
