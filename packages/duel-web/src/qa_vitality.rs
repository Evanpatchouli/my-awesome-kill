use super::Duel;
use duel_core::{ActiveSkillRegistration, ChoiceBranch, Game, SeatId, TriggerStep};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// A real classic table with Weizhong and two fixture-only active skills.
#[wasm_bindgen(js_name = classicVitalityFixture)]
pub fn classic_vitality_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    if count != 8 || seat as usize >= count {
        return Err(JsError::new("The vitality fixture needs eight seats"));
    }
    let target = (seat + 1) % count as SeatId;
    let mut game = Game::classic_with_triggers(
        seed,
        seat,
        hero,
        count,
        duel_core::vitality_content::weizhong(target).into(),
    )
    .map_err(|error| JsError::new(&format!("{error:?}")))?;
    for (skill, label, gain) in [
        ("fixture_gain_max_hp", "增加体力上限", true),
        ("fixture_lose_max_hp", "减少体力上限", false),
    ] {
        game.register_active_skill(ActiveSkillRegistration {
            owner: seat,
            skill: skill.into(),
            limited: false,
            choice: Arc::new(move |game, owner| {
                let targets: Vec<_> = game
                    .seat_order_from(owner)
                    .ok()?
                    .into_iter()
                    .filter(|&id| game.is_alive(id))
                    .collect();
                (!targets.is_empty()).then(|| ChoiceBranch {
                    id: 1,
                    label: label.into(),
                    cards: vec![],
                    min_cards: 0,
                    max_cards: 0,
                    targets,
                    min_targets: 1,
                    max_targets: 1,
                })
            }),
            effect: Arc::new(move |ctx, _, selected| {
                if let Some(target) = selected
                    .and_then(|selection| selection.targets.first())
                    .copied()
                {
                    let owner = ctx.owner();
                    if gain {
                        let _ = ctx.gain_max_hp(target, 1, Some(owner));
                    } else {
                        let _ = ctx.lose_max_hp(target, 1, Some(owner));
                    }
                }
                TriggerStep::Done
            }),
        })
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    }
    Ok(Duel { game })
}
