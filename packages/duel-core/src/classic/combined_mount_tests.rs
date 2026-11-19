use crate::classic_data::Suit;
use crate::{
    Action, ActiveSkillRegistration, CardId, CardKind, CardZone, ChoiceBranch, Command, Decision,
    EquipmentSlot, EventKind, Game, MountSlots, SeatId, TriggerEffect, TriggerEvent, TriggerStep,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

const A: SeatId = 0;
const B: SeatId = 1;
const D: SeatId = 3;
const DEFENSE: EquipmentSlot = EquipmentSlot::DefenseHorse;
const ATTACK: EquipmentSlot = EquipmentSlot::AttackHorse;
type Cards = Arc<Mutex<BTreeMap<&'static str, CardId>>>;

fn register(game: &mut Game, name: &str, effect: TriggerEffect) -> u64 {
    game.register_active_skill(ActiveSkillRegistration {
        owner: A,
        skill: name.into(),
        limited: false,
        choice: Arc::new(|_, _| {
            Some(ChoiceBranch {
                id: 0,
                label: "fixture".into(),
                cards: vec![],
                min_cards: 0,
                max_cards: 0,
                targets: vec![],
                min_targets: 0,
                max_targets: 0,
            })
        }),
        effect,
    })
    .unwrap()
}
fn created_horse(name: &'static str, kind: CardKind, ids: Cards) -> TriggerEffect {
    created_horse_for(name, kind, ids, B)
}
fn created_horse_for(
    name: &'static str,
    kind: CardKind,
    ids: Cards,
    target: SeatId,
) -> TriggerEffect {
    Arc::new(move |ctx, _, _| {
        let id = ctx
            .gain_created_card(A, kind, Suit::Spade, 5, None)
            .unwrap();
        ids.lock().unwrap().insert(name, id);
        TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
            ctx.give_cards(A, target, &[id], true).unwrap();
            TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
                ctx.equip_from_hand(target, id).unwrap();
                TriggerStep::Done
            }))
        }))
    })
}
fn recover_horse(ids: Cards) -> TriggerEffect {
    Arc::new(move |ctx, _, _| {
        let id = ids.lock().unwrap()["chitu"];
        ctx.gain_public_pile_card(A, CardZone::DiscardPile, id)
            .unwrap();
        TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
            ctx.give_cards(A, B, &[id], true).unwrap();
            TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
                ctx.equip_from_hand(B, id).unwrap();
                TriggerStep::Done
            }))
        }))
    })
}
fn activate(game: &mut Game, registration: u64) {
    game.submit(Command {
        version: game.version,
        seat: A,
        action: Action::ActivateSkill {
            registration,
            cards: vec![],
            targets: vec![],
        },
    })
    .unwrap();
}
fn pick(game: &mut Game, expected: SeatId, id: CardId) {
    let choice = game
        .view(Some(expected))
        .unwrap()
        .choice
        .expect("real chooseButton prompt");
    assert_eq!(choice.seat, expected);
    assert_eq!(
        game.event_kind(choice.event),
        Some(EventKind::ChooseButton { seat: expected })
    );
    let branch = choice
        .branches
        .iter()
        .find(|b| b.cards.contains(&id))
        .unwrap();
    assert_eq!((branch.min_cards, branch.max_cards), (1, 1));
    game.submit(Command {
        version: game.version,
        seat: expected,
        action: Action::Select {
            event: choice.event,
            branch: branch.id,
            cards: vec![id],
            targets: vec![],
        },
    })
    .unwrap();
}
fn kind(name: &str) -> CardKind {
    match name {
        "chitu" => CardKind::Chitu,
        "jueying" => CardKind::Jueying,
        "dawan" => CardKind::Dawan,
        _ => panic!("unexpected mount"),
    }
}
fn assert_stage(game: &Game, stage: &Value, ids: &Cards) {
    for (seat, label) in [(B, "B"), (D, "D")] {
        let expected = &stage[label];
        let states = [
            game.equipment_slot_state(seat, DEFENSE),
            game.equipment_slot_state(seat, ATTACK),
            game.combined_mount_slot_state(seat),
        ];
        let _: i64 = states[0].enabled;
        for (key, values) in [
            (
                "enabled",
                states.iter().map(|s| s.enabled).collect::<Vec<_>>(),
            ),
            (
                "disabled",
                states.iter().map(|s| s.disabled as i64).collect(),
            ),
            ("empty", states.iter().map(|s| s.empty as i64).collect()),
            (
                "equipable",
                states.iter().map(|s| s.equipable as i64).collect(),
            ),
        ] {
            assert_eq!(
                json!(values),
                expected[key],
                "{} {label} {key}",
                stage["name"]
            );
        }
        let actual = &game.view(None).unwrap().seats[seat as usize];
        let expected_physical: Vec<_> = expected["physical"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| kind(v["name"].as_str().unwrap()))
            .collect();
        assert_eq!(
            actual.equipment.iter().map(|c| c.kind).collect::<Vec<_>>(),
            expected_physical,
            "{} {label} physical",
            stage["name"]
        );
        let expected_registered: Vec<_> = expected["registered"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| kind(v["name"].as_str().unwrap()))
            .collect();
        let registered = game
            .equipment_registrations(seat, CardKind::Jueying)
            .into_iter()
            .chain(game.equipment_registrations(seat, CardKind::Chitu))
            .map(|c| c.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            registered, expected_registered,
            "{} {label} registrations",
            stage["name"]
        );
        assert_eq!(
            actual.equipment_disabled_markers.len(),
            expected["markers"].as_array().unwrap().len()
        );
        assert!(actual
            .equipment_disabled_markers
            .iter()
            .all(|&slot| slot == DEFENSE));
        assert_eq!(
            json!([
                states[0].enabled > 0,
                states[0].empty > 0,
                states[0].disabled > 0
            ]),
            expected["horse"]
        );
        for (i, horse) in [CardKind::Chitu, CardKind::Jueying].into_iter().enumerate() {
            assert_eq!(
                json!([
                    game.can_equip(seat, horse, false),
                    game.can_equip(seat, horse, true)
                ]),
                expected["canEquip"][i]
            );
        }
        assert_eq!(
            json!([
                game.distance(D, seat).unwrap_or(0),
                game.distance(seat, D).unwrap_or(0)
            ]),
            expected["distanceToD"]
        );
    }
    for (&name, &id) in ids.lock().unwrap().iter() {
        let count = game
            .seats
            .iter()
            .map(|p| {
                p.hand
                    .iter()
                    .chain(&p.equipment)
                    .chain(&p.judges)
                    .chain(&p.expansions)
                    .filter(|card| card.id == id)
                    .count()
            })
            .sum::<usize>()
            + game
                .discard
                .iter()
                .chain(&game.resolving)
                .chain(&game.transferring)
                .chain(&game.classic.as_ref().unwrap().deck)
                .filter(|card| card.id == id)
                .count();
        assert_eq!(count, 1, "{name} conserved at {}", stage["name"]);
    }
}
fn event_name(game: &Game, event: Option<u64>) -> Option<&'static str> {
    match game.event_kind(event?)? {
        EventKind::DisableEquip { .. } => Some("disableEquip"),
        EventKind::EnableEquip { .. } => Some("enableEquip"),
        EventKind::ExpandEquip { .. } => Some("expandEquip"),
        EventKind::ReplaceEquip { .. } => Some("replaceEquip"),
        EventKind::ChooseButton { .. } => Some("chooseButton"),
        EventKind::LoseToDiscardpile { .. } => Some("loseToDiscardpile"),
        EventKind::Lose { .. } => Some("lose"),
        EventKind::Equip { .. } => Some("equip"),
        EventKind::ActivateSkill { .. } | EventKind::ActiveSkillEffect { .. } => {
            Some("reference_fixture_use")
        }
        _ => None,
    }
}
fn common(game: &Game, event: &TriggerEvent, ids: &Cards) -> Value {
    let ids = ids.lock().unwrap();
    let labels = |cards: &[CardId]| {
        cards
            .iter()
            .filter_map(|id| {
                ids.iter()
                    .find(|(_, value)| *value == id)
                    .map(|(name, _)| *name)
            })
            .collect::<Vec<_>>()
    };
    let seat = |id: Option<SeatId>| id.map(|n| ((b'A' + n as u8) as char).to_string());
    let slots = event.equipment_slots.as_deref().map(|slots| {
        slots
            .iter()
            .map(|slot| match slot {
                EquipmentSlot::DefenseHorse => "equip3",
                EquipmentSlot::AttackHorse => "equip4",
                _ => "other",
            })
            .collect::<Vec<_>>()
    });
    json!([
        event.timing,
        event_name(game, Some(event.subject)),
        event_name(game, event.parent),
        seat(event.player),
        seat(event.source),
        slots,
        labels(&event.cards.iter().map(|c| c.id).collect::<Vec<_>>()),
        labels(&event.button_links)
    ])
}

