use super::Duel;
use duel_core::{
    classic_data::Suit, ActiveSkillRegistration, CardKind, ChoiceBranch, EquipmentSlot, Game,
    MountSlots, SeatId, TriggerEffect, TriggerStep,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

fn choice(label: &str) -> ChoiceBranch {
    ChoiceBranch {
        id: 0,
        label: label.into(),
        cards: vec![],
        min_cards: 0,
        max_cards: 0,
        targets: vec![],
        min_targets: 0,
        max_targets: 0,
    }
}
fn equip_created(
    owner: SeatId,
    target: SeatId,
    kind: CardKind,
    next: TriggerEffect,
) -> TriggerEffect {
    Arc::new(move |ctx, _, _| {
        let id = ctx
            .gain_created_card(owner, kind, Suit::Diamond, 6, None)
            .expect("fixture gain");
        let next = next.clone();
        TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
            if owner != target {
                ctx.give_cards(owner, target, &[id], true)
                    .expect("fixture public transfer");
            }
            let next = next.clone();
            TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
                ctx.equip_from_hand(target, id).expect("fixture equip");
                TriggerStep::ContinueWith(next.clone())
            }))
        }))
    })
}
fn prepare(owner: SeatId, target: SeatId) -> TriggerEffect {
    let done: TriggerEffect = Arc::new(|_, _, _| TriggerStep::Done);
    let second = equip_created(owner, target, CardKind::Qinggang, done);
    let first = equip_created(owner, target, CardKind::Zhuge, second);
    Arc::new(move |ctx, _, _| {
        ctx.expand_equip(target, vec![EquipmentSlot::Weapon])
            .expect("fixture expand");
        TriggerStep::ContinueWith(first.clone())
    })
}
fn skill(
    game: &mut Game,
    owner: SeatId,
    name: &str,
    label: &'static str,
    effect: TriggerEffect,
) -> Result<(), JsError> {
    game.register_active_skill(ActiveSkillRegistration {
        owner,
        skill: name.into(),
        limited: false,
        choice: Arc::new(move |_, _| Some(choice(label))),
        effect,
    })
    .map_err(|err| JsError::new(&format!("{err:?}")))?;
    Ok(())
}
#[wasm_bindgen(js_name = classicEquipSlotsFixture)]
pub fn classic_equip_slots_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    if seat != 0 || count != 8 {
        return Err(JsError::new("fixture requires seat 0 and eight seats"));
    }
    let mut game =
        Game::classic(seed, seat, hero, count).map_err(|err| JsError::new(&format!("{err:?}")))?;
    skill(
        &mut game,
        seat,
        "fixture_prepare_b",
        "准备双武器",
        prepare(seat, 1),
    )?;
    skill(
        &mut game,
        seat,
        "fixture_prepare_a",
        "准备自身双武器",
        prepare(seat, seat),
    )?;
    skill(
        &mut game,
        seat,
        "fixture_disable_b",
        "废除武器栏",
        Arc::new(|ctx, _, _| {
            ctx.disable_equip(1, vec![EquipmentSlot::Weapon])
                .expect("fixture disable");
            TriggerStep::Done
        }),
    )?;
    let finish: TriggerEffect = Arc::new(|_, _, _| TriggerStep::Done);
    skill(
        &mut game,
        seat,
        "fixture_third_b",
        "替换电脑武器",
        equip_created(seat, 1, CardKind::Qinglong, finish.clone()),
    )?;
    skill(
        &mut game,
        seat,
        "fixture_third_a",
        "替换自身武器",
        equip_created(seat, seat, CardKind::Qinglong, finish),
    )?;
    skill(
        &mut game,
        seat,
        "fixture_detach",
        "分离武器登记",
        Arc::new(move |ctx, _, _| {
            let card = ctx
                .game()
                .equipment_registrations(seat, CardKind::Zhuge)
                .into_iter()
                .find(|c| c.kind == CardKind::Zhuge)
                .expect("fixture sword");
            ctx.add_judge(seat, card.id, CardKind::Lebu)
                .expect("fixture judgment");
            TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
                let key = ctx
                    .game()
                    .judgments(seat)
                    .find(|j| j.material.id == card.id && j.is_virtual())
                    .expect("fixture judgment entry")
                    .key;
                ctx.gain_owned_card(seat, seat, card.id)
                    .expect("fixture recover");
                TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
                    ctx.equip_from_hand(seat, card.id)
                        .expect("fixture reinstall");
                    TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
                        ctx.discard_judgment(seat, key)
                            .expect("fixture discard judgment");
                        TriggerStep::Done
                    }))
                }))
            }))
        }),
    )?;
    Ok(Duel { game })
}

#[wasm_bindgen(js_name = classicCombinedMountFixture)]
pub fn classic_combined_mount_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    if seat != 0 || count != 8 {
        return Err(JsError::new("fixture requires seat 0 and eight seats"));
    }
    let mut game = Game::classic_with_mount_slots(seed, seat, hero, count, MountSlots::Combined)
        .map_err(|err| JsError::new(&format!("{err:?}")))?;
    let done: TriggerEffect = Arc::new(|_, _, _| TriggerStep::Done);
    let second = equip_created(seat, 1, CardKind::Jueying, done.clone());
    let first = equip_created(seat, 1, CardKind::Chitu, second);
    skill(
        &mut game,
        seat,
        "fixture_mount_prepare",
        "准备合马",
        Arc::new(move |ctx, _, _| {
            ctx.expand_equip(
                1,
                vec![EquipmentSlot::DefenseHorse, EquipmentSlot::AttackHorse],
            )
            .expect("fixture expand mounts");
            TriggerStep::ContinueWith(first.clone())
        }),
    )?;
    skill(
        &mut game,
        seat,
        "fixture_mount_third",
        "替换电脑坐骑",
        equip_created(seat, 1, CardKind::Dawan, done),
    )?;
    for (name, label, disable) in [
        ("fixture_mount_disable", "废除合马栏", true),
        ("fixture_mount_enable", "恢复合马栏", false),
    ] {
        game.register_active_skill(ActiveSkillRegistration {
            owner: seat,
            skill: name.into(),
            limited: false,
            choice: Arc::new(move |game, _| {
                game.is_in(1).then(|| ChoiceBranch {
                    targets: vec![1],
                    min_targets: 1,
                    max_targets: 1,
                    ..choice(label)
                })
            }),
            effect: Arc::new(move |ctx, _, selected| {
                let target = selected
                    .and_then(|selection| selection.targets.first())
                    .copied()
                    .expect("fixture target");
                let slots = if disable {
                    vec![
                        EquipmentSlot::DefenseHorse,
                        EquipmentSlot::DefenseHorse,
                        EquipmentSlot::AttackHorse,
                    ]
                } else {
                    vec![
                        EquipmentSlot::DefenseHorse,
                        EquipmentSlot::AttackHorse,
                        EquipmentSlot::AttackHorse,
                    ]
                };
                if disable {
                    ctx.disable_equip(target, slots)
                        .expect("fixture disable mounts");
                } else {
                    ctx.enable_equip(target, slots)
                        .expect("fixture restore mounts");
                }
                TriggerStep::Done
            }),
        })
        .map_err(|err| JsError::new(&format!("{err:?}")))?;
    }
    Ok(Duel { game })
}
