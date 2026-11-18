use std::sync::{Arc, Mutex};

use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, CardKind, Command, EventKind, Phase, Program, TriggerOrder, TriggerRegistration,
    TriggerRole, TriggerStep,
};

#[test]
fn physical_sha_matches_reference_trigger_order_and_cancel() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/trigger-order-cancel.json"
    ))
    .unwrap();
    let mut game = scene(
        &[
            Role::Lord,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Renegade,
        ],
        0,
    );
    let sha = card(1, CardKind::Sha);
    let tao = card(2, CardKind::Tao);
    game.seat_mut(0).hand.push(sha);
    game.seat_mut(1).hand.push(tao);
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let low_valid = Arc::new(Mutex::new(true));
    let filter_results = Arc::new(Mutex::new(Vec::<bool>::new()));
    for (name, owner, role, order, priority) in [
        ("first", 0, TriggerRole::Source, TriggerOrder::First, 0),
        ("alpha", 0, TriggerRole::Source, TriggerOrder::Normal, 0),
        ("beta", 0, TriggerRole::Source, TriggerOrder::Normal, 0),
        (
            "invalidated",
            0,
            TriggerRole::Source,
            TriggerOrder::Normal,
            -1,
        ),
        ("target", 1, TriggerRole::Player, TriggerOrder::Normal, 100),
        ("last", 0, TriggerRole::Global, TriggerOrder::Last, 0),
    ] {
        let call_log = calls.clone();
        let validity = low_valid.clone();
        let filter_validity = low_valid.clone();
        let results = filter_results.clone();
        game.register_trigger(TriggerRegistration {
            owner,
            skill: name.into(),
            timing: "damageBegin4".into(),
            role,
            order,
            priority,
            label: Some(
                match name {
                    "alpha" => "Alpha",
                    "beta" => "Beta",
                    _ => name,
                }
                .into(),
            ),
            silent: false,
            filter: Arc::new(move |_, _, _| {
                if name == "invalidated" {
                    let value = *filter_validity.lock().unwrap();
                    results.lock().unwrap().push(value);
                    value
                } else {
                    true
                }
            }),
            effect: Arc::new(move |ctx, stage, _| {
                assert_eq!(
                    (ctx.event().player, ctx.event().source, ctx.event().target),
                    (Some(1), Some(0), None)
                );
                assert_eq!(ctx.event().cards, [sha]);
                if name == "target" && stage == 0 {
                    call_log.lock().unwrap().push("cost".into());
                    ctx.discard_hand(1, &[tao.id]).unwrap();
                    return TriggerStep::Continue(1);
                }
                call_log.lock().unwrap().push(name.into());
                if name == "beta" {
                    *validity.lock().unwrap() = false;
                }
                if name == "last" {
                    TriggerStep::CancelSubject
                } else {
                    TriggerStep::Done
                }
            }),
        })
        .unwrap();
    }
    let hooks = Arc::new(Mutex::new(Vec::<(String, u64)>::new()));
    for timing in [
        "damageBefore",
        "damageBegin",
        "damageBegin1",
        "damageBegin2",
        "damageBegin3",
        "damageBegin4",
        "damage",
        "damageCancelled",
        "damageSource",
        "damageEnd",
        "damageAfter",
        "shaDamage",
        "useCardAfter",
    ] {
        let observed = hooks.clone();
        game.register_trigger(TriggerRegistration {
            owner: 0,
            skill: format!("observe_{timing}"),
            timing: timing.into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 1000,
            label: None,
            silent: true,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(move |ctx, _, _| {
                let event = ctx.event();
                assert!(ctx.game().view(None).unwrap().resolving.contains(&sha));
                observed
                    .lock()
                    .unwrap()
                    .push((event.timing.clone(), event.subject));
                TriggerStep::Done
            }),
        })
        .unwrap();
    }
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::PlayCard {
            card: sha.id,
            targets: vec![1],
        },
    })
    .unwrap();
    let defense = game.view(Some(1)).unwrap().choice.unwrap();
    game.submit(Command {
        version: game.version,
        seat: 1,
        action: Action::Choose {
            event: defense.event,
            card: None,
        },
    })
    .unwrap();
    let private = game.view(Some(0)).unwrap();
    let choice = private
        .choice
        .as_ref()
        .expect("same-priority source triggers require a choice");
    assert!(choice.branches.iter().any(|b| b.label == "Alpha"));
    let beta = choice
        .branches
        .iter()
        .find(|b| b.label == "Beta")
        .unwrap()
        .id;
    assert!(game.view(Some(1)).unwrap().choice.is_none());
    assert!(game.view(None).unwrap().choice.is_none());
    let select = |version, branch| Command {
        version,
        seat: 0,
        action: Action::Select {
            event: choice.event,
            branch,
            cards: vec![],
            targets: vec![],
        },
    };
    assert!(game
        .submit(select(private.version.wrapping_add(1), beta))
        .is_err());
    assert_eq!(game.view(Some(0)).unwrap(), private);
    assert!(game.submit(select(private.version, u32::MAX)).is_err());
    assert_eq!(game.view(Some(0)).unwrap(), private);
    game.submit(select(private.version, beta)).unwrap();
    let expected: Vec<_> = golden["skillSequence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["skill"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(*calls.lock().unwrap(), expected);
    let results = filter_results.lock().unwrap();
    let expected_filters: Vec<_> = golden["filterResults"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_bool().unwrap())
        .collect();
    assert_eq!(*results, expected_filters);
    let hooks = hooks.lock().unwrap();
    let expected_hooks: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| {
            let hook = row[0].as_str().unwrap();
            ([
                "damageBefore",
                "damageBegin",
                "damageBegin1",
                "damageBegin2",
                "damageBegin3",
                "damageBegin4",
                "damage",
                "damageCancelled",
                "damageSource",
                "damageEnd",
                "damageAfter",
                "shaDamage",
                "useCardAfter",
            ]
            .contains(&hook))
            .then_some(hook.to_owned())
        })
        .collect();
    assert_eq!(
        hooks
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>(),
        expected_hooks
    );
    assert!(hooks[..7].iter().all(|(_, subject)| *subject == hooks[0].1));
    let view = game.view(None).unwrap();
    assert_eq!(
        view.seats[1].hp,
        golden["final"]["targetHp"].as_i64().unwrap() as i32
    );
    assert_eq!(view.discard.iter().filter(|c| **c == sha).count(), 1);
    assert_eq!(view.discard.iter().filter(|c| **c == tao).count(), 1);
    assert!(!view.resolving.contains(&sha));
    assert!(game.classic.as_ref().unwrap().sha_played);
}

