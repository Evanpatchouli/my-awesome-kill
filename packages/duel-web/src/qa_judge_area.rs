use super::Duel;
use duel_core::{
    ActiveSkillRegistration, ChoiceBranch, Game, SeatId, TriggerOrder, TriggerRegistration,
    TriggerRole, TriggerStep,
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
        effect: Arc::new(|_, step, selected| {
            if step == 0 {
                TriggerStep::Choose {
                    branches: vec![choice(0, "允许弃置"), choice(1, "取消弃置")],
                    then: 1,
                }
            } else if selected.is_some_and(|s| s.branch == 1) {
                TriggerStep::CancelSubject
            } else {
                TriggerStep::Done
            }
        }),
    };
    let mut game = Game::classic_with_triggers(seed, seat, hero, count, vec![intercept])
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
