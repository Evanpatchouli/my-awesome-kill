use super::*;

const REVIVE_TIMINGS: &[&str] = &[
    "reviveBefore",
    "reviveBegin",
    "reviveEnd",
    "reviveAfter",
    "reviveCancelled",
];

fn sample(game: &Game, event: &TriggerEvent, kind: &str) -> Value {
    assert_eq!(event.subject_number(game), None);
    assert!(matches!(
        game.event_kind(event.subject),
        Some(EventKind::Revive { seat: 1, hp: 2 })
    ));
    let p = game.seat(1).unwrap();
    json!([
        kind,
        event.timing,
        p.hp,
        p.alive,
        names(game.living_seats()),
        names(game.dead_seats()),
        game.revival_cancelled(event.subject).unwrap()
    ])
}

#[test]
fn revival_cancellation_matches_all_35_reference_observations() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/reference/revival.json"
    ))
    .unwrap();
    let mut count = 0;
    for trial in golden["cancellationTrials"].as_array().unwrap() {
        let cancel_at = trial["cancelAt"].as_str().unwrap().to_owned();
        let mut game = scene(&roles(), 0);
        let kill_d = active(&mut game, "prepare_die_d", |ctx, _, _| {
            ctx.die(3, None).unwrap();
            TriggerStep::Done
        });
        let restore_d = active(&mut game, "prepare_restore_d", |ctx, _, _| {
            ctx.revive(3, Some(2)).unwrap();
            TriggerStep::Done
        });
        let kill_b = active(&mut game, "prepare_die_b", |ctx, _, _| {
            ctx.die(1, None).unwrap();
            TriggerStep::Done
        });
        let continued = Arc::new(Mutex::new(false));
        let continuation = continued.clone();
        let revive = active(&mut game, "cancellable_revive", move |ctx, step, _| {
            if step == 0 {
                ctx.revive_event(1, Some(2)).unwrap();
                TriggerStep::Continue(1)
            } else {
                *continuation.lock().unwrap() = true;
                TriggerStep::Done
            }
        });
        game.scheduler.start(Program::event(EventKind::Phase {
            seat: 0,
            phase: Phase::Play,
        }));
        game.run(None);
        activate(&mut game, kill_d);
        activate(&mut game, restore_d);
        activate(&mut game, kill_b);
        let rows = Arc::new(Mutex::new(Vec::new()));
        let log = rows.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if event.player == Some(1) && REVIVE_TIMINGS.contains(&event.timing.as_str()) {
                log.lock().unwrap().push(sample(game, event, "request"));
            }
        }));
        for &timing in REVIVE_TIMINGS {
            let mut observer = trigger(2, timing);
            let log = rows.clone();
            observer.effect = Arc::new(move |ctx, _, _| {
                log.lock()
                    .unwrap()
                    .push(sample(ctx.game(), ctx.event(), "callback"));
                TriggerStep::Done
            });
            game.register_trigger(observer).unwrap();
            if timing != "reviveCancelled" {
                for (label, forced) in [("own-ordinary", false), ("own-force-die", true)] {
                    let mut own = trigger(1, timing);
                    own.skill = format!("{label}_{timing}");
                    own.order = TriggerOrder::Normal;
                    let log = rows.clone();
                    own.effect = Arc::new(move |ctx, _, _| {
                        log.lock()
                            .unwrap()
                            .push(sample(ctx.game(), ctx.event(), label));
                        TriggerStep::Done
                    });
                    let id = game.register_trigger(own).unwrap();
                    if forced {
                        game.set_trigger_force_die(id, true).unwrap();
                    }
                }
            }
        }
        let mut cancel = trigger(2, &cancel_at);
        cancel.skill = "cancel_revive".into();
        cancel.priority = -1;
        cancel.effect = Arc::new(|_, _, _| TriggerStep::CancelSubject);
        game.register_trigger(cancel).unwrap();
        activate(&mut game, revive);
        assert!(*continued.lock().unwrap(), "host skill must continue");
        assert_eq!(
            game.events
                .iter()
                .filter(
                    |event| matches!(event.kind, EventKind::Revive { seat: 1, hp: 2 })
                        && event.status == crate::EventStatus::Cancelled
                )
                .count(),
            1,
            "one cancelled revival is recorded"
        );
        let actual = rows.lock().unwrap();
        let expected: Vec<_> = trial["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let mut row = row.as_array().unwrap().clone();
                row.remove(6); // JS finished is not exposed for a live Rust frame.
                Value::Array(row)
            })
            .collect();
        assert_eq!(actual.as_slice(), expected.as_slice(), "{cancel_at}");
        count += actual.len();
        assert_eq!(
            game.seat(1).unwrap().hp,
            trial["hp"].as_i64().unwrap() as i32
        );
        assert_eq!(game.is_alive(1), trial["alive"].as_bool().unwrap());
        assert_eq!(json!(names(game.living_seats())), trial["players"]);
        assert_eq!(json!(names(game.dead_seats())), trial["dead"]);
    }
    assert_eq!(count, 35);
}

#[test]
fn revival_forced_dead_registrations_do_not_offer_order_choice_and_are_revoked() {
    let mut game = scene(&roles(), 0);
    let die = active(&mut game, "kill_b", |ctx, _, _| {
        ctx.die(1, None).unwrap();
        TriggerStep::Done
    });
    let revive = active(&mut game, "restore_b", |ctx, _, _| {
        ctx.revive_event(1, Some(2)).unwrap();
        TriggerStep::Done
    });
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    activate(&mut game, die);
    let calls = Arc::new(Mutex::new(Vec::new()));
    for i in 0..3 {
        let mut probe = trigger(1, "reviveBefore");
        probe.skill = format!("dead_probe_{i}");
        probe.order = TriggerOrder::Normal;
        probe.silent = false;
        probe.label = Some(format!("probe {i}"));
        let log = calls.clone();
        probe.effect = Arc::new(move |_, _, _| {
            log.lock().unwrap().push(i);
            TriggerStep::Done
        });
        let id = game.register_trigger(probe).unwrap();
        if i < 2 {
            game.set_trigger_force_die(id, true).unwrap();
        }
        if i == 1 {
            game.set_trigger_force_die(id, false).unwrap();
            game.set_trigger_force_die(id, true).unwrap();
        }
    }
    let id = game.register_trigger(trigger(1, "reviveBefore")).unwrap();
    game.set_trigger_force_die(id, true).unwrap();
    assert!(game.unregister_trigger(id));
    assert!(game.set_trigger_force_die(id, true).is_err());
    activate(&mut game, revive);
    assert_eq!(*calls.lock().unwrap(), vec![0, 1]);
    assert!(game.is_alive(1));
    assert_eq!(game.revival_cancelled(0), None);
}