#[test]
fn ordinary_sha_damage_hooks_match_reference() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/sha-no-shan.json"
    ))
    .unwrap();
    let mut game = scene(&[Role::Lord, Role::Rebel], 0);
    let sha = card(1, CardKind::Sha);
    game.seat_mut(0).hand.push(sha);
    let hooks = Arc::new(Mutex::new(Vec::<(String, u64)>::new()));
    let timings = [
        "damageBefore",
        "damageBegin",
        "damageBegin1",
        "damageBegin2",
        "damageBegin3",
        "damageBegin4",
        "damage",
        "damageSource",
        "damageEnd",
        "damageAfter",
        "damageCancelled",
        "shaDamage",
        "useCardAfter",
    ];
    for timing in timings {
        let observed = hooks.clone();
        game.register_trigger(TriggerRegistration {
            owner: 0,
            skill: format!("observe_{timing}"),
            timing: timing.into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(move |ctx, _, _| {
                let event = ctx.event();
                let fields = match timing {
                    "useCardAfter" => (Some(0), None, None, 0),
                    "shaDamage" => (Some(0), None, Some(1), 0),
                    _ => (Some(1), Some(0), None, 1),
                };
                assert_eq!(
                    (event.player, event.source, event.target, event.amount),
                    fields
                );
                assert_eq!(event.cards, [sha]);
                observed
                    .lock()
                    .unwrap()
                    .push((event.timing.clone(), event.subject));
                TriggerStep::Done
            }),
        })
        .unwrap();
    }
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::PlayCard {
            card: sha.id,
            targets: vec![1],
        },
    })
    .unwrap();
    let defense = game.view(Some(1)).unwrap().choice.unwrap();
    game.submit(Command {
        version: game.version,
        seat: 1,
        action: Action::Choose {
            event: defense.event,
            card: None,
        },
    })
    .unwrap();
    let expected: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| {
            let hook = row[0].as_str().unwrap();
            timings.contains(&hook).then_some(hook.to_owned())
        })
        .collect();
    let hooks = hooks.lock().unwrap();
    assert_eq!(
        hooks
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert!(hooks[..10]
        .iter()
        .all(|(_, subject)| *subject == hooks[0].1));
    assert_eq!(game.seat(1).unwrap().hp, 3);
    assert_eq!(game.discard.iter().filter(|card| **card == sha).count(), 1);
}