#[test]
fn combined_mounts_match_real_eight_seat_upstream_trace() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/combined-mounts.json"
    ))
    .unwrap();
    let mut game = (0..100)
        .map(|seed| {
            Game::classic_with_mount_slots(seed, A, "guanyu", 8, MountSlots::Combined).unwrap()
        })
        .find(|game| matches!(game.decision, Some(Decision::Play(A))))
        .expect("human active seat");
    for player in &mut game.seats {
        player.hero = None;
    }
    assert_eq!(game.seats.len(), 8);
    let ids: Cards = Arc::new(Mutex::new(BTreeMap::new()));
    let observer = Arc::new(Mutex::new(Vec::new()));
    let observed = observer.clone();
    let observer_ids = ids.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if matches!(
            event.timing.as_str(),
            "disableEquipBefore"
                | "disableEquipBegin"
                | "disableEquipEnd"
                | "disableEquipAfter"
                | "enableEquipBefore"
                | "enableEquipBegin"
                | "enableEquipEnd"
                | "enableEquipAfter"
                | "expandEquipBefore"
                | "expandEquipBegin"
                | "expandEquipEnd"
                | "expandEquipAfter"
                | "chooseButtonBefore"
                | "chooseButtonBegin"
                | "chooseButtonEnd"
                | "chooseButtonAfter"
                | "loseToDiscardpileBefore"
                | "loseToDiscardpileBegin"
                | "loseToDiscardpile"
                | "loseToDiscardpileEnd"
                | "loseToDiscardpileAfter"
        ) {
            observed
                .lock()
                .unwrap()
                .push(common(game, event, &observer_ids));
        }
    }));
    let chitu = register(
        &mut game,
        "combined_chitu",
        created_horse("chitu", CardKind::Chitu, ids.clone()),
    );
    let jueying = register(
        &mut game,
        "combined_jueying",
        created_horse("jueying", CardKind::Jueying, ids.clone()),
    );
    let dawan = register(
        &mut game,
        "combined_dawan",
        created_horse("dawan", CardKind::Dawan, ids.clone()),
    );
    let recover = register(&mut game, "combined_recover", recover_horse(ids.clone()));
    let operations = [
        ("combined_expand", B, vec![DEFENSE, ATTACK], 0),
        (
            "combined_disable_duplicate",
            B,
            vec![DEFENSE, DEFENSE, ATTACK],
            1,
        ),
        ("combined_disable_pair", B, vec![DEFENSE, ATTACK], 1),
        (
            "combined_enable_duplicates",
            B,
            vec![DEFENSE, ATTACK, ATTACK],
            2,
        ),
        ("combined_enable_attack", B, vec![ATTACK], 2),
        ("combined_expand_d", D, vec![DEFENSE, ATTACK, ATTACK], 0),
    ]
    .into_iter()
    .map(|(name, seat, slots, op)| {
        register(
            &mut game,
            name,
            Arc::new(move |ctx, _, _| {
                match op {
                    0 => ctx.expand_equip(seat, slots.clone()).unwrap(),
                    1 => ctx.disable_equip(seat, slots.clone()).unwrap(),
                    _ => ctx.enable_equip(seat, slots.clone()).unwrap(),
                };
                TriggerStep::Done
            }),
        )
    })
    .collect::<Vec<_>>();
    let stage = |name| {
        golden["stages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()
    };
    assert_stage(&game, stage("empty"), &ids);
    for (name, action) in [
        ("equip-chitu", chitu),
        ("equip-jueying", jueying),
        ("expand-3-4", operations[0]),
        ("equip-chitu-again", recover),
    ] {
        activate(&mut game, action);
        assert_stage(&game, stage(name), &ids);
    }
    observer.lock().unwrap().clear();
    activate(&mut game, dawan);
    let chitu_id = ids.lock().unwrap()["chitu"];
    pick(&mut game, B, chitu_id);
    assert_stage(&game, stage("equip-third"), &ids);
    let observed = observer.lock().unwrap().clone();
    let actual: Vec<_> = observed
        .into_iter()
        .filter(|v| v[0].as_str().unwrap().starts_with("chooseButton"))
        .collect();
    let expected: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            r[0] == "equip-third"
                && r[1] == "request"
                && r[2].as_str().unwrap().starts_with("chooseButton")
        })
        .map(|r| json!([r[2], r[3], r[4], r[5], r[6], r[7], r[8], r[9]]))
        .collect();
    assert_eq!(actual, expected, "third mount chooser and event parent");
    for (name, action) in [
        ("disable-3-3-4", operations[1]),
        ("disable-3-4-first", operations[2]),
        ("disable-3-4-second", operations[2]),
        ("enable-3-4-4", operations[3]),
        ("enable-4", operations[4]),
        ("expand-d-3-4-4", operations[5]),
    ] {
        observer.lock().unwrap().clear();
        activate(&mut game, action);
        assert_stage(&game, stage(name), &ids);
        let expected: Vec<_> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                r[0] == name
                    && r[1] == "request"
                    && matches!(
                        r[2].as_str().unwrap(),
                        "disableEquipBefore"
                            | "disableEquipBegin"
                            | "disableEquipEnd"
                            | "disableEquipAfter"
                            | "enableEquipBefore"
                            | "enableEquipBegin"
                            | "enableEquipEnd"
                            | "enableEquipAfter"
                            | "expandEquipBefore"
                            | "expandEquipBegin"
                            | "expandEquipEnd"
                            | "expandEquipAfter"
                            | "loseToDiscardpileBefore"
                            | "loseToDiscardpileBegin"
                            | "loseToDiscardpile"
                            | "loseToDiscardpileEnd"
                            | "loseToDiscardpileAfter"
                    )
            })
            .map(|r| json!([r[2], r[3], r[4], r[5], r[6], r[7], r[8], r[9]]))
            .collect();
        assert_eq!(
            *observer.lock().unwrap(),
            expected,
            "{name} event common fields"
        );
        if name == "disable-3-3-4" {
            let recorded: Vec<_> = game
                .loss_history()
                .iter()
                .filter(|loss| loss.seat == B && loss.cards.len() == 2)
                .collect();
            assert_eq!(recorded.len(), 1, "both mounts leave in one loss");
        }
    }
}

