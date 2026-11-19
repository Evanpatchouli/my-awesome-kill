use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, ActiveSkillRegistration, CardKind, ChoiceBranch, Command, EventKind, Game, SeatId,
    TemporarySkill, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::{Arc, Mutex};

fn trigger(game: &mut Game, owner: SeatId, timing: &str, effect: crate::TriggerEffect) -> u64 {
    let skill = format!("reserve_{timing}_{}", game.trigger_registrations.len());
    game.register_trigger(TriggerRegistration {
        owner,
        skill,
        timing: timing.into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect,
    })
    .unwrap()
}
fn submit_death(game: &mut Game, actor: SeatId, target: SeatId, including_out: bool) {
    let registration = game
        .register_active_skill(ActiveSkillRegistration {
            owner: actor,
            skill: "fixture_die".into(),
            limited: false,
            choice: Arc::new(|_, _| {
                Some(ChoiceBranch {
                    id: 0,
                    label: "die".into(),
                    cards: vec![],
                    min_cards: 0,
                    max_cards: 0,
                    targets: vec![],
                    min_targets: 0,
                    max_targets: 0,
                })
            }),
            effect: Arc::new(move |ctx, _, _| {
                if including_out {
                    ctx.die_including_out(target, Some(actor)).unwrap();
                } else {
                    ctx.die(target, Some(actor)).unwrap();
                }
                TriggerStep::Done
            }),
        })
        .unwrap();
    if game.decision.is_none() {
        game.scheduler
            .start(crate::Program::event(EventKind::Phase {
                seat: actor,
                phase: crate::Phase::Play,
            }));
        game.run(None);
    }
    game.submit(Command {
        version: game.version,
        seat: actor,
        action: Action::ActivateSkill {
            registration,
            cards: vec![],
            targets: vec![],
        },
    })
    .unwrap();
}
#[test]
fn reserved_lord_and_last_rebel_do_not_end_or_reveal() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../tests/reference/rest.json")).unwrap();
    let rows: Vec<_> = reference["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row[1] == "mbmowang" && row[0] == "callback")
        .collect();
    assert_eq!(
        rows.iter()
            .take(6)
            .map(|row| row[2].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["dieBefore", "dieBegin", "die", "dieEnd", "dieAfter", "rest"]
    );
    for target in [0, 1] {
        let actor = 1 - target;
        let mut game = scene(&[Role::Lord, Role::Rebel], actor);
        let hand = card(51, CardKind::Sha);
        let equip = card(52, CardKind::Zhuge);
        game.seat_mut(target).hand.push(hand);
        game.seat_mut(target).equipment.push(equip);
        game.seat_mut(target).linked = true;
        game.seat_mut(target).turned_over = true;
        game.add_skill_mark(target, "keep", None, 2).unwrap();
        game.add_skill_mark(target, "hide", None, 1).unwrap();
        game.grant_temporary(target, TemporarySkill::until_removed("temp"))
            .unwrap();
        trigger(
            &mut game,
            target,
            "dieBefore",
            Arc::new(|ctx, _, _| {
                let id = ctx.event().subject;
                assert!(!ctx.game().death_reserved(id));
                ctx.reserve_death(id, &["keep"]).unwrap();
                assert!(ctx.game().death_reserved(id));
                TriggerStep::Done
            }),
        );
        let seen = Arc::new(Mutex::new(Vec::new()));
        for timing in ["die", "dieEnd", "dieAfter"] {
            let seen = seen.clone();
            let id = trigger(
                &mut game,
                target,
                timing,
                Arc::new(move |ctx, _, _| {
                    assert!(ctx.game().death_reserved(ctx.event().subject));
                    seen.lock().unwrap().push(ctx.event().timing.clone());
                    if ctx.event().timing == "dieAfter" {
                        ctx.rest(
                            ctx.event().player.unwrap(),
                            crate::RestClock {
                                unit: crate::RestUnit::Round,
                                count: 1,
                            },
                        )
                        .unwrap();
                    }
                    TriggerStep::Done
                }),
            );
            game.set_trigger_force_die(id, true).unwrap();
        }
        submit_death(&mut game, actor, target, false);
        assert_eq!(*seen.lock().unwrap(), ["die", "dieEnd", "dieAfter"]);
        assert!(game.match_result().is_none());
        assert!(!game.seat(target).unwrap().role_shown);
        assert!(game.is_alive(target) && game.is_out(target));
        assert!(game.living_seats().contains(&target));
        assert!(!game.dead_seats().contains(&target));
        let p = game.seat(target).unwrap();
        assert!(!p.linked && !p.turned_over && p.hp == 0);
        assert!(p.hand.is_empty() && p.equipment.is_empty());
        assert!(game.discard.contains(&hand) && game.discard.contains(&equip));
        assert_eq!(
            game.skill_marks(target)
                .iter()
                .map(|m| m.skill.as_str())
                .collect::<Vec<_>>(),
            vec!["keep"]
        );
        assert!(!game.has_skill(target, "temp"));
        assert!(game.seat(actor).unwrap().hand.is_empty());
        assert_eq!(game.seat(actor).unwrap().turn_kills, 1);
        assert!(game
            .events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::Death { target: t, .. } if t == target))
            .all(|event| matches!(event.kind, EventKind::Death { role: None, .. })));
    }
}
#[test]
fn out_death_attempt_is_cancellable_or_skipped() {
    let mut game = scene(&[Role::Lord, Role::Rebel], 0);
    game.seat_mut(1).out = true;
    let id = trigger(
        &mut game,
        1,
        "dieBefore",
        Arc::new(|_, _, _| TriggerStep::CancelSubject),
    );
    game.set_trigger_force_out(id, true).unwrap();
    submit_death(&mut game, 0, 1, true);
    assert!(game.is_alive(1) && game.is_out(1));
    game.unregister_trigger(id);
    submit_death(&mut game, 0, 1, false);
    assert!(game.is_alive(1) && game.is_out(1));
    assert!(game.match_result().is_none());
}

