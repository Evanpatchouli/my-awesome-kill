use super::Duel;
use duel_core::classic_data::Suit;
use duel_core::{
    CardKind, Game, SeatId, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// Prepare physical cards, then let the real 巧说 registration run normally.
#[wasm_bindgen(js_name = classicQiaoshuiFixture)]
pub fn classic_qiaoshui_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    if count < 8 || seat as usize >= count {
        return Err(JsError::new("The qiaoshui fixture needs eight seats"));
    }
    let b = (seat + 1) % count as SeatId;
    let d = (seat + 3) % count as SeatId;
    let setup = TriggerRegistration {
        owner: seat,
        skill: "qiaoshui_fixture".into(),
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
            for target in [seat, b, d] {
                let old: Vec<_> = ctx.game().view(Some(target)).unwrap().seats[target as usize]
                    .hand
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|card| card.id)
                    .collect();
                ctx.discard_hand(target, &old).unwrap();
            }
            ctx.gain_created_card(seat, CardKind::Tao, Suit::Heart, 13, None)
                .unwrap();
            ctx.gain_created_card(b, CardKind::Tao, Suit::Club, 1, None)
                .unwrap();
            ctx.gain_created_card(seat, CardKind::Sha, Suit::Spade, 2, None)
                .unwrap();
            ctx.gain_created_card(seat, CardKind::Juedou, Suit::Spade, 3, None)
                .unwrap();
            TriggerStep::Done
        }),
    };
    let game = Game::classic_with_triggers(
        seed,
        seat,
        hero,
        count,
        vec![setup, duel_core::qiaoshui::registration(seat)],
    )
    .map_err(|error| JsError::new(&format!("{error:?}")))?;
    Ok(Duel { game })
}