#[test]
fn separate_mount_mode_retains_two_independent_slots() {
    let game = Game::classic_with_mount_slots(5, A, "guanyu", 8, MountSlots::Separate).unwrap();
    let defense = game.equipment_slot_state(B, DEFENSE);
    let attack = game.equipment_slot_state(B, ATTACK);
    assert_eq!(
        (
            defense.enabled,
            defense.empty,
            defense.equipable,
            defense.disabled
        ),
        (1, 1, 1, 0)
    );
    assert_eq!(
        (
            attack.enabled,
            attack.empty,
            attack.equipable,
            attack.disabled
        ),
        (1, 1, 1, 0)
    );
}

#[test]
fn combined_mount_replay_has_explicit_kind_and_legacy_seeded_stays_separate() {
    use crate::ReplaySetup;
    let legacy = ReplaySetup::Seeded {
        seed: 5,
        human: A,
        hero: "guanyu".into(),
        seats: 8,
    };
    let encoded = serde_json::to_value(&legacy).unwrap();
    assert_eq!(
        encoded,
        json!({ "kind": "seeded", "seed": 5, "human": 0, "hero": "guanyu", "seats": 8 })
    );
    let decoded: ReplaySetup = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        decoded
            .start()
            .unwrap()
            .equipment_slot_state(B, ATTACK)
            .enabled,
        1
    );
    let combined = ReplaySetup::SeededCombinedMounts {
        seed: 5,
        human: A,
        hero: "guanyu".into(),
        seats: 8,
    };
    let encoded = serde_json::to_value(&combined).unwrap();
    assert_eq!(encoded["kind"], "seededCombinedMounts");
    let decoded: ReplaySetup = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        decoded
            .start()
            .unwrap()
            .equipment_slot_state(B, ATTACK)
            .enabled,
        0
    );
}

