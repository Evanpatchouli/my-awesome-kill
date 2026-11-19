use super::*;
use crate::classic_data::{roster, Suit};
use crate::{CardFace, TriggerContext, TriggerEvent};

fn table() -> Game {
    scene(
        &[
            Role::Lord,
            Role::Loyalist,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
        ],
        0,
    )
}
fn fixture(game: &mut Game, material: Card) -> u64 {
    game.register_active_skill(ActiveSkillRegistration {
        owner: 0,
        skill: "placement_fixture".into(),
        limited: false,
        choice: Arc::new(|_, _| {
            Some(ChoiceBranch {
                id: 0,
                label: "place".into(),
                cards: vec![],
                min_cards: 0,
                max_cards: 0,
                targets: vec![],
                min_targets: 0,
                max_targets: 0,
            })
        }),
        effect: Arc::new(move |ctx, _, _| {
            ctx.add_judge(1, material.id, CardKind::Lebu).unwrap();
            TriggerStep::Done
        }),
    })
    .unwrap()
}
fn play_phase(game: &mut Game) {
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
}
fn observe(game: &Game, event: &TriggerEvent, mode: &str) -> Value {
    let parent = event.parent.and_then(|id| game.event_kind(id));
    let name = match game.event_kind(event.subject).unwrap() {
        EventKind::AddJudge { .. } => "addJudge",
        EventKind::DisableJudge { .. } => "disableJudge",
        _ => panic!("unexpected event"),
    };
    let parent = match parent {
        Some(EventKind::TrickEffect {
            kind: CardKind::Lebu,
            ..
        }) => "lebu",
        Some(EventKind::RegisteredTrigger { .. }) => "reference_judge_interruptions_observer",
        Some(EventKind::ActiveSkillEffect { .. }) | Some(EventKind::ActivateSkill { .. }) => {
            "reference_fixture_use"
        }
        other => panic!("unexpected parent {other:?}"),
    };
    let seat = |s: Option<SeatId>| s.map(|s| ((b'A' + s as u8) as char).to_string());
    let p = game.seat(1).unwrap();
    json!([
        mode,
        event.timing,
        name,
        parent,
        seat(event.player),
        seat(event.source),
        seat(event.target),
        p.judge_disabled,
        usize::from(p.judge_disabled_marker),
        p.judges
            .iter()
            .map(|&c| match game.judgment_kind(c) {
                CardKind::Lebu => "lebu",
                _ => "other",
            })
            .collect::<Vec<_>>()
    ])
}
fn probe_named(
    game: &mut Game,
    timing: &str,
    label: &str,
    effect: impl Fn(&mut TriggerContext<'_>) -> TriggerStep + Send + Sync + 'static,
) {
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: format!("placement_{label}_{timing}"),
        timing: timing.into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |ctx, _, _| effect(ctx)),
    })
    .unwrap();
}
fn probe(
    game: &mut Game,
    timing: &str,
    effect: impl Fn(&mut TriggerContext<'_>) -> TriggerStep + Send + Sync + 'static,
) {
    probe_named(game, timing, "interrupt", effect);
}
fn golden() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../../tests/reference/judge-interruptions.json"
    ))
    .unwrap()
}
fn reference_trial<'a>(g: &'a Value, name: &str) -> &'a Value {
    g["trials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == name)
        .unwrap()
}