fn submit_fixture(game: &mut Game, name: &str, effect: crate::TriggerEffect) {
    let registration = game
        .register_active_skill(ActiveSkillRegistration {
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
            effect,
        })
        .unwrap();
    if game.decision.is_none() {
        game.scheduler
            .start(crate::Program::event(EventKind::Phase {
                seat: 0,
                phase: crate::Phase::Play,
            }));
        game.run(None);
    }
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

#[test]
fn death_of_resting_seat_obeys_include_out_and_force_out() {
    for including in [false, true] {
        let mut game = scene(&[Role::Lord, Role::Loyalist, Role::Rebel, Role::Rebel], 0);
        let hand = card(51, CardKind::Sha);
        game.seat_mut(1).hp = if including { 2 } else { 1 };
        game.seat_mut(1).hand.push(hand);
        game.add_skill_mark(1, "other", None, 2).unwrap();
        submit_fixture(
            &mut game,
            "rest_fixture",
            Arc::new(|ctx, _, _| {
                ctx.rest(
                    1,
                    crate::RestClock {
                        unit: crate::RestUnit::Round,
                        count: 2,
                    },
                )
                .unwrap();
                TriggerStep::Done
            }),
        );
        assert!(game.is_out(1) && game.has_skill(1, "undist"));
        let observed = Arc::new(Mutex::new(Vec::new()));
        let unforced = Arc::new(Mutex::new(Vec::new()));
        for timing in ["dieBefore", "dieBegin", "die", "dieEnd", "dieAfter"] {
            let rows = observed.clone();
            let id = trigger(
                &mut game,
                1,
                timing,
                Arc::new(move |ctx, _, _| {
                    let game = ctx.game();
                    rows.lock().unwrap().push((
                        ctx.event().timing.clone(),
                        game.is_alive(1),
                        game.is_out(1),
                        game.view(None).unwrap().seats[1].rest.is_some(),
                        game.rest_clock(1).is_some(),
                        game.has_skill(1, "undist"),
                        !game.skill_marks(1).is_empty(),
                    ));
                    TriggerStep::Done
                }),
            );
            game.set_trigger_force_die(id, true).unwrap();
            game.set_trigger_force_out(id, true).unwrap();
            let rows = unforced.clone();
            let id = trigger(
                &mut game,
                1,
                timing,
                Arc::new(move |ctx, _, _| {
                    rows.lock().unwrap().push(ctx.event().timing.clone());
                    TriggerStep::Done
                }),
            );
            game.set_trigger_force_die(id, true).unwrap();
        }
        submit_death(&mut game, 0, 1, including);
        let actual = observed.lock().unwrap().clone();
        let names: Vec<_> = actual.iter().map(|row| row.0.as_str()).collect();
        let reference: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../tests/reference/rest.json")).unwrap();
        let trial = if including {
            "include-out-die"
        } else {
            "ordinary-die"
        };
        let expected: Vec<_> = reference["outDeath"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row[0] == "callback" && row[1] == trial)
            .collect();
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual.0, expected[2].as_str().unwrap());
            assert_eq!(
                (actual.1, actual.2, actual.3, actual.4, actual.5),
                (
                    expected[5].as_bool().unwrap(),
                    expected[6].as_bool().unwrap(),
                    expected[10].as_bool().unwrap(),
                    !expected[7].is_null(),
                    expected[11].as_bool().unwrap()
                )
            );
        }
        if !including {
            assert_eq!(names, ["dieBefore", "dieBegin", "dieEnd", "dieAfter"]);
            assert!(unforced.lock().unwrap().is_empty());
            assert!(actual
                .iter()
                .all(|row| row.1 && row.2 && row.3 && row.4 && row.5 && row.6));
            assert!(game.seat(1).unwrap().hand.contains(&hand));
        } else {
            assert_eq!(
                names,
                ["dieBefore", "dieBegin", "die", "dieEnd", "dieAfter"]
            );
            assert_eq!(*unforced.lock().unwrap(), ["die", "dieEnd", "dieAfter"]);
            assert_eq!(
                (
                    actual[2].1,
                    actual[2].2,
                    actual[2].3,
                    actual[2].4,
                    actual[2].5,
                    actual[2].6
                ),
                (false, false, true, true, true, true)
            );
            assert_eq!(
                (
                    actual[3].1,
                    actual[3].2,
                    actual[3].3,
                    actual[3].4,
                    actual[3].5,
                    actual[3].6
                ),
                (false, false, false, true, true, false)
            );
            assert!(game.discard.contains(&hand));
            drop(actual);
            submit_fixture(
                &mut game,
                "rest_end_fixture",
                Arc::new(|ctx, _, _| {
                    ctx.rest_end(1, Some(3)).unwrap();
                    TriggerStep::Done
                }),
            );
            assert!(game.is_alive(1) && !game.is_out(1));
            assert_eq!(game.seat(1).unwrap().hp, 3);
            assert!(game.rest_clock(1).is_none() && !game.has_skill(1, "undist"));
        }
    }
}

#[test]
fn excluded_rest_mark_stays_visible_after_included_death() {
    let mut game = scene(&[Role::Lord, Role::Loyalist, Role::Rebel, Role::Rebel], 0);
    submit_fixture(
        &mut game,
        "rest_fixture",
        Arc::new(|ctx, _, _| {
            ctx.rest(
                1,
                crate::RestClock {
                    unit: crate::RestUnit::Round,
                    count: 2,
                },
            )
            .unwrap();
            TriggerStep::Done
        }),
    );
    let id = trigger(
        &mut game,
        1,
        "dieBefore",
        Arc::new(|ctx, _, _| {
            ctx.reserve_death(ctx.event().subject, &["_rest_return"])
                .unwrap();
            TriggerStep::Done
        }),
    );
    game.set_trigger_force_out(id, true).unwrap();
    submit_death(&mut game, 0, 1, true);
    assert!(!game.is_alive(1));
    assert!(game.view(None).unwrap().seats[1].rest.is_some());
    assert!(game.rest_clock(1).is_some());
}