#[test]
fn qilin_runs_during_damage_begin2_and_only_after_earlier_candidates() {
    for (cancel_at, order) in [
        ("damageBefore", TriggerOrder::Normal),
        ("damageBegin1", TriggerOrder::Normal),
        ("damageBegin2", TriggerOrder::First),
        ("damageBegin2", TriggerOrder::Normal),
        ("damageBegin3", TriggerOrder::Normal),
    ] {
        let mut game = scene(&[Role::Lord, Role::Rebel], 0);
        let sha = card(1, CardKind::Sha);
        let bow = card(2, CardKind::Qilin);
        let horse = card(3, CardKind::Dilu);
        game.seat_mut(0).hand.push(sha);
        game.seat_mut(0).equipment.push(bow);
        game.seat_mut(1).equipment.push(horse);

        let timings: &[&str] = if cancel_at == "damageBegin3" {
            &["damageBegin2", "damageBegin3"]
        } else {
            &[cancel_at]
        };
        for &timing in timings {
            game.register_trigger(TriggerRegistration {
                owner: 0,
                skill: format!("observe_{timing}"),
                timing: timing.into(),
                role: TriggerRole::Source,
                order,
                priority: 0,
                label: Some("Observe mount".into()),
                silent: false,
                filter: Arc::new(|_, _, _| true),
                effect: Arc::new(move |ctx, _, _| {
                    let has_horse = ctx.game().seat(1).unwrap().equipment.contains(&horse);
                    assert_eq!(has_horse, timing != "damageBegin3");
                    if timing == cancel_at {
                        TriggerStep::CancelSubject
                    } else {
                        TriggerStep::Done
                    }
                }),
            })
            .unwrap();
        }
        game.scheduler.start(Program::event(EventKind::Phase {
            seat: 0,
            phase: Phase::Play,
        }));
        game.run(None);
        game.submit(Command {
            version: game.version,
            seat: 0,
            action: Action::PlayCard {
                card: sha.id,
                targets: vec![1],
            },
        })
        .unwrap();
        let defense = game.view(Some(1)).unwrap().choice.unwrap();
        game.submit(Command {
            version: game.version,
            seat: 1,
            action: Action::Choose {
                event: defense.event,
                card: None,
            },
        })
        .unwrap();

        if cancel_at == "damageBegin3" {
            let choice = game.view(Some(0)).unwrap().choice.unwrap();
            assert_eq!(choice.target, Some(1));
            assert_eq!(choice.branches[1].cards, vec![horse.id]);
            game.submit(Command {
                version: game.version,
                seat: 0,
                action: Action::Select {
                    event: choice.event,
                    branch: 1,
                    cards: vec![horse.id],
                    targets: vec![],
                },
            })
            .unwrap();
            assert!(!game.seat(1).unwrap().equipment.contains(&horse));
            assert_eq!(
                game.discard.iter().filter(|card| **card == horse).count(),
                1
            );
        } else {
            assert!(game.seat(1).unwrap().equipment.contains(&horse));
            assert!(!game.discard.contains(&horse));
        }
        assert!(game.view(Some(0)).unwrap().choice.is_none());
        assert_eq!(game.seat(1).unwrap().hp, 4, "cancelled at {cancel_at}");
        assert_eq!(game.discard.iter().filter(|card| **card == sha).count(), 1);
    }
}