fn transfer_row(game: &Game, e: &TriggerEvent, mode: &str, material: Card) -> Option<Value> {
    let subject = game.event_kind(e.subject)?;
    let parent = e.parent.and_then(|id| game.event_kind(id));
    let (event_name, parent_name) = match (subject, parent) {
        (
            EventKind::AddJudge { seat: 1 },
            Some(EventKind::ActiveSkillEffect { .. } | EventKind::ActivateSkill { .. }),
        ) => {
            assert_eq!(e.card_kind, Some(CardKind::Lebu));
            assert_eq!(e.cards, [material]);
            ("addJudge", "reference_fixture_use")
        }
        (EventKind::Lose { .. }, Some(EventKind::AddJudge { seat: 1 })) => ("lose", "addJudge"),
        _ => return None,
    };
    let seat = |s: Option<SeatId>| s.map(|s| ((b'A' + s as u8) as char).to_string());
    Some(json!([
        mode,
        e.timing,
        event_name,
        parent_name,
        seat(e.player),
        seat(e.source),
        seat(e.target)
    ]))
}
#[test]
fn real_card_and_guose_placement_interruptions_match_reference() {
    let g = golden();
    let mut total = 0;
    for form in ["lebu", "guose"] {
        for interruption in [
            "useCardToPlayer",
            "useCardToTarget",
            "useCardToPlayered",
            "useCardToTargeted",
            "addJudgeBefore",
            "addJudgeBegin",
        ] {
            let label = format!("{form}:{interruption}");
            let mut game = table();
            let material = card(
                30,
                if form == "lebu" {
                    CardKind::Lebu
                } else {
                    CardKind::Sha
                },
            );
            game.seat_mut(0).hand.push(material);
            game.seat_mut(0).hero = roster().iter().find(|hero| hero.id == "daqiao");
            game.classic.as_mut().unwrap().faces.push(CardFace {
                id: material.id,
                suit: Suit::Diamond,
                rank: 6,
            });
            let rows = Arc::new(Mutex::new(Vec::new()));
            let log = rows.clone();
            game.timing_observer = Some(Arc::new(move |game, e| {
                if e.timing.starts_with("addJudge") || e.timing.starts_with("disableJudge") {
                    log.lock().unwrap().push(observe(game, e, "request"));
                }
            }));
            for timing in [
                "addJudgeBefore",
                "addJudgeBegin",
                "addJudgeEnd",
                "addJudgeAfter",
                "disableJudgeBefore",
                "disableJudgeBegin",
                "disableJudgeEnd",
                "disableJudgeAfter",
            ] {
                let log = rows.clone();
                let interrupt = timing == interruption;
                probe(&mut game, timing, move |ctx| {
                    log.lock()
                        .unwrap()
                        .push(observe(ctx.game(), ctx.event(), "callback"));
                    if interrupt {
                        ctx.disable_judge(1).unwrap();
                    }
                    TriggerStep::Done
                });
            }
            if interruption.starts_with("useCardTo") {
                probe(&mut game, interruption, |ctx| {
                    assert_eq!(ctx.event().target, Some(1));
                    ctx.disable_judge(1).unwrap();
                    TriggerStep::Done
                });
            }
            play_phase(&mut game);
            let action = if form == "lebu" {
                Action::PlayCard {
                    card: material.id,
                    targets: vec![1],
                }
            } else {
                Action::PlayVirtual {
                    kind: CardKind::Lebu,
                    cards: vec![material.id],
                    targets: vec![1],
                }
            };
            game.submit(Command {
                version: game.version,
                seat: 0,
                action,
            })
            .unwrap();
            crate::classic::tests::pass_counters(&mut game);
            let expected: Vec<_> = g["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| {
                    r[0] == label
                        && (r[2].as_str().unwrap().starts_with("addJudge")
                            || r[2].as_str().unwrap().starts_with("disableJudge"))
                })
                .map(|r| {
                    json!([
                        r[1],
                        r[2],
                        r[3],
                        r[4],
                        r[5],
                        r[6],
                        r[7],
                        r[10]["disabled"],
                        r[10]["placeholder"],
                        r[10]["judges"]
                    ])
                })
                .collect();
            assert_eq!(*rows.lock().unwrap(), expected, "{label}");
            total += expected.len();
            let expected_state = &reference_trial(&g, &label)["state"];
            assert_eq!(game.is_judge_disabled(1), expected_state["disabled"]);
            let placed = !expected_state["judges"].as_array().unwrap().is_empty();
            assert_eq!(
                game.seat(1).unwrap().judges.contains(&material),
                placed,
                "{label}"
            );
            assert_eq!(game.discard.contains(&material), !placed, "{label}");
            if placed {
                assert_eq!(game.judgment_kind(material), CardKind::Lebu);
            }
        }
    }
    assert_eq!(total, 128);
}

