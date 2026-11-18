use super::Duel;
use duel_core::classic_data::Suit;
use duel_core::{
    CardKind, DamageNature, Game, SeatId, TriggerOrder, TriggerRegistration, TriggerRole,
    TriggerStep,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// Grant three private cards and set up the two opponents through ordinary content events.
#[wasm_bindgen(js_name = classicElementalFixture)]
pub fn classic_elemental_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    if count < 3 {
        return Err(JsError::new("The elemental fixture needs two opponents"));
    }
    let opening = TriggerRegistration {
        owner: seat,
        skill: "elemental_fixture".into(),
        timing: "phaseZhunbeiBegin".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|game, _, _| {
            game.view(None)
                .is_ok_and(|view| view.turn.is_some_and(|turn| turn.number == 1))
        }),
        effect: Arc::new(move |ctx, _, _| {
            ctx.change_armor((seat + 1) % count as SeatId, 1).unwrap();
            ctx.turn_over((seat + 2) % count as SeatId, Some(true))
                .unwrap();
            ctx.gain_created_card(seat, CardKind::Tiesuo, Suit::Club, 10, None)
                .unwrap();
            ctx.gain_created_card(
                seat,
                CardKind::Sha,
                Suit::Heart,
                4,
                Some(DamageNature::Fire),
            )
            .unwrap();
            ctx.gain_created_card(
                seat,
                CardKind::Sha,
                Suit::Spade,
                5,
                Some(DamageNature::Thunder),
            )
            .unwrap();
            TriggerStep::Done
        }),
    };
    let game = Game::classic_with_triggers(seed, seat, hero, count, vec![opening])
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    Ok(Duel { game })
}
