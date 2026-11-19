use super::*;
use serde_json::{json, Value};

const SKILL: &str = "qinggang2";
const SHA: Card = Card {
    id: 5011,
    kind: CardKind::Sha,
};
const SWORD: Card = Card {
    id: 5012,
    kind: CardKind::Qinggang,
};
const FANG: Card = Card {
    id: 5013,
    kind: CardKind::Fangtian,
};
const ARMOR_B: Card = Card {
    id: 5014,
    kind: CardKind::Bagua,
};
const ARMOR_C: Card = Card {
    id: 5015,
    kind: CardKind::Bagua,
};
const SHAN: Card = Card {
    id: 5016,
    kind: CardKind::Shan,
};
const RED: Card = Card {
    id: 5017,
    kind: CardKind::Sha,
};

fn golden() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tests/reference/qinggang-expiry.json"
    ))
    .unwrap()
}

fn stage<'a>(golden: &'a Value, trial: &str) -> &'a Value {
    &golden["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == trial)
        .unwrap()[1]
}

fn snapshot(game: &Game) -> (Value, Value, Value) {
    let public = game.view(None).unwrap();
    let hp = [1, 2].map(|id| game.seat(id).unwrap().hp);
    let has = [1, 2].map(|id| game.has_skill(id, SKILL));
    let shown = [1usize, 2].map(|id| public.seats[id].marks.iter().any(|m| m.skill == SKILL));
    (json!(hp), json!(has), json!(shown))
}

fn assert_stage(game: &Game, golden: &Value, trial: &str) {
    let expected = stage(golden, trial);
    let (hp, has, shown) = snapshot(game);
    assert_eq!(hp, expected["hp"], "{trial} HP");
    assert_eq!(has, expected["has"], "{trial} effective skill");
    assert_eq!(shown, has, "{trial} public mark");
}

fn register(
    game: &mut Game,
    name: &str,
    timing: &str,
    order: TriggerOrder,
    priority: i32,
    effect: impl Fn(&mut crate::TriggerContext<'_>, u16, Option<&crate::TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) {
    game.register_trigger(TriggerRegistration {
        owner: 3,
        skill: name.into(),
        timing: timing.into(),
        order,
        priority,
        role: TriggerRole::Global,
        silent: true,
        label: None,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(effect),
    })
    .unwrap();
}

type Snapshots = Arc<Mutex<Vec<(String, Value, Value, Value)>>>;

fn observe(game: &mut Game, timing: &str) -> Snapshots {
    let captured = Arc::new(Mutex::new(Vec::new()));
    for (label, priority) in [("callback-high", 13), ("callback-low", 11)] {
        let rows = Arc::clone(&captured);
        register(
            game,
            &format!("inspect_{timing}_{priority}"),
            timing,
            TriggerOrder::First,
            priority,
            move |ctx, _, _| {
                let (hp, has, shown) = snapshot(ctx.game());
                rows.lock()
                    .unwrap()
                    .push((label.to_owned(), hp, has, shown));
                TriggerStep::Done
            },
        );
    }
    captured
}

fn assert_rows(rows: &Snapshots, golden: &Value, trial: &str, timing: &str) {
    let expected: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            r[0] == trial
                && r[2] == timing
                && matches!(r[1].as_str(), Some("callback-high" | "callback-low"))
        })
        .map(|r| {
            (
                r[1].as_str().unwrap().to_owned(),
                r[10]["hp"].clone(),
                r[10]["has"].clone(),
            )
        })
        .collect();
    assert!(
        !expected.is_empty(),
        "missing reference rows for {trial}/{timing}"
    );
    let rows = rows.lock().unwrap();
    let actual: Vec<_> = rows
        .iter()
        .map(|(phase, hp, has, _)| (phase.clone(), hp.clone(), has.clone()))
        .collect();
    assert_eq!(actual, expected, "{trial}/{timing}, in dispatch order");
    for (phase, _, has, shown) in rows.iter() {
        assert_eq!(shown, has, "{trial}/{phase} public mark must follow skill");
    }
}