#[test]
fn direct_add_and_interrupted_material_movements_keep_identity_and_privacy() {
    let g = golden();
    for form in ["physical", "virtual"] {
        for mode in ["redirect-add", "redirect-lose", "cancel-lose"] {
            let label = format!("transfer:{form}:{mode}");
            let mut game = table();
            let material = card(
                40,
                if form == "physical" {
                    CardKind::Lebu
                } else {
                    CardKind::Sha
                },
            );
            game.seat_mut(0).hand.push(material);
            assert!(!game.is_known_by(material.id, Some(2)));
            let registration = fixture(&mut game, material);
            let interim = Arc::new(Mutex::new(None));
            let moved = interim.clone();
            let rows = Arc::new(Mutex::new(Vec::new()));
            let requests = rows.clone();
            game.timing_observer = Some(Arc::new(move |game, event| {
                if event.timing == "addJudgeBegin" {
                    *moved.lock().unwrap() = game.card_location(material.id);
                }
                if let Some(row) = transfer_row(game, event, "request", material) {
                    requests.lock().unwrap().push(row);
                }
            }));
            for timing in [
                "addJudgeBefore",
                "addJudgeBegin",
                "addJudgeEnd",
                "addJudgeAfter",
                "loseBefore",
                "loseBegin",
                "loseEnd",
                "loseAfter",
                "loseCancelled",
            ] {
                let callbacks = rows.clone();
                probe_named(&mut game, timing, "observer", move |ctx| {
                    if let Some(row) = transfer_row(ctx.game(), ctx.event(), "callback", material) {
                        callbacks.lock().unwrap().push(row);
                    }
                    TriggerStep::Done
                });
            }
            let performed = Arc::new(Mutex::new(false));
            let acted = performed.clone();
            let mode_owned = mode.to_string();
            probe(
                &mut game,
                if mode == "redirect-add" {
                    "addJudgeBefore"
                } else {
                    "loseBefore"
                },
                move |ctx| {
                    let event = ctx.event();
                    let is_placement = matches!(
                        ctx.game().event_kind(event.subject),
                        Some(EventKind::AddJudge { seat: 1 })
                    ) || matches!(
                        event.parent.and_then(|id| ctx.game().event_kind(id)),
                        Some(EventKind::AddJudge { seat: 1 })
                    );
                    if !is_placement || *acted.lock().unwrap() {
                        return TriggerStep::Done;
                    }
                    *acted.lock().unwrap() = true;
                    match mode_owned.as_str() {
                        "redirect-add" => ctx.give_cards(0, 2, &[material.id], false).unwrap(),
                        "redirect-lose" => ctx.discard_hand(0, &[material.id]).unwrap(),
                        "cancel-lose" => return TriggerStep::CancelSubject,
                        _ => unreachable!(),
                    }
                    TriggerStep::Done
                },
            );
            play_phase(&mut game);
            submit(&mut game, registration);
            let expected: Vec<_> = g["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| {
                    r[0] == label && (r[3] == "addJudge" || (r[3] == "lose" && r[4] == "addJudge"))
                })
                .map(|r| json!([r[1], r[2], r[3], r[4], r[5], r[6], r[7]]))
                .collect();
            assert_eq!(*rows.lock().unwrap(), expected, "{label} movement rows");
            if mode == "redirect-add" {
                assert_eq!(
                    *interim.lock().unwrap(),
                    Some(crate::CardLocation {
                        owner: Some(2),
                        zone: Some(crate::CardZone::Hand)
                    })
                );
            }
            let reference = reference_trial(&g, &label);
            assert_eq!(*performed.lock().unwrap(), reference["performed"]);
            assert_eq!(
                game.is_known_by(material.id, Some(2)),
                reference["knownAfter"]
            );
            for seat in 0..8 {
                assert!(
                    game.is_known_by(material.id, Some(seat)),
                    "{label}: seat {seat}"
                );
            }
            let placed = mode != "cancel-lose";
            assert_eq!(
                game.seat(1).unwrap().judges.contains(&material),
                placed,
                "{label}"
            );
            assert_eq!(
                game.seat(0).unwrap().hand.contains(&material),
                !placed,
                "{label}"
            );
            if placed {
                assert_eq!(game.judgment_kind(material), CardKind::Lebu);
                assert_eq!(game.card_location(material.id).unwrap().owner, Some(1));
                let public = game.view(None).unwrap();
                assert_eq!(public.seats[1].judges, [material]);
                if form == "virtual" {
                    assert_eq!(public.seats[1].judge_assignments[0].kind, CardKind::Lebu);
                }
            }
        }
    }
    let mut game = table();
    let material = card(40, CardKind::Lebu);
    game.seat_mut(0).hand.push(material);
    let disable = register(&mut game, true);
    let add = fixture(&mut game, material);
    play_phase(&mut game);
    submit(&mut game, disable);
    assert!(!game.can_add_judge(1, CardKind::Lebu));
    submit(&mut game, add);
    let expected = reference_trial(&g, "direct-disabled");
    assert_eq!(game.can_add_judge(1, CardKind::Lebu), expected["canAdd"]);
    assert!(game.is_judge_disabled(1));
    assert_eq!(game.seat(1).unwrap().judges, [material]);
}