fn playable(mode: MountSlots) -> Game {
    let mut game = (0..100)
        .map(|seed| Game::classic_with_mount_slots(seed, A, "guanyu", 8, mode).unwrap())
        .find(|game| matches!(game.decision, Some(Decision::Play(A))))
        .expect("human active seat");
    for player in &mut game.seats {
        player.hero = None;
    }
    game
}

#[test]
fn literal_mount_alias_is_independent_in_separate_mode() {
    let mut game = playable(MountSlots::Separate);
    let expand = register(
        &mut game,
        "alias_expand",
        Arc::new(|ctx, _, _| {
            ctx.expand_equip(B, vec![DEFENSE, DEFENSE]).unwrap();
            TriggerStep::Done
        }),
    );
    let disable = register(
        &mut game,
        "alias_disable",
        Arc::new(|ctx, _, _| {
            ctx.disable_equip(B, vec![DEFENSE]).unwrap();
            TriggerStep::Done
        }),
    );
    activate(&mut game, expand);
    let alias = game.combined_mount_slot_state(B);
    assert_eq!(
        (alias.enabled, alias.disabled, alias.empty, alias.equipable),
        (1, 0, 3, 3)
    );
    activate(&mut game, disable);
    let alias = game.combined_mount_slot_state(B);
    assert_eq!(
        (alias.enabled, alias.disabled, alias.empty, alias.equipable),
        (1, 0, 2, 2)
    );
    assert_eq!(
        game.combined_mount_slot_state(999),
        crate::EquipmentSlotState::default()
    );
}