fn active(
    game: &mut Game,
    name: &str,
    effect: impl Fn(&mut crate::TriggerContext<'_>) + Send + Sync + 'static,
) -> u64 {
    game.register_active_skill(ActiveSkillRegistration {
        owner: 0,
        skill: name.into(),
        limited: false,
        choice: Arc::new(|_, _| {
            Some(ChoiceBranch {
                id: 0,
                label: "setup".into(),
                cards: vec![],
                min_cards: 0,
                max_cards: 0,
                targets: vec![],
                min_targets: 0,
                max_targets: 0,
            })
        }),
        effect: Arc::new(move |ctx, _, _| {
            effect(ctx);
            TriggerStep::Done
        }),
    })
    .unwrap()
}

struct Table {
    game: Game,
    follow: u64,
    material: Vec<Card>,
}

impl Table {
    fn new(multi: bool, give_shan: bool) -> Self {
        let mut game = scene(
            &[
                Role::Lord,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Rebel,
            ],
            0,
        );
        for seat in [1, 2] {
            game.seat_mut(seat).max_hp = 12;
            game.seat_mut(seat).hp = if seat == 2 && !multi { 11 } else { 12 };
        }
        game.seat_mut(0).hand.extend([SHA, SWORD, FANG]);
        game.seat_mut(1).hand.push(ARMOR_B);
        game.mark_known_to(ARMOR_B.id, 0);
        let mut material = vec![SHA, SWORD, FANG, ARMOR_B, RED];
        if multi {
            material.push(ARMOR_C);
            game.seat_mut(2).hand.push(ARMOR_C);
            game.mark_known_to(ARMOR_C.id, 0);
        }
        if give_shan {
            game.seat_mut(1).hand.push(SHAN);
            material.push(SHAN);
        }
        let classic = game.classic.as_mut().unwrap();
        classic.deck.push(RED);
        classic.faces.push(CardFace {
            id: RED.id,
            suit: Suit::Heart,
            rank: 9,
        });
        let setup = active(&mut game, "equip_for_expiry", move |ctx| {
            ctx.expand_equip(0, vec![crate::EquipmentSlot::Weapon])
                .unwrap();
            ctx.equip_from_hand(0, FANG.id).unwrap();
            ctx.equip_from_hand(0, SWORD.id).unwrap();
            ctx.equip_from_hand(1, ARMOR_B.id).unwrap();
            if multi {
                ctx.equip_from_hand(2, ARMOR_C.id).unwrap();
            }
        });
        let follow = active(&mut game, "unarmed_follow", |ctx| {
            ctx.use_virtual_sha(2, 1, None).unwrap();
        });
        play_phase(&mut game);
        multi_weapon_activate(&mut game, setup);
        Self {
            game,
            follow,
            material,
        }
    }

    fn attack(&mut self, targets: &[SeatId]) {
        play(&mut self.game, SHA.id, targets);
    }

    fn check_conservation(&self) {
        multi_weapon_conservation(&self.game, &self.material);
        assert!(self.game.discard.contains(&SHA));
    }

    fn use_bagua(&mut self) {
        let choice = self
            .game
            .view(Some(1))
            .unwrap()
            .choice
            .expect("Bagua response decision");
        assert!(choice
            .branches
            .iter()
            .any(|branch| branch.id == 1 && branch.label.contains("八卦")));
        select(&mut self.game, 1, 1, &[]);
    }
}

#[test]
fn each_multi_target_damage_clears_only_its_mark_between_first_callbacks() {
    let golden = golden();
    let mut table = Table::new(true, false);
    let rows = observe(&mut table.game, "damage");
    table.attack(&[1, 2]);
    for target in [1, 2] {
        defend(&mut table.game, target, None);
    }
    assert_rows(&rows, &golden, "multi", "damage");
    assert_stage(&table.game, &golden, "multi");
    table.check_conservation();
}

#[test]
fn physical_shan_clears_the_mark_before_unarmed_follow_can_offer_bagua() {
    let golden = golden();
    let mut table = Table::new(false, true);
    let rows = observe(&mut table.game, "shaMiss");
    table.attack(&[1]);
    defend(&mut table.game, 1, Some(SHAN.id));
    assert_rows(&rows, &golden, "miss", "shaMiss");
    assert_stage(&table.game, &golden, "miss");
    assert_eq!(stage(&golden, "miss")["cards"]["shan"], "d");
    assert!(table.game.discard.contains(&SHAN));
    // The upstream follow-up uses a physical Sha. Our unarmed content user
    // supplies a material-free Sha; both share the response and judgment rules.
    multi_weapon_activate(&mut table.game, table.follow);
    table.use_bagua();
    assert_stage(&table.game, &golden, "follow");
    assert_eq!(stage(&golden, "follow")["cards"]["red"], "d");
    assert!(table.game.discard.contains(&RED));
    table.check_conservation();
}

fn check_interrupted_damage(trial: &str, timing: &str) {
    let golden = golden();
    let mut table = Table::new(false, false);
    let rows = observe(&mut table.game, timing);
    if trial == "cancel-damage" {
        register(
            &mut table.game,
            "cancel_damage",
            "damageBegin1",
            TriggerOrder::First,
            14,
            |_, _, _| TriggerStep::CancelSubject,
        );
    } else {
        // Content can set the incoming Sha's damage to zero during useCard2.
        // The reference instead sets trigger.num in damageBegin1; both emit damageZero.
        register(
            &mut table.game,
            "zero_damage",
            "useCard2",
            TriggerOrder::First,
            14,
            |ctx, _, _| {
                ctx.set_base_damage(0).unwrap();
                TriggerStep::Done
            },
        );
    }
    table.attack(&[1]);
    defend(&mut table.game, 1, None);
    assert_rows(&rows, &golden, trial, timing);
    assert_stage(&table.game, &golden, trial);
    table.check_conservation();
}

#[test]
fn damage_cancelled_before_commit_clears_its_mark() {
    check_interrupted_damage("cancel-damage", "damageCancelled");
}

#[test]
fn zero_damage_clears_its_mark() {
    check_interrupted_damage("zero-damage", "damageZero");
}

#[test]
fn cancelling_target_before_the_sword_leaves_bagua_available() {
    let golden = golden();
    let mut table = Table::new(false, false);
    register(
        &mut table.game,
        "cancel_target",
        "useCardToPlayered",
        TriggerOrder::First,
        14,
        |_, _, _| TriggerStep::CancelSubject,
    );
    table.attack(&[1]);
    let effects = table
        .game
        .events
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::EquipmentEffect {
                    kind: CardKind::Qinggang,
                    ..
                }
            ) && e.status == EventStatus::Started
        })
        .count();
    let expected = golden["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r[0] == "cancel-target" && r[1] == "qinggang_skill" && r[2] == "begin")
        .count();
    assert_eq!(effects, expected);
    let (_, has, shown) = snapshot(&table.game);
    assert_eq!(has, json!([false, false]));
    assert_eq!(shown, has);
    table.use_bagua();
    assert_stage(&table.game, &golden, "cancel-target");
    assert!(table.game.discard.contains(&RED));
    table.check_conservation();
}

