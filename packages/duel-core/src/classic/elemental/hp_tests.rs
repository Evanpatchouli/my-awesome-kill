use super::*;

fn hp_row(game: &Game, event: &TriggerEvent, trial: &str, observation: &str) -> Value {
    let subject = frame_name(game, event.subject);
    let mut ancestor = Some(event.subject);
    let mut damage = None;
    while let Some(id) = ancestor {
        let (frame, program) = game.scheduler.active_frame(id).unwrap();
        if matches!(program.kind(), EventKind::Damage { .. }) {
            damage = Some(id);
            break;
        }
        ancestor = frame.parent;
    }
    let hit = damage.and_then(|id| game.damage_history().iter().find(|hit| hit.event == id));
    let received = damage
        .map(|id| hit.is_some_and(|hit| game.damage_received(hit.target).any(|h| h.event == id)));
    let dealt = damage.map(|id| game.damage_dealt(0).any(|hit| hit.event == id));
    let amount = match subject.as_str() {
        "link" => None,
        "changeHujia" | "changeHp" => event.delta,
        _ => Some(i32::from(event.amount)),
    };
    let seat = game.seat(event.player.unwrap()).unwrap();
    json!([
        trial,
        observation,
        event.timing,
        subject,
        event.parent.map(|id| frame_name(game, id)),
        label(0, seat.id),
        event.source.map(|id| label(0, id)),
        amount,
        seat.hp,
        seat.armor,
        seat.linked,
        received,
        dealt,
        hit.map(|h| h.absorbed).filter(|&n| n != 0)
    ])
}

#[test]
fn cancelling_the_hp_child_matches_all_reference_requests_and_callbacks() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/hp-cancel.json"
    ))
    .unwrap();
    let mut observed = Vec::new();
    for trial in golden["trials"].as_array().unwrap() {
        let timing = trial["cancelAt"].as_str().unwrap().to_owned();
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
        game.classic.as_mut().unwrap().turn = None;
        // The upstream fixture sets use.baseDamage=2 on the physical Sha.
        game.run(None);
        let mut base = registration(0, "reference_base_damage", "useCard2");
        base.role = TriggerRole::Player;
        base.filter = Arc::new(|_, event, _| event.card_kind == Some(CardKind::Sha));
        base.effect = Arc::new(|ctx, _, _| {
            ctx.set_base_damage(2).unwrap();
            TriggerStep::Done
        });
        game.register_trigger(base).unwrap();
        let sha = card(80, CardKind::Sha);
        game.seat_mut(0).hand.push(sha);
        game.set_sha_nature(sha.id, Some(DamageNature::Fire))
            .unwrap();
        game.seat_mut(1).armor = 1;
        for target in [1, 2] {
            game.seat_mut(target).linked = true;
        }
        let times = [
            "damageBefore",
            "damageBegin4",
            "damage",
            "damageSource",
            "damageEnd",
            "damageAfter",
            "damageCancelled",
            "changeHpBefore",
            "changeHpBegin",
            "changeHp",
            "changeHpEnd",
            "changeHpAfter",
            "changeHpCancelled",
            "changeHujiaBefore",
            "changeHujiaAfter",
            "linkAfter",
            "shaDamage",
            "useCardAfter",
        ];
        let rows = Arc::new(Mutex::new(Vec::new()));
        let log = rows.clone();
        let when = timing.clone();
        let subjects = Mutex::new(BTreeMap::new());
        game.timing_observer = Some(Arc::new(move |game, event| {
            if times.contains(&event.timing.as_str()) {
                if event.timing.starts_with("changeHp") {
                    let mut ids = subjects.lock().unwrap();
                    let id = ids.entry(event.player).or_insert(event.subject);
                    assert_eq!(*id, event.subject);
                }
                log.lock()
                    .unwrap()
                    .push(hp_row(game, event, &when, "request"));
            }
        }));
        for when in times {
            let mut observer = registration(0, "reference_hp_observer", when);
            observer.order = TriggerOrder::First;
            let log = rows.clone();
            let trial = timing.clone();
            observer.effect = Arc::new(move |ctx, _, _| {
                log.lock()
                    .unwrap()
                    .push(hp_row(ctx.game(), ctx.event(), &trial, "callback"));
                TriggerStep::Done
            });
            game.register_trigger(observer).unwrap();
        }
        let mut cancel = registration(1, "reference_hp_cancel", &timing);
        cancel.role = TriggerRole::Player;
        cancel.effect = Arc::new(|_, _, _| TriggerStep::CancelSubject);
        game.register_trigger(cancel).unwrap();
        submit(
            &mut game,
            0,
            Action::PlayCard {
                card: sha.id,
                targets: vec![1],
            },
        );
        answer_effects(&mut game, &[]);
        let final_state: Vec<_> = [1, 2]
            .into_iter()
            .map(|id| {
                let p = game.seat(id).unwrap();
                json!([p.hp, p.armor, p.linked])
            })
            .collect();
        assert_eq!(json!(final_state), trial["final"]);
        assert!(game.discard.contains(&sha));
        assert!(game.damage_history().iter().all(|r| r.cards == [sha]));
        observed.extend(rows.lock().unwrap().iter().cloned());
    }
    let mut expected = golden["events"].as_array().unwrap().clone();
    for row in &mut expected {
        if row[4] == "reference_fixture_use" {
            row[4] = json!("play");
        }
    }
    for index in 0..expected.len().max(observed.len()) {
        assert_eq!(
            observed.get(index),
            expected.get(index),
            "HP observation {index}"
        );
    }
}