#[test]
fn multiple_mount_distances_count_all_live_registrations_and_payment() {
    for mode in [MountSlots::Separate, MountSlots::Combined] {
        let mut game = playable(mode);
        let ids: Cards = Arc::new(Mutex::new(BTreeMap::new()));
        let expand_attack = register(
            &mut game,
            "distance_expand_attack",
            Arc::new(|ctx, _, _| {
                ctx.expand_equip(B, vec![ATTACK]).unwrap();
                TriggerStep::Done
            }),
        );
        let expand_defense = register(
            &mut game,
            "distance_expand_defense",
            Arc::new(|ctx, _, _| {
                ctx.expand_equip(4, vec![DEFENSE]).unwrap();
                TriggerStep::Done
            }),
        );
        let chitu = register(
            &mut game,
            "distance_chitu",
            created_horse_for("chitu", CardKind::Chitu, ids.clone(), B),
        );
        let dawan = register(
            &mut game,
            "distance_dawan",
            created_horse_for("dawan", CardKind::Dawan, ids.clone(), B),
        );
        let jueying = register(
            &mut game,
            "distance_jueying",
            created_horse_for("jueying", CardKind::Jueying, ids.clone(), 4),
        );
        let dilu = register(
            &mut game,
            "distance_dilu",
            created_horse_for("dilu", CardKind::Dilu, ids.clone(), 4),
        );
        assert_eq!(game.distance(B, 4), Ok(3));
        activate(&mut game, expand_attack);
        activate(&mut game, chitu);
        assert_eq!(game.distance(B, 4), Ok(2));
        activate(&mut game, dawan);
        let chitu_id = ids.lock().unwrap()["chitu"];
        assert_eq!(game.distance(B, 4), Ok(1));
        assert_eq!(game.distance_without(B, 4, Some(chitu_id)), Ok(2));
        activate(&mut game, expand_defense);
        activate(&mut game, jueying);
        assert_eq!(game.distance(B, 4), Ok(2));
        activate(&mut game, dilu);
        assert_eq!(game.distance(B, 4), Ok(3));
        assert_eq!(game.distance_without(B, 4, Some(chitu_id)), Ok(4));
        let target_id = ids.lock().unwrap()["jueying"];
        assert_eq!(game.distance_without(B, 4, Some(target_id)), Ok(2));
        let detach = register(
            &mut game,
            "distance_detach",
            Arc::new(move |ctx, _, _| {
                ctx.add_judge(B, chitu_id, CardKind::Lebu).unwrap();
                TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
                    let key = ctx
                        .game()
                        .judgments(B)
                        .find(|entry| entry.material.id == chitu_id && entry.is_virtual())
                        .unwrap()
                        .key;
                    ctx.gain_owned_card(B, B, chitu_id).unwrap();
                    TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
                        ctx.equip_from_hand(B, chitu_id).unwrap();
                        TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
                            ctx.discard_judgment(B, key).unwrap();
                            TriggerStep::Done
                        }))
                    }))
                }))
            }),
        );
        activate(&mut game, detach);
        assert_eq!(
            game.view(None).unwrap().seats[B as usize].detached_equipment,
            [CardKind::Chitu]
        );
        assert_eq!(game.distance(B, 4), Ok(3));
        assert_eq!(game.distance_without(B, 4, Some(chitu_id)), Ok(4));
        activate(&mut game, expand_attack);
        let reinstall = register(&mut game, "distance_reinstall", recover_horse(ids.clone()));
        activate(&mut game, reinstall);
        assert_eq!(
            game.distance(B, 4),
            Ok(2),
            "same material now backs two effective registrations"
        );
        assert_eq!(
            game.distance_without(B, 4, Some(chitu_id)),
            Ok(4),
            "payment excludes both registrations referencing that material"
        );
        for (&name, &id) in ids.lock().unwrap().iter() {
            let count = game
                .seats
                .iter()
                .map(|p| {
                    p.hand
                        .iter()
                        .chain(&p.equipment)
                        .chain(&p.judges)
                        .chain(&p.expansions)
                        .filter(|card| card.id == id)
                        .count()
                })
                .sum::<usize>()
                + game
                    .discard
                    .iter()
                    .chain(&game.resolving)
                    .chain(&game.transferring)
                    .chain(&game.classic.as_ref().unwrap().deck)
                    .filter(|card| card.id == id)
                    .count();
            assert_eq!(count, 1, "{mode:?} {name} conserved");
        }
    }
}

