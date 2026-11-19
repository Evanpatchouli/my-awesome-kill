use super::Duel;
use duel_core::{
    classic_data::Suit, ActiveSkillRegistration, CardKind, ChoiceBranch, Game, SeatId,
    TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

fn choice(id: u32, label: &str) -> ChoiceBranch {
    ChoiceBranch {
        id,
        label: label.into(),
        cards: vec![],
        min_cards: 0,
        max_cards: 0,
        targets: vec![],
        min_targets: 0,
        max_targets: 0,
    }
}

#[wasm_bindgen(js_name = classicJudgeAreaFixture)]
pub fn classic_judge_area_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    let intercept = TriggerRegistration {
        owner: seat,
        skill: "fixture_judge_discard".into(),
        timing: "discardBefore".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|game, event, _| {
            event
                .parent
                .and_then(|id| game.event_kind(id))
                .is_some_and(|kind| matches!(kind, duel_core::EventKind::DisableJudge { .. }))
        }),
        effect: Arc::new(|ctx, step, selected| {
            let sword = ctx
                .event()
                .cards
                .iter()
                .find(|c| c.kind == CardKind::Qinggang)
                .copied();
            if step == 0 {
                let mut branches = vec![choice(0, "允许弃置"), choice(1, "取消弃置")];
                if sword.is_some() {
                    branches.push(choice(2, "取回并装备青釭剑"));
                }
                TriggerStep::Choose { branches, then: 1 }
            } else if selected.is_some_and(|s| s.branch == 1) {
                TriggerStep::CancelSubject
            } else if let (Some(card), Some(seat)) = (sword, ctx.event().player) {
                if step == 1
                    && selected.is_some_and(|s| s.branch == 2)
                    && ctx.gain_owned_card(seat, seat, card.id).is_ok()
                {
                    return TriggerStep::Continue(2);
                }
                if step == 2 {
                    let _ = ctx.equip_from_hand(seat, card.id);
                }
                TriggerStep::Done
            } else {
                TriggerStep::Done
            }
        }),
    };
    let mut game = Game::classic_with_triggers(seed, seat, hero, count, vec![intercept])
        .map_err(|err| JsError::new(&format!("{err:?}")))?;
    game.register_active_skill(ActiveSkillRegistration {
        owner: seat,
        skill: "fixture_gain_judge_material".into(),
        limited: false,
        choice: Arc::new(|game, _| {
            let targets: Vec<_> = game
                .living_seats()
                .iter()
                .copied()
                .filter(|&id| {
                    game.is_in(id)
                        && game
                            .judgments(id)
                            .any(|j| j.is_virtual() && j.is_attached())
                })
                .collect();
            (!targets.is_empty()).then(|| ChoiceBranch {
                targets,
                min_targets: 1,
                max_targets: 1,
                ..choice(0, "取回判定材料")
            })
        }),
        effect: Arc::new(|ctx, _, selected| {
            if let Some(&id) = selected.and_then(|s| s.targets.first()) {
                let material = ctx
                    .game()
                    .judgments(id)
                    .find(|j| j.is_virtual() && j.is_attached())
                    .map(|j| j.material.id);
                if let Some(card) = material {
                    let _ = ctx.gain_owned_card(id, id, card);
                }
            }
            TriggerStep::Done
        }),
    })
    .map_err(|err| JsError::new(&format!("{err:?}")))?;
    game.register_active_skill(ActiveSkillRegistration {
        owner: seat,
        skill: "fixture_sword_lebu".into(),
        limited: false,
        choice: Arc::new(|game, owner| {
            let targets: Vec<_> = game
                .living_seats()
                .iter()
                .copied()
                .filter(|&seat| {
                    seat != owner && game.is_in(seat) && game.can_add_judge(seat, CardKind::Lebu)
                })
                .collect();
            (!targets.is_empty()).then(|| ChoiceBranch {
                targets,
                min_targets: 1,
                max_targets: 1,
                ..choice(0, "用青釭剑放置乐不思蜀")
            })
        }),
        effect: Arc::new(|ctx, _, selected| {
            let Some(&target) = selected.and_then(|s| s.targets.first()) else {
                return TriggerStep::Done;
            };
            let Ok(id) = ctx.gain_created_card(target, CardKind::Qinggang, Suit::Diamond, 6, None)
            else {
                return TriggerStep::Done;
            };
            TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
                let _ = ctx.add_judge(target, id, CardKind::Lebu);
                TriggerStep::Done
            }))
        }),
    })
    .map_err(|err| JsError::new(&format!("{err:?}")))?;
    for (disable, label, skill) in [
        (true, "废除判定区", "fixture_disable_judge"),
        (false, "恢复判定区", "fixture_enable_judge"),
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
                    .filter(|&target| {
                        game.is_in(target) && (disable || game.is_judge_disabled(target))
                    })
                    .collect();
                (!targets.is_empty()).then(|| ChoiceBranch {
                    targets,
                    min_targets: 1,
                    max_targets: 1,
                    ..choice(0, label)
                })
            }),
            effect: Arc::new(move |ctx, _, selected| {
                if let Some(&target) = selected.and_then(|s| s.targets.first()) {
                    if disable {
                        let _ = ctx.disable_judge(target);
                    } else {
                        let _ = ctx.enable_judge(target);
                    }
                }
                TriggerStep::Done
            }),
        })
        .map_err(|err| JsError::new(&format!("{err:?}")))?;
    }
    Ok(Duel { game })
}
