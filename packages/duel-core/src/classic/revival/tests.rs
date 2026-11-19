use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::{Role, Suit};
use crate::{
    Action, ActiveSkillRegistration, CardKind, ChoiceBranch, Command, EventId, Phase,
    TemporarySkill, TriggerContext, TriggerEvent, TriggerOrder, TriggerRegistration, TriggerRole,
    TriggerSelection, TriggerStep,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const TIMINGS: &[&str] = &[
    "dieBefore",
    "dieBegin",
    "die",
    "dieEnd",
    "dieAfter",
    "changeHpBefore",
    "changeHpBegin",
    "changeHp",
    "changeHpEnd",
    "changeHpAfter",
    "loseMaxHpBefore",
    "loseMaxHpBegin",
    "loseMaxHpEnd",
    "loseMaxHpAfter",
    "reviveBefore",
    "reviveBegin",
    "reviveEnd",
    "reviveAfter",
    "reviveCancelled",
];
fn names(seats: &[SeatId]) -> Vec<String> {
    seats
        .iter()
        .map(|s| ((b'A' + *s as u8) as char).to_string())
        .collect()
}
fn frame(game: &Game, id: EventId) -> &'static str {
    match game.event_kind(id).unwrap() {
        EventKind::Death { .. } => "die",
        EventKind::ChangeHp { .. } => "changeHp",
        EventKind::LoseMaxHp { .. } => "loseMaxHp",
        EventKind::Revive { .. } => "revive",
        EventKind::ActiveSkillEffect { .. } | EventKind::ActivateSkill { .. } => "fixture",
        other => panic!("unexpected frame {other:?}"),
    }
}
fn row(game: &Game, e: &TriggerEvent, trial: &str, observation: &str) -> Value {
    let kind = game.event_kind(e.subject).unwrap();
    let number = match kind {
        EventKind::ChangeHp { .. } => e.delta,
        EventKind::LoseMaxHp { .. } => game.max_hp_change(e.subject).map(|c| i64::from(c.num)),
        _ => None,
    };
    let event_hp = match kind {
        EventKind::Revive { hp, .. } => Some(hp),
        _ => None,
    };
    let p = game.seat(1).unwrap();
    json!([
        trial,
        observation,
        e.timing,
        frame(game, e.subject),
        e.parent.map(|p| frame(game, p)),
        number,
        event_hp,
        p.hp,
        p.max_hp,
        p.alive,
        names(game.living_seats()),
        names(game.dead_seats()),
        game.revival_cancelled(e.subject).unwrap_or(false)
    ])
}
fn trigger(owner: SeatId, timing: &str) -> TriggerRegistration {
    TriggerRegistration {
        owner,
        skill: format!("probe_{timing}"),
        timing: timing.into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, e, _| e.player == Some(1)),
        effect: Arc::new(|_, _, _| TriggerStep::Done),
    }
}
fn active(
    game: &mut Game,
    name: &str,
    effect: impl Fn(&mut TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) -> u64 {
    game.register_active_skill(ActiveSkillRegistration {
        owner: 0,
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
        effect: Arc::new(effect),
    })
    .unwrap()
}
fn activate(game: &mut Game, registration: u64) {
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::ActivateSkill {
            registration,
            cards: vec![],
            targets: vec![],
        },
    })
    .unwrap();
}
fn roles() -> [Role; 8] {
    [
        Role::Lord,
        Role::Loyalist,
        Role::Loyalist,
        Role::Rebel,
        Role::Loyalist,
        Role::Rebel,
        Role::Loyalist,
        Role::Rebel,
    ]
}
fn stage(game: &Game, expected: &Value) {
    let p = game.seat(1).unwrap();
    assert_eq!(p.role, Some(Role::Loyalist));
    assert_eq!(p.hero.unwrap().id, "zhangfei");
    assert_eq!(expected["identity"], "zhong");
    assert!(p.hand.is_empty() && p.equipment.is_empty());
    for (key, actual) in [
        ("hp", json!(p.hp)),
        ("maxHp", json!(p.max_hp)),
        ("alive", json!(p.alive)),
        ("identityShown", json!(p.role_shown)),
        ("linked", json!(p.linked)),
        ("turnedOver", json!(p.turned_over)),
        (
            "markCount",
            json!(game.skill_mark_count(1, "reference_mark", None)),
        ),
        ("markShown", json!(!game.skill_marks(1).is_empty())),
        ("players", json!(names(game.living_seats()))),
        ("dead", json!(names(game.dead_seats()))),
        ("ring", json!(names(&game.living_seat_ring(0).unwrap()))),
        ("gameOver", json!(game.match_result().is_some())),
    ] {
        assert_eq!(actual, expected[key], "{}: {key}", expected["name"]);
    }
}
#[test]
fn revival_reference_all_requests_callbacks_and_full_death_stages() {
    let reference: Value =
        serde_json::from_str(include_str!("../../../../../tests/reference/revival.json")).unwrap();
    let mut game = scene(&roles(), 0);
    game.seat_mut(1).hero = Some(
        crate::classic_data::roster()
            .iter()
            .find(|g| g.id == "zhangfei")
            .unwrap(),
    );
    let hand = game
        .create_content_card(CardKind::Sha, Suit::Spade, 7, None)
        .unwrap();
    let equip = game
        .create_content_card(CardKind::Zhuge, Suit::Club, 1, None)
        .unwrap();
    game.seat_mut(1).hand.push(hand);
    game.seat_mut(1).equipment.install_test(equip);
    game.seat_mut(1).linked = true;
    game.seat_mut(1).turned_over = true;
    game.add_skill_mark(1, "reference_mark", None, 2).unwrap();
    game.register_active_skill(crate::compare_content::gushe(1))
        .unwrap();
    game.add_skill_mark(1, "gushe", None, 7).unwrap();
    game.grant_temporary(1, TemporarySkill::until_removed("fixture_temp"))
        .unwrap();
    let initial = game.view(None).unwrap();
    assert_eq!(initial.seats[1].role, None);
    assert!(initial.seats[1]
        .marks
        .iter()
        .any(|m| m.skill == "gushe" && m.count == 7));
    let rows = Arc::new(Mutex::new(Vec::new()));
    let trial = Arc::new(Mutex::new(String::new()));
    let r = rows.clone();
    let t = trial.clone();
    game.timing_observer = Some(Arc::new(move |g, e| {
        if e.player == Some(1) && TIMINGS.contains(&e.timing.as_str()) {
            r.lock()
                .unwrap()
                .push(row(g, e, &t.lock().unwrap(), "request"));
        }
    }));
    for &timing in TIMINGS {
        let mut probe = trigger(2, timing);
        let r = rows.clone();
        let t = trial.clone();
        probe.effect = Arc::new(move |ctx, _, _| {
            r.lock()
                .unwrap()
                .push(row(ctx.game(), ctx.event(), &t.lock().unwrap(), "callback"));
            TriggerStep::Done
        });
        game.register_trigger(probe).unwrap();
    }
    let mut cancel = trigger(2, "reviveBefore");
    cancel.skill = "cancel_revival".into();
    cancel.order = TriggerOrder::Last;
    let t = trial.clone();
    cancel.filter =
        Arc::new(move |_, e, _| e.player == Some(1) && *t.lock().unwrap() == "revive-event-cancel");
    cancel.effect = Arc::new(|_, _, _| TriggerStep::CancelSubject);
    game.register_trigger(cancel).unwrap();
    let death = active(&mut game, "death_fixture", |ctx, _, _| {
        ctx.die(1, None).unwrap();
        TriggerStep::Done
    });
    let revive = |g: &mut Game, name: &str, hp: i32| {
        active(g, name, move |ctx, _, _| {
            ctx.revive(1, Some(hp)).unwrap();
            TriggerStep::Done
        })
    };
    let two = revive(&mut game, "revive_two", 2);
    let nine = revive(&mut game, "revive_nine", 9);
    let zero = revive(&mut game, "revive_zero", 0);
    let neg = revive(&mut game, "revive_neg", -2);
    let lose2 = active(&mut game, "lose_two", |ctx, _, _| {
        ctx.lose_max_hp(1, 2, None).unwrap();
        TriggerStep::Done
    });
    let lose1 = active(&mut game, "lose_one", |ctx, _, _| {
        ctx.lose_max_hp(1, 1, None).unwrap();
        TriggerStep::Done
    });
    let event2 = active(&mut game, "event_two", |ctx, _, _| {
        ctx.revive_event(1, Some(2)).unwrap();
        TriggerStep::Done
    });
    let event0 = active(&mut game, "event_zero", |ctx, _, _| {
        ctx.revive_event(1, Some(0)).unwrap();
        TriggerStep::Done
    });
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    let stages = reference["stages"].as_array().unwrap();
    for (index, (name, id)) in [
        ("death-full", death),
        ("revive-two", two),
        ("repeat-over-max", nine),
        ("repeat-zero", zero),
        ("repeat-negative", neg),
        ("death-negative-max", lose2),
        ("revive-negative-max", two),
        ("death-zero-max", lose1),
        ("revive-event-cancel", event2),
        ("revive-event-zero", event0),
    ]
    .into_iter()
    .enumerate()
    {
        if index == 5 {
            activate(&mut game, zero);
            game.seat_mut(1).max_hp = 1;
        }
        *trial.lock().unwrap() = name.into();
        activate(&mut game, id);
        stage(&game, &stages[index]);
        assert_eq!(game.skill_mark_count(1, "gushe", None), 7);
        let public = game.view(None).unwrap();
        assert_eq!(public.seats[1].role, Some(Role::Loyalist));
        assert!(public.seats[1].marks.is_empty());
        for card in [hand, equip] {
            assert_eq!(
                game.discard
                    .iter()
                    .filter(|&&entity| entity == card)
                    .count(),
                1
            );
        }
        assert!(!game.has_skill(1, "fixture_temp"));
    }
    let expected: Vec<_> = reference["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let mut a = v.as_array().unwrap().clone();
            if a[4] == "reference_fixture_use" {
                a[4] = json!("fixture");
            }
            a.remove(12);
            Value::Array(a)
        })
        .collect();
    let actual = rows.lock().unwrap();
    assert_eq!(actual.len(), 78);
    assert_eq!(expected.len(), 78);
    for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(a, e, "reference observation {i}");
    }
}

