use super::Duel;
use duel_core::{
    ActiveSkillRegistration, ChoiceBranch, Game, SeatId, TriggerOrder, TriggerRegistration,
    TriggerRole, TriggerStep,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

fn option(id: u32, label: &str) -> ChoiceBranch {
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

#[derive(Clone, Copy)]
enum Operation {
    Die,
    Direct,
    Event,
}

#[wasm_bindgen(js_name = classicRevivalFixture)]
pub fn classic_revival_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    let before = TriggerRegistration {
        owner: seat,
        skill: "fixture_decide_revival".into(),
        timing: "reviveBefore".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        silent: true,
        label: None,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|_, step, selection| {
            if step == 0 {
                TriggerStep::Choose {
                    branches: vec![option(0, "允许复活"), option(1, "取消复活")],
                    then: 1,
                }
            } else if selection.is_some_and(|s| s.branch == 1) {
                TriggerStep::CancelSubject
            } else {
                TriggerStep::Done
            }
        }),
    };
    let mut game = Game::classic_with_triggers(seed, seat, hero, count, vec![before])
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    for (operation, skill, label) in [
        (Operation::Die, "fixture_die", "令其阵亡"),
        (Operation::Direct, "fixture_revive", "直接复活"),
        (Operation::Event, "fixture_revive_event", "发动复活"),
    ] {
        game.register_active_skill(ActiveSkillRegistration {
            owner: seat,
            skill: skill.into(),
            limited: false,
            choice: Arc::new(move |game, owner| {
                let candidates = match operation {
                    Operation::Die => game.living_seats(),
                    Operation::Direct | Operation::Event => game.dead_seats(),
                };
                let targets: Vec<_> = candidates
                    .iter()
                    .copied()
                    .filter(|&id| id != owner)
                    .collect();
                (!targets.is_empty()).then(|| ChoiceBranch {
                    targets,
                    min_targets: 1,
                    max_targets: 1,
                    ..option(0, label)
                })
            }),
            effect: Arc::new(move |ctx, _, selection| {
                if let Some(&target) = selection.and_then(|s| s.targets.first()) {
                    match operation {
                        Operation::Die => ctx.die(target, None),
                        Operation::Direct => ctx.revive(target, Some(2)),
                        Operation::Event => ctx.revive_event(target, Some(0)),
                    }
                    .expect("the selected seat belongs to this fixture");
                }
                TriggerStep::Done
            }),
        })
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    }
    Ok(Duel { game })
}
