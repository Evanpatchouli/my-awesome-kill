use super::Duel;
use duel_core::classic_data::Suit;
use duel_core::{
    CardKind, ChoiceBranch, Game, SeatId, TriggerOrder, TriggerRegistration, TriggerRole,
    TriggerStep,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// Use real phase content; only prepare cards or pause the extra turn for inspection.
#[wasm_bindgen(js_name = classicPhaseSkillsFixture)]
pub fn classic_phase_skills_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
    scenario: &str,
) -> Result<Duel, JsError> {
    if count != 8 || seat as usize >= count {
        return Err(JsError::new("The phase fixture needs eight seats"));
    }
    let mut content = Vec::new();
    if scenario == "dangxian" {
        content.push(TriggerRegistration {
            owner: seat,
            skill: "phase_fixture".into(),
            timing: "phaseBegin".into(),
            role: TriggerRole::Player,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|game, _, _| {
                game.view(None)
                    .is_ok_and(|v| v.turn.is_some_and(|t| t.number == 1))
            }),
            effect: Arc::new(move |ctx, _, _| {
                let target = (seat + 1) % count as SeatId;
                let old: Vec<_> = ctx.game().view(Some(target)).unwrap().seats[target as usize]
                    .hand
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|card| card.id)
                    .collect();
                ctx.discard_hand(target, &old).unwrap();
                ctx.gain_created_card(seat, CardKind::Sha, Suit::Spade, 2, None)
                    .unwrap();
                ctx.gain_created_card(seat, CardKind::Sha, Suit::Spade, 3, None)
                    .unwrap();
                TriggerStep::Done
            }),
        });
        content.push(duel_core::phase_content::dangxian(seat));
    } else if scenario == "fangquan" {
        content.push(duel_core::phase_content::fangquan(seat));
        content.push(TriggerRegistration {
            owner: seat,
            skill: "phase_fixture_pause".into(),
            timing: "phaseBegin".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(move |game, event, _| {
                event.player == Some((seat + 1) % count as SeatId)
                    && game
                        .view(None)
                        .is_ok_and(|v| v.turn.is_some_and(|t| (2..=3).contains(&t.number)))
            }),
            effect: Arc::new(|_, stage, _| {
                if stage == 0 {
                    TriggerStep::Choose {
                        branches: vec![ChoiceBranch {
                            id: 1,
                            label: "继续当前回合".into(),
                            cards: vec![],
                            targets: vec![],
                            min_cards: 0,
                            max_cards: 0,
                            min_targets: 0,
                            max_targets: 0,
                        }],
                        then: 1,
                    }
                } else {
                    TriggerStep::Done
                }
            }),
        });
    } else {
        return Err(JsError::new("Unknown phase scenario"));
    }
    let game = Game::classic_with_triggers(seed, seat, hero, count, content)
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    Ok(Duel { game })
}