fn early_snapshot(
    game: &Game,
    name: &str,
    hand: crate::Card,
    equip: crate::Card,
    rewards: &[crate::Card],
) -> Value {
    let p = game.seat(3).unwrap();
    assert_eq!(p.role, Some(Role::Rebel));
    assert_eq!(p.hand, [hand]);
    assert_eq!(p.equipment, [equip]);
    let face = |card: &crate::Card| {
        let printed = game.face(card.id).unwrap();
        json!([card.kind, printed.suit, printed.rank])
    };
    let source = game.seat(0).unwrap();
    json!({
        "name": name,
        "hp": p.hp,
        "maxHp": p.max_hp,
        "alive": p.alive,
        "identity": "fan",
        "identityShown": game.view(None).unwrap().seats[3].role.is_some(),
        "hand": p.hand.iter().map(face).collect::<Vec<_>>(),
        "equips": p.equipment.iter().map(face).collect::<Vec<_>>(),
        "markCount": game.skill_mark_count(3, "gushe", None),
        "markShown": game.skill_marks(3).iter().any(|m| m.skill == "gushe"),
        "tempSkill": game.has_skill(3, "early_temp"),
        "sourceHand": source.hand.len(),
        "sourceKill": game.turn_kills(0).unwrap(),
        "rewardsInHand": rewards.iter().filter(|c| source.hand.contains(c)).map(face).collect::<Vec<_>>(),
        "players": names(game.living_seats()),
        "dead": names(game.dead_seats()),
        "ring": names(&game.living_seat_ring(0).unwrap()),
        "gameOver": game.match_result().is_some(),
    })
}
#[test]
fn revival_during_die_preserves_cards_marks_and_temporary_skill_and_pays_reward() {
    let reference: Value =
        serde_json::from_str(include_str!("../../../../../tests/reference/revival.json")).unwrap();
    let mut game = scene(&roles(), 0);
    game.seat_mut(3).hero = Some(
        crate::classic_data::roster()
            .iter()
            .find(|g| g.id == "zhangfei")
            .unwrap(),
    );
    let hand = game
        .create_content_card(CardKind::Shan, Suit::Diamond, 6, None)
        .unwrap();
    let equip = game
        .create_content_card(CardKind::Qinggang, Suit::Spade, 6, None)
        .unwrap();
    game.seat_mut(3).hand.push(hand);
    game.seat_mut(3).equipment.install_test(equip);
    game.add_skill_mark(3, "gushe", None, 7).unwrap();
    game.grant_temporary(3, TemporarySkill::until_removed("early_temp"))
        .unwrap();
    game.seat_mut(0)
        .hand
        .extend((210..216).map(|id| card(id, CardKind::Sha)));
    let rewards: Vec<_> = (8..11)
        .map(|rank| {
            game.create_content_card(CardKind::Tao, Suit::Heart, rank, None)
                .unwrap()
        })
        .collect();
    for (offset, reward) in rewards.iter().enumerate() {
        let deck = &mut game.classic.as_mut().unwrap().deck;
        let index = deck.len() - 1 - offset;
        deck[index] = *reward;
    }
    let reorder_die = active(&mut game, "reorder_die", |ctx, _, _| {
        ctx.die(1, None).unwrap();
        TriggerStep::Done
    });
    let reorder_revive = active(&mut game, "reorder_revive", |ctx, _, _| {
        ctx.revive(1, Some(2)).unwrap();
        TriggerStep::Done
    });
    let die = active(&mut game, "early_die", |ctx, _, _| {
        ctx.die(3, Some(0)).unwrap();
        TriggerStep::Done
    });
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    activate(&mut game, reorder_die);
    activate(&mut game, reorder_revive);
    let source_before = game.seat(0).unwrap().hand.clone();
    let reward_order = Arc::new(Mutex::new(Vec::<Value>::new()));
    let rows = Arc::new(Mutex::new(vec![early_snapshot(
        &game,
        "before-die",
        hand,
        equip,
        &rewards,
    )]));
    let r = rows.clone();
    let reward_copy = rewards.clone();
    let before_copy = source_before.clone();
    let order_rows = reward_order.clone();
    game.timing_observer = Some(Arc::new(move |g, e| {
        let name = match (e.timing.as_str(), e.player) {
            ("dieBefore", Some(3)) => Some("request-dieBefore"),
            ("dieBegin", Some(3)) => Some("request-dieBegin"),
            ("die", Some(3)) => Some("request-die"),
            ("dieEnd", Some(3)) => Some("request-dieEnd"),
            ("dieAfter", Some(3)) => Some("request-dieAfter"),
            ("drawBefore", Some(0)) => Some("request-drawBefore"),
            ("drawAfter", Some(0)) => Some("request-drawAfter"),
            _ => None,
        };
        if let Some(name) = name {
            r.lock()
                .unwrap()
                .push(early_snapshot(g, name, hand, equip, &reward_copy));
        }
        if e.player == Some(0) && matches!(e.timing.as_str(), "drawBefore" | "drawAfter") {
            let order = g
                .seat(0)
                .unwrap()
                .hand
                .iter()
                .map(|card| {
                    if let Some(index) = reward_copy.iter().position(|reward| reward == card) {
                        format!("bonus{index}")
                    } else if let Some(index) = before_copy.iter().position(|old| old == card) {
                        format!("old{index}")
                    } else {
                        panic!("unexpected card in the reward source hand: {card:?}");
                    }
                })
                .collect::<Vec<_>>();
            order_rows
                .lock()
                .unwrap()
                .push(json!([e.timing.as_str(), order]));
        }
    }));
    let mut revive = trigger(2, "die");
    revive.filter = Arc::new(|_, e, _| e.player == Some(3));
    let r = rows.clone();
    let reward_copy = rewards.clone();
    revive.effect = Arc::new(move |ctx, _, _| {
        r.lock().unwrap().push(early_snapshot(
            ctx.game(),
            "callback-before-revive",
            hand,
            equip,
            &reward_copy,
        ));
        ctx.revive(3, Some(2)).unwrap();
        r.lock().unwrap().push(early_snapshot(
            ctx.game(),
            "callback-after-revive",
            hand,
            equip,
            &reward_copy,
        ));
        TriggerStep::Done
    });
    game.register_trigger(revive).unwrap();
    activate(&mut game, die);
    rows.lock()
        .unwrap()
        .push(early_snapshot(&game, "after-die", hand, equip, &rewards));
    let actual = rows.lock().unwrap();
    let expected = reference["earlyRevival"]["stages"].as_array().unwrap();
    assert_eq!(actual.len(), 11, "observed: {actual:#?}");
    assert_eq!(actual.as_slice(), expected.as_slice());
    drop(actual);
    assert_eq!(
        reward_order.lock().unwrap().as_slice(),
        reference["earlyRevival"]["rewardOrder"]
            .as_array()
            .unwrap()
            .as_slice()
    );
    let expected_hand = rewards
        .iter()
        .chain(&source_before)
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(game.seat(0).unwrap().hand, expected_hand);
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::EndPlay,
    })
    .unwrap();
    assert_eq!(game.current_phase_player(), Some(1));
    assert_eq!(game.turn_kills(0).unwrap(), 0);
}