fn combined_replay_fixture(setup: &crate::ReplaySetup) -> Result<Game, crate::RuleError> {
    let mut game = setup.start()?;
    for player in &mut game.seats {
        player.hero = None;
    }
    let ids: Cards = Arc::new(Mutex::new(BTreeMap::new()));
    register(
        &mut game,
        "replay_expand",
        Arc::new(|ctx, _, _| {
            ctx.expand_equip(B, vec![DEFENSE, ATTACK]).unwrap();
            TriggerStep::Done
        }),
    );
    register(
        &mut game,
        "replay_jueying",
        created_horse("jueying", CardKind::Jueying, ids.clone()),
    );
    register(
        &mut game,
        "replay_chitu",
        created_horse("chitu", CardKind::Chitu, ids.clone()),
    );
    register(
        &mut game,
        "replay_dawan",
        created_horse("dawan", CardKind::Dawan, ids),
    );
    Ok(game)
}

#[test]
fn combined_mount_choice_is_private_and_replays_at_pending_selection() {
    use crate::{RecordedGame, ReplayLog, ReplaySetup};
    let seed = (0..100)
        .find(|&seed| {
            let game =
                Game::classic_with_mount_slots(seed, A, "guanyu", 8, MountSlots::Combined).unwrap();
            matches!(game.decision, Some(Decision::Play(A)))
        })
        .unwrap();
    let setup = ReplaySetup::SeededCombinedMounts {
        seed,
        human: A,
        hero: "guanyu".into(),
        seats: 8,
    };
    let mut recorded =
        RecordedGame::with_factory(setup, "combined-mount-choice", combined_replay_fixture)
            .unwrap();
    for skill in [
        "replay_expand",
        "replay_jueying",
        "replay_chitu",
        "replay_dawan",
    ] {
        let view = recorded.game().view(Some(A)).unwrap();
        let registration = view
            .active_plays
            .iter()
            .find(|play| play.skill == skill)
            .unwrap()
            .registration;
        recorded
            .submit(Command {
                version: view.version,
                seat: A,
                action: Action::ActivateSkill {
                    registration,
                    cards: vec![],
                    targets: vec![],
                },
            })
            .unwrap();
    }
    let choice = recorded
        .game()
        .view(Some(B))
        .unwrap()
        .choice
        .expect("B chooses replacement");
    assert_eq!(choice.seat, B);
    assert_eq!(choice.branches[0].cards.len(), 2);
    assert_eq!(
        (choice.branches[0].min_cards, choice.branches[0].max_cards),
        (1, 1)
    );
    assert_eq!(
        recorded.game().event_kind(choice.event),
        Some(EventKind::ChooseButton { seat: B })
    );
    for viewer in (0..8).map(Some).chain([None]) {
        let view = recorded.game().view(viewer).unwrap();
        if viewer == Some(B) {
            assert_eq!(view.choice.as_ref(), Some(&choice));
        } else {
            assert!(
                view.choice.is_none(),
                "choice must be private to B for viewer {viewer:?}"
            );
        }
        if viewer != Some(B) {
            assert!(
                view.seats[B as usize].hand.is_none(),
                "B hand remains private for viewer {viewer:?}"
            );
        }
    }
    let encoded = serde_json::to_vec(recorded.log()).unwrap();
    let log: ReplayLog = serde_json::from_slice(&encoded).unwrap();
    let replay = log
        .restore_with(
            log.commands.len(),
            "combined-mount-choice",
            combined_replay_fixture,
        )
        .unwrap();
    for viewer in (0..8).map(Some).chain([None]) {
        assert_eq!(
            recorded.game().view(viewer).unwrap(),
            replay.view(viewer).unwrap(),
            "replayed pending choice for {viewer:?}"
        );
    }
}