#[test]
fn propagation_runs_damage_begin2_without_activating_qilin_for_another_mount() {
    let mut game = prescribed();
    let sha = card(80, CardKind::Sha);
    game.seat_mut(7).hand.push(sha);
    game.set_sha_nature(sha.id, Some(DamageNature::Fire))
        .unwrap();
    game.seat_mut(7).equipment.push(card(90, CardKind::Qilin));
    let horse = card(91, CardKind::Dilu);
    game.seat_mut(10).equipment.push(horse);
    for id in [99, 10] {
        game.seat_mut(id).linked = true;
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let calls = seen.clone();
    let mut hook = registration(7, "observe_both_hits", "damageBegin2");
    hook.effect = Arc::new(move |ctx, _, _| {
        calls.lock().unwrap().push(ctx.event().player.unwrap());
        TriggerStep::Done
    });
    game.register_trigger(hook).unwrap();
    submit(
        &mut game,
        7,
        Action::PlaySha {
            card: sha.id,
            target: 99,
        },
    );
    answer_effects(&mut game, &[]);
    assert_eq!(*seen.lock().unwrap(), [99, 10]);
    assert!(game.seat(10).unwrap().equipment.contains(&horse));
    assert!(!game.events.iter().any(|e| matches!(
        e.kind,
        EventKind::EquipmentEffect {
            kind: CardKind::Qilin,
            ..
        }
    )));
}

#[test]
fn hp_application_rechecks_armor_and_hp_after_before_and_begin_children() {
    let mut game = prescribed();
    let mut before = registration(42, "nested_loss", "changeHpBefore");
    before.filter = Arc::new(|game, event, _| {
        game.scheduler
            .active_frame(event.parent.unwrap())
            .is_some_and(|(_, p)| matches!(p.kind(), EventKind::Damage { .. }))
    });
    before.role = TriggerRole::Player;
    before.effect = Arc::new(|ctx, _, _| {
        ctx.lose_hp(42, 1).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(before).unwrap();
    let mut begin = registration(42, "gain_armor", "changeHpBegin");
    begin.role = TriggerRole::Player;
    begin.filter = Arc::new(|game, event, _| {
        game.scheduler
            .active_frame(event.parent.unwrap())
            .is_some_and(|(_, p)| matches!(p.kind(), EventKind::Damage { .. }))
    });
    begin.effect = Arc::new(|ctx, _, _| {
        ctx.change_armor(42, 1).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(begin).unwrap();
    run_hit(&mut game, None, 42, 2, Some(DamageNature::Fire));
    assert_eq!(
        (game.seat(42).unwrap().hp, game.seat(42).unwrap().armor),
        (2, 0)
    );
    assert_eq!(game.damage_received(42).next().unwrap().absorbed, 1);
    assert!(game
        .events
        .iter()
        .any(|e| e.status == crate::EventStatus::Finished
            && e.kind
                == EventKind::ChangeHp {
                    seat: 42,
                    before: 3,
                    after: 2
                }));
}

#[test]
fn accepted_link_and_turn_over_toggle_the_live_state_after_nested_before() {
    for turned in [false, true] {
        let mut game = prescribed();
        let timing = if turned {
            "turnOverBefore"
        } else {
            "linkBefore"
        };
        let mut outer = registration(7, "outer", "setup");
        outer.effect = Arc::new(move |ctx, _, _| {
            if turned {
                ctx.turn_over(42, Some(true)).unwrap();
                ctx.turn_over(42, Some(false)).unwrap();
            } else {
                ctx.link(42, Some(true)).unwrap();
                ctx.link(42, Some(false)).unwrap();
            }
            TriggerStep::Done
        });
        game.register_trigger(outer).unwrap();
        let once = Mutex::new(false);
        let mut inner = registration(42, "inner", timing);
        inner.role = TriggerRole::Player;
        inner.effect = Arc::new(move |ctx, _, _| {
            let mut done = once.lock().unwrap();
            if !*done {
                *done = true;
                if turned {
                    ctx.turn_over(42, None).unwrap();
                } else {
                    ctx.link(42, None).unwrap();
                }
            }
            TriggerStep::Done
        });
        game.register_trigger(inner).unwrap();
        let event = game.subject_timing("setup", Some(7), None, None, 0, &[]);
        game.scheduler
            .start(Program::Dispatch(game.dispatch_for_timing(event)));
        game.run(None);
        let states: Vec<_> = game
            .events
            .iter()
            .filter(|e| e.status == crate::EventStatus::Finished)
            .filter_map(|e| match e.kind {
                EventKind::Link { linked, .. } if !turned => Some(linked),
                EventKind::TurnOver { turned_over, .. } if turned => Some(turned_over),
                _ => None,
            })
            .collect();
        assert_eq!(states, [true, false]);
        assert!(!game.seat(42).unwrap().linked && !game.seat(42).unwrap().turned_over);
    }
}

#[test]
fn death_revalidates_live_sources_without_rewriting_already_committed_history() {
    for at in ["damageBefore", "damage"] {
        let mut game = scene(&[Role::Lord, Role::Rebel, Role::Rebel], 0);
        let mut kill = registration(0, "kill_source", at);
        kill.filter = Arc::new(|_, event, _| event.source == Some(1) && event.player == Some(2));
        kill.effect = Arc::new(|ctx, _, _| {
            ctx.lose_hp(1, 4).unwrap();
            TriggerStep::Done
        });
        game.register_trigger(kill).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        for timing in [at, "damageSource"] {
            let mut observe = registration(0, "observe_live_source", timing);
            observe.order = TriggerOrder::Last;
            let seen = seen.clone();
            observe.effect = Arc::new(move |ctx, _, _| {
                seen.lock()
                    .unwrap()
                    .push((ctx.event().player.unwrap(), ctx.event().source));
                TriggerStep::Done
            });
            game.register_trigger(observe).unwrap();
        }
        game.seat_mut(2).linked = true;
        game.seat_mut(0).linked = true;
        run_hit(&mut game, Some(1), 2, 1, Some(DamageNature::Fire));
        while let Some(crate::Decision::Cards(choice)) = &game.decision {
            assert_eq!(choice.kind, ChoiceKind::Save);
            let (seat, event) = (choice.seat, choice.event);
            submit(&mut game, seat, Action::Choose { event, card: None });
        }
        assert!(game.match_result().is_none() && !game.seat(1).unwrap().alive);
        assert_eq!(
            *seen.lock().unwrap(),
            [(2, None), (2, None), (0, None), (0, None)]
        );
        let sources: Vec<_> = game
            .damage_history()
            .iter()
            .map(|r| (r.target, r.source))
            .collect();
        assert_eq!(sources, [(2, (at == "damage").then_some(1)), (0, None)]);
    }
}
