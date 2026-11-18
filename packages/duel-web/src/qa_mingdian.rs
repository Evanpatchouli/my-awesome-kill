use super::Duel;
use duel_core::{Game, SeatId};
use wasm_bindgen::prelude::*;

/// Add Mingdian to the human and next seat without changing the classic roster.
#[wasm_bindgen(js_name = classicMingdianFixture)]
pub fn classic_mingdian_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    if count != 8 || seat as usize >= count {
        return Err(JsError::new("The mingdian fixture needs eight seats"));
    }
    let mut game = Game::classic(seed, seat, hero, count)
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    for owner in [seat, (seat + 1) % count as SeatId] {
        duel_core::mingdian::install(&mut game, owner)
            .map_err(|error| JsError::new(&format!("{error:?}")))?;
    }
    Ok(Duel { game })
}
