use super::Duel;
use duel_core::{Game, SeatId};
use wasm_bindgen::prelude::*;

/// Install the real trigger on the human and the next seat before the first turn.
#[wasm_bindgen(js_name = classicXunxunFixture)]
pub fn classic_xunxun_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    if count != 8 || seat as usize >= count {
        return Err(JsError::new("The xunxun fixture needs eight seats"));
    }
    let registrations = [seat, (seat + 1) % count as SeatId]
        .into_iter()
        .map(duel_core::xunxun::registration)
        .collect();
    let game = Game::classic_with_triggers(seed, seat, hero, count, registrations)
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    Ok(Duel { game })
}