#[test]
fn revival_real_gushe_stored_mark_survives_full_death() {
    let reference: Value =
        serde_json::from_str(include_str!("../../../../../tests/reference/revival.json")).unwrap();
    let mut game = scene(&roles(), 0);
    game.register_active_skill(crate::compare_content::gushe(1))
        .unwrap();
    let death = active(&mut game, "gushe_death", |ctx, _, _| {
        ctx.die(1, None).unwrap();
        TriggerStep::Done
    });
    let first_revive = active(&mut game, "gushe_first_revive", |ctx, _, _| {
        ctx.revive(1, Some(4)).unwrap();
        TriggerStep::Done
    });
    let revive = active(&mut game, "gushe_revive", |ctx, _, _| {
        ctx.revive(1, Some(2)).unwrap();
        TriggerStep::Done
    });
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    activate(&mut game, death);
    activate(&mut game, first_revive);
    game.add_skill_mark(1, "gushe", None, 7).unwrap();
    let capture = |g: &Game, name: &str| {
        json!({
            "name": name,
            "hp": g.seat(1).unwrap().hp,
            "alive": g.is_alive(1),
            "count": g.skill_mark_count(1, "gushe", None),
            "shown": g.skill_marks(1).iter().any(|m| m.skill == "gushe"),
            "hasSkill": g.has_skill(1, "gushe"),
            "players": names(g.living_seats()),
            "dead": names(g.dead_seats()),
            "ring": names(&g.living_seat_ring(0).unwrap()),
        })
    };
    assert_eq!(
        capture(&game, "before-die"),
        reference["gushe"]["stages"][0]
    );
    activate(&mut game, death);
    assert_eq!(capture(&game, "after-die"), reference["gushe"]["stages"][1]);
    activate(&mut game, revive);
    assert_eq!(
        capture(&game, "after-revive"),
        reference["gushe"]["stages"][2]
    );
}

mod cancellation;