#[test]
fn outer_qinggang_also_blocks_bagua_for_an_unarmed_nested_sha() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/sha-target-equipment.json"
    ))
    .unwrap();
    let mut table = Table::new(false, false);
    register(
        &mut table.game,
        "nested_sha_in_outer_target",
        "useCardToPlayered",
        TriggerOrder::Last,
        0,
        |ctx, _, _| {
            if ctx.event().player == Some(0) {
                ctx.use_virtual_sha(2, 1, None).unwrap();
            }
            TriggerStep::Done
        },
    );
    table.attack(&[1]);
    for source in ["nested-inner", "nested-outer"] {
        let choice = table.game.view(Some(1)).unwrap().choice.unwrap();
        assert_eq!(choice.kind, ChoiceKind::Shan, "{source}: no Bagua");
        let (_, has, shown) = snapshot(&table.game);
        assert_eq!(
            has,
            json!([true, false]),
            "{source}: outer Sha still holds mark"
        );
        assert_eq!(shown, has);
        defend(&mut table.game, 1, None);
    }
    let expected = &golden["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == "nested")
        .unwrap()[1];
    assert_eq!(snapshot(&table.game).0, expected["hp"]);
    assert_eq!(expected["cards"]["nested-red"], "c");
    assert!(table.game.classic.as_ref().unwrap().deck.contains(&RED));
    assert!(!table.game.has_skill(1, SKILL));
    table.check_conservation();
}

#[test]
fn independently_granted_qinggang2_suppresses_bagua_without_a_sword_token() {
    let mut table = Table::new(false, false);
    let grant = active(&mut table.game, "independent_qinggang2", |ctx| {
        ctx.grant_temporary(1, crate::TemporarySkill::until_removed(SKILL))
            .unwrap();
    });
    multi_weapon_activate(&mut table.game, grant);
    assert!(table.game.has_skill(1, SKILL));
    multi_weapon_activate(&mut table.game, table.follow);
    let choice = table.game.view(Some(1)).unwrap().choice.unwrap();
    assert_eq!(
        choice.kind,
        ChoiceKind::Shan,
        "independent qinggang2 suppresses armor even with no sword token"
    );
    table
        .game
        .submit(Command {
            version: table.game.version,
            seat: 1,
            action: Action::Choose {
                event: choice.event,
                card: None,
            },
        })
        .unwrap();
    assert_eq!(table.game.seat(1).unwrap().hp, 11);
    assert!(table.game.classic.as_ref().unwrap().deck.contains(&RED));
    multi_weapon_conservation(&table.game, &table.material);
}