#[test]
fn retained_delayed_tricks_still_resolve_in_disabled_judgment_phase() {
    let g = golden();
    for cancellation in ["discardBefore", "loseBefore"] {
        let label = format!("retained:{cancellation}");
        let mut game = table();
        game.classic.as_mut().unwrap().turn.as_mut().unwrap().seat = 2;
        let delayed = card(41, CardKind::Lebu);
        let reveal = card(42, CardKind::Sha);
        game.seat_mut(1).judges.push(delayed);
        game.classic.as_mut().unwrap().deck.push(reveal);
        game.classic.as_mut().unwrap().faces.push(CardFace {
            id: reveal.id,
            suit: Suit::Heart,
            rank: 9,
        });
        probe(&mut game, "phaseJudgeBefore", |ctx| {
            if ctx.event().player == Some(1) {
                ctx.disable_judge(1).unwrap();
            }
            TriggerStep::Done
        });
        let called = Arc::new(Mutex::new(false));
        let flag = called.clone();
        probe(&mut game, cancellation, move |ctx| {
            if !*flag.lock().unwrap() && ctx.event().player == Some(1) {
                *flag.lock().unwrap() = true;
                return TriggerStep::CancelSubject;
            }
            TriggerStep::Done
        });
        let stage = Arc::new(Mutex::new(None));
        let snapshot = stage.clone();
        probe(&mut game, "phaseJudgeBegin", move |ctx| {
            if ctx.event().player == Some(1) {
                *snapshot.lock().unwrap() = Some((
                    ctx.game().is_judge_disabled(1),
                    ctx.game().seat(1).unwrap().judges.contains(&delayed),
                ));
            }
            TriggerStep::Done
        });
        game.scheduler.start(Program::event(EventKind::Phase {
            seat: 1,
            phase: Phase::Judgment,
        }));
        game.run(None);
        crate::classic::tests::pass_counters(&mut game);
        let trial = reference_trial(&g, &label);
        assert!(*called.lock().unwrap());
        assert_eq!(
            *stage.lock().unwrap(),
            Some((trial["beforeJudge"]["disabled"].as_bool().unwrap(), true))
        );
        assert_eq!(game.is_judge_disabled(1), trial["afterJudge"]["disabled"]);
        assert!(game.seat(1).unwrap().judges.is_empty());
        assert!(game.discard.contains(&delayed));
        assert!(game.discard.contains(&reveal));
    }
}

