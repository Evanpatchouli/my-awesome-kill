use super::Duel;
use duel_core::{Game, SeatId};
use wasm_bindgen::prelude::*;

/// Install the real team skill without adding it to the ordinary classic roster.
#[wasm_bindgen(js_name = classicZhongyiFixture)]
pub fn classic_zhongyi_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    if count != 8 || seat as usize >= count {
        return Err(JsError::new("The zhongyi fixture needs eight seats"));
    }
    let mut game = Game::classic(seed, seat, hero, count)
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    for id in 0..count as SeatId {
        game.set_team(id, u32::from(id % 2))
            .map_err(|error| JsError::new(&format!("{error:?}")))?;
    }
    game.register_active_skill(duel_core::zhongyi::registration(seat))
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    Ok(Duel { game })
}
