use super::*;

fn material(game: &mut Game, id: u32, tag: &'static str) -> Card {
    let card = card(id, CardKind::Tao);
    game.seat_mut(1).hand.push(card);
    let setup = setup_active(game, tag, move |ctx| {
        ctx.add_to_expansion(1, card.id, tag).unwrap();
    });
    activate_setup(game, setup);
    assert_eq!(game.expansions(1, tag), [card]);
    card
}

fn cleanup(
    game: &mut Game,
    name: &'static str,
    tag: &'static str,
    expiry: Option<&str>,
    trace: Arc<Mutex<Vec<&'static str>>>,
) {
    let expires = expiry
        .map(|timing| vec![crate::SkillExpiry::new(TriggerRole::Global, timing)])
        .unwrap_or_default();
    let mut skill = crate::TemporarySkill::new(name, expires);
    skill.on_remove = Some(Arc::new(move |ctx| {
        trace.lock().unwrap().push(name);
        ctx.discard_expansions(tag);
    }));
    game.grant_temporary(1, skill).unwrap();
}

fn copies(game: &Game, card: Card) -> usize {
    game.seats
        .iter()
        .map(|p| {
            p.hand
                .iter()
                .chain(&p.equipment)
                .chain(&p.judges)
                .chain(&p.expansions)
                .filter(|&&c| c == card)
                .count()
        })
        .sum::<usize>()
        + game
            .discard
            .iter()
            .chain(&game.resolving)
            .chain(&game.transferring)
            .filter(|&&c| c == card)
            .count()
}

#[test]
fn queued_gain_precedes_synchronous_expiry_cleanup_for_emit_and_remove() {
    for emit in [false, true] {
        let (mut game, _) = table();
        let gained = material(&mut game, 7201, "fifo-gain");
        let discarded = material(&mut game, 7202, "fifo-cleanup");
        let trace = Arc::new(Mutex::new(Vec::new()));
        cleanup(
            &mut game,
            "fifo-remove",
            "fifo-cleanup",
            emit.then_some("fifoExpiry"),
            trace.clone(),
        );
        let callback = trace.clone();
        register(
            &mut game,
            "useCardToPlayered",
            TriggerOrder::First,
            move |ctx| {
                if ctx.event().target == Some(1) {
                    ctx.gain_expansion(1, 2, gained.id).unwrap();
                    if emit {
                        assert!(ctx.emit("fifoExpiry", None, None, None, 0, &[]));
                    } else {
                        assert!(ctx.remove_temporary(1, "fifo-remove"));
                    }
                    assert!(!ctx.game().has_skill(1, "fifo-remove"));
                    assert_eq!(ctx.game().expansions(1, "fifo-gain"), [gained]);
                    assert_eq!(ctx.game().expansions(1, "fifo-cleanup"), [discarded]);
                    callback.lock().unwrap().push("callback");
                }
                TriggerStep::Done
            },
        );
        play(&mut game, &[1]);
        assert_eq!(*trace.lock().unwrap(), ["fifo-remove", "callback"]);
        let gain = game
            .events
            .iter()
            .position(|e| {
                matches!(e.kind, EventKind::Gain { seat: 2, .. })
                    && e.status == EventStatus::Started
            })
            .unwrap();
        let discard = game
            .events
            .iter()
            .position(|e| {
                matches!(e.kind, EventKind::LoseToDiscardpile { seat: 1, .. })
                    && e.status == EventStatus::Started
            })
            .unwrap();
        assert!(gain < discard);
        assert!(game.seat(2).unwrap().hand.contains(&gained));
        assert_eq!(game.discard.iter().filter(|&&c| c == discarded).count(), 1);
        assert_eq!((copies(&game, gained), copies(&game, discarded)), (1, 1));
    }
}