#[test]
fn nested_enable_and_cancelled_outer_disable_match_reference() {
    let g = golden();
    for interruption in ["disableJudgeBefore", "discard"] {
        let label = format!("outer:{interruption}");
        let mut game = table();
        let delayed = card(41, CardKind::Lebu);
        game.seat_mut(1).judges.push(delayed);
        let disable = register(&mut game, true);
        let called = Arc::new(Mutex::new(false));
        let flag = called.clone();
        let action = interruption.to_string();
        probe(&mut game, interruption, move |ctx| {
            if !*flag.lock().unwrap() && ctx.event().player == Some(1) {
                *flag.lock().unwrap() = true;
                if action == "discard" {
                    ctx.enable_judge(1).unwrap();
                } else {
                    return TriggerStep::CancelSubject;
                }
            }
            TriggerStep::Done
        });
        let states = Arc::new(Mutex::new(Vec::new()));
        let log = states.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if [
                "discard",
                "enableJudgeEnd",
                "discardAfter",
                "disableJudgeEnd",
            ]
            .contains(&event.timing.as_str())
            {
                let p = game.seat(1).unwrap();
                log.lock().unwrap().push(json!([
                    event.timing,
                    p.judge_disabled,
                    usize::from(p.judge_disabled_marker)
                ]));
            }
        }));
        play_phase(&mut game);
        submit(&mut game, disable);
        let trial = reference_trial(&g, &label);
        assert_eq!(*called.lock().unwrap(), trial["performed"]);
        assert_eq!(game.is_judge_disabled(1), trial["state"]["disabled"]);
        assert_eq!(
            usize::from(game.seat(1).unwrap().judge_disabled_marker),
            trial["state"]["placeholder"]
        );
        assert_eq!(
            game.seat(1).unwrap().judges.contains(&delayed),
            interruption == "disableJudgeBefore"
        );
        assert_eq!(game.discard.contains(&delayed), interruption == "discard");
        let expected: Vec<_> = g["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                r[0] == label
                    && r[1] == "request"
                    && [
                        "discard",
                        "enableJudgeEnd",
                        "discardAfter",
                        "disableJudgeEnd",
                    ]
                    .contains(&r[2].as_str().unwrap())
            })
            .map(|r| json!([r[2], r[10]["disabled"], r[10]["placeholder"]]))
            .collect();
        assert_eq!(*states.lock().unwrap(), expected, "{label}");
    }
}

#[test]
fn lightning_returns_to_owner_when_all_other_areas_are_disabled() {
    let g = golden();
    let mut game = table();
    game.classic.as_mut().unwrap().turn.as_mut().unwrap().seat = 2;
    let lightning = card(43, CardKind::Shandian);
    let reveal = card(44, CardKind::Sha);
    game.seat_mut(0).judges.push(lightning);
    for seat in 1..8 {
        game.seat_mut(seat).judge_disabled = true;
        game.seat_mut(seat).judge_disabled_marker = true;
    }
    game.classic.as_mut().unwrap().deck.push(reveal);
    game.classic.as_mut().unwrap().faces.push(CardFace {
        id: reveal.id,
        suit: Suit::Heart,
        rank: 9,
    });
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Judgment,
    }));
    game.run(None);
    crate::classic::tests::pass_counters(&mut game);
    let trial = reference_trial(&g, "lightning-self");
    assert_eq!(trial["zone"], "A:j");
    assert_eq!(game.seat(0).unwrap().judges, [lightning]);
    assert_eq!(game.card_location(lightning.id).unwrap().owner, Some(0));
}