#[test]
fn externally_owned_qinggang2_does_not_accumulate_public_marks() {
    let mut table = Table::new(false, false);
    table
        .game
        .register_active_skill(ActiveSkillRegistration {
            owner: 1,
            skill: SKILL.into(),
            limited: false,
            choice: Arc::new(|_, _| None),
            effect: Arc::new(|_, _, _| TriggerStep::Done),
        })
        .unwrap();
    assert!(table.game.has_skill(1, SKILL));
    let follow = active(&mut table.game, "second_sword_sha", |ctx| {
        ctx.use_virtual_sha(0, 1, None).unwrap();
    });
    table.attack(&[1]);
    defend(&mut table.game, 1, None);
    assert!(!table.game.has_skill(1, SKILL));
    let marks = |game: &Game| {
        game.view(None).unwrap().seats[1]
            .marks
            .iter()
            .filter(|mark| mark.skill == SKILL)
            .map(|mark| mark.count)
            .sum::<u32>()
    };
    let first = marks(&table.game);
    multi_weapon_activate(&mut table.game, follow);
    defend(&mut table.game, 1, None);
    assert!(!table.game.has_skill(1, SKILL));
    assert_eq!(
        marks(&table.game),
        first,
        "a second expired token must not accumulate a public mark"
    );
    assert!(marks(&table.game) <= 1);
    assert_eq!(table.game.seat(1).unwrap().hp, 10);
    table.check_conservation();
}

#[test]
fn internal_cleanup_is_not_shadowed_by_registered_same_name() {
    let mut table = Table::new(false, false);
    let collision = Arc::new(Mutex::new(0));
    let seen = collision.clone();
    table
        .game
        .register_trigger(TriggerRegistration {
            owner: 1,
            skill: "qinggang2_internal_cleanup".into(),
            timing: "damage".into(),
            role: TriggerRole::Player,
            order: TriggerOrder::First,
            priority: 13,
            label: None,
            silent: true,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(move |ctx, _, _| {
                assert!(ctx.game().has_skill(1, SKILL));
                *seen.lock().unwrap() += 1;
                TriggerStep::Done
            }),
        })
        .unwrap();
    let rows = observe(&mut table.game, "damage");
    table.attack(&[1]);
    defend(&mut table.game, 1, None);
    assert_eq!(*collision.lock().unwrap(), 1);
    let rows = rows.lock().unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row.2[0].as_bool().unwrap())
            .collect::<Vec<_>>(),
        [true, false]
    );
    assert!(!table.game.has_skill(1, SKILL));
    table.check_conservation();
}

#[test]
fn resting_target_keeps_sword_mark_until_it_can_return() {
    let mut table = Table::new(false, false);
    let first = std::sync::atomic::AtomicBool::new(true);
    register(
        &mut table.game,
        "rest_during_playered",
        "useCardToPlayered",
        TriggerOrder::Last,
        0,
        move |ctx, _, _| {
            if !first.swap(false, std::sync::atomic::Ordering::SeqCst) {
                return TriggerStep::Done;
            }
            assert!(ctx.game().has_skill(1, SKILL));
            ctx.rest(
                1,
                crate::RestClock {
                    unit: crate::RestUnit::Phase,
                    count: 1,
                },
            )
            .unwrap();
            TriggerStep::Done
        },
    );
    let restore = active(&mut table.game, "restore_resting_target", |ctx| {
        ctx.rest_end(1, None).unwrap();
    });
    table.attack(&[1]);
    assert!(table.game.is_resting(1));
    assert!(
        table.game.has_skill(1, SKILL),
        "out-of-play owner must not run unforced cleanup at useCardEnd"
    );
    multi_weapon_activate(&mut table.game, restore);
    assert!(!table.game.is_resting(1));
    assert!(table.game.has_skill(1, SKILL));
    multi_weapon_activate(&mut table.game, table.follow);
    let choice = table.game.view(Some(1)).unwrap().choice.unwrap();
    assert_eq!(
        choice.kind,
        ChoiceKind::Shan,
        "retained skill suppresses Bagua after return"
    );
    table
        .game
        .submit(Command {
            version: table.game.version,
            seat: 1,
            action: Action::Choose {
                event: choice.event,
                card: None,
            },
        })
        .unwrap();
    table.check_conservation();
}

#[test]
fn existing_max_public_mark_does_not_overflow_when_sword_hits() {
    let mut table = Table::new(false, false);
    table
        .game
        .register_active_skill(ActiveSkillRegistration {
            owner: 1,
            skill: SKILL.into(),
            limited: false,
            choice: Arc::new(|_, _| None),
            effect: Arc::new(|_, _, _| TriggerStep::Done),
        })
        .unwrap();
    let mark = active(&mut table.game, "existing_max_qinggang2_mark", |ctx| {
        ctx.add_skill_mark(1, SKILL, None, u32::MAX).unwrap();
    });
    multi_weapon_activate(&mut table.game, mark);
    assert_eq!(table.game.skill_mark_count(1, SKILL, None), u32::MAX);
    table.attack(&[1]);
    defend(&mut table.game, 1, None);
    assert_eq!(table.game.skill_mark_count(1, SKILL, None), u32::MAX);
    assert!(!table.game.has_skill(1, SKILL));
    table.check_conservation();
}