#[test]
fn failed_required_operation_skips_normal_work_but_drains_removal_cleanup() {
    for replan in [false, true] {
        let (mut game, _) = table();
        let first = material(&mut game, 7211, "fifo-first");
        let last = material(&mut game, 7212, "fifo-last");
        let trace = Arc::new(Mutex::new(Vec::new()));
        if replan {
            cleanup(
                &mut game,
                "fifo-first-remove",
                "fifo-first",
                Some("fifoFirstExpiry"),
                trace.clone(),
            );
        } else {
            register(&mut game, "gainBefore", TriggerOrder::First, |_| {
                TriggerStep::CancelSubject
            });
        }
        cleanup(
            &mut game,
            "fifo-last-remove",
            "fifo-last",
            None,
            trace.clone(),
        );
        let later = trace.clone();
        register(&mut game, "fifoLater", TriggerOrder::First, move |_| {
            later.lock().unwrap().push("later");
            TriggerStep::Done
        });
        let callback = trace.clone();
        register(
            &mut game,
            "useCardToPlayered",
            TriggerOrder::First,
            move |ctx| {
                if ctx.event().target == Some(1) {
                    if replan {
                        assert!(ctx.emit("fifoFirstExpiry", None, None, None, 0, &[]));
                    }
                    ctx.gain_expansion(1, 2, first.id).unwrap();
                    assert!(ctx.emit("fifoLater", None, None, None, 0, &[]));
                    assert!(ctx.remove_temporary(1, "fifo-last-remove"));
                    assert!(!ctx.game().has_skill(1, "fifo-last-remove"));
                    assert_eq!(ctx.game().expansions(1, "fifo-first"), [first]);
                    assert_eq!(ctx.game().expansions(1, "fifo-last"), [last]);
                    callback.lock().unwrap().push("callback");
                }
                TriggerStep::Done
            },
        );
        play(&mut game, &[1]);
        let expected = if replan {
            vec!["fifo-first-remove", "fifo-last-remove", "callback"]
        } else {
            vec!["fifo-last-remove", "callback"]
        };
        assert_eq!(*trace.lock().unwrap(), expected);
        assert_eq!(game.discard.iter().filter(|&&c| c == last).count(), 1);
        assert!(!game.seat(2).unwrap().hand.contains(&first));
        if replan {
            assert_eq!(game.discard.iter().filter(|&&c| c == first).count(), 1);
            assert!(
                game.discard.iter().position(|&c| c == first).unwrap()
                    < game.discard.iter().position(|&c| c == last).unwrap()
            );
            assert!(!game
                .events
                .iter()
                .any(|e| matches!(e.kind, EventKind::Gain { seat: 2, .. })
                    && e.status == EventStatus::Started));
        } else {
            assert_eq!(game.expansions(1, "fifo-first"), [first]);
            assert!(game
                .events
                .iter()
                .any(|e| matches!(e.kind, EventKind::Gain { seat: 2, .. })
                    && e.status == EventStatus::Cancelled));
            let cancelled = game
                .events
                .iter()
                .position(|e| {
                    matches!(e.kind, EventKind::Gain { seat: 2, .. })
                        && e.status == EventStatus::Cancelled
                })
                .unwrap();
            let cleanup = game
                .events
                .iter()
                .position(|e| {
                    matches!(e.kind, EventKind::LoseToDiscardpile { seat: 1, .. })
                        && e.status == EventStatus::Started
                })
                .unwrap();
            assert!(cancelled < cleanup);
        }
        assert_eq!((copies(&game, first), copies(&game, last)), (1, 1));
        let cleanup_count = game
            .events
            .iter()
            .filter(|e| {
                matches!(e.kind, EventKind::LoseToDiscardpile { seat: 1, .. })
                    && e.status == EventStatus::Started
            })
            .count();
        assert_eq!(cleanup_count, if replan { 2 } else { 1 });
    }
}

#[test]
fn gain_first_keeps_empty_cleanup_wrapper_timings_and_requested_material() {
    let (mut game, _) = table();
    let gained = material(&mut game, 7221, "fifo-wrapper");
    let trace = Arc::new(Mutex::new(Vec::new()));
    cleanup(
        &mut game,
        "fifo-wrapper-expiry",
        "fifo-wrapper",
        Some("useCardCancelled"),
        Arc::new(Mutex::new(Vec::new())),
    );

    // The fixed upstream gain-first trace observes the original stockcards on
    // the outer event even though the material is already in C's hand.
    let observed = trace.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if matches!(
            event.timing.as_str(),
            "loseToDiscardpileBefore" | "loseToDiscardpileEnd" | "loseToDiscardpileAfter"
        ) {
            assert_eq!(event.cards, [gained]);
            assert!(game.seat(2).unwrap().hand.contains(&gained));
            observed
                .lock()
                .unwrap()
                .push((event.timing.clone(), event.subject));
        }
    }));
    register(
        &mut game,
        "useCardToPlayered",
        TriggerOrder::First,
        move |ctx| {
            if ctx.event().target == Some(1) {
                ctx.gain_expansion(1, 2, gained.id).unwrap();
                ctx.cancel_card_use().unwrap();
            }
            TriggerStep::Done
        },
    );
    play(&mut game, &[1]);
    let rows = trace.lock().unwrap();
    assert_eq!(
        rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
        [
            "loseToDiscardpileBefore",
            "loseToDiscardpileEnd",
            "loseToDiscardpileAfter"
        ]
    );
    assert!(rows.iter().all(|row| row.1 == rows[0].1));
    let wrapper = rows[0].1;
    assert!(
        !game
            .events
            .iter()
            .any(|event| event.parent == Some(wrapper)
                && matches!(event.kind, EventKind::Lose { .. }))
    );
    assert!(game.seat(2).unwrap().hand.contains(&gained));
    assert_eq!(copies(&game, gained), 1);
}
