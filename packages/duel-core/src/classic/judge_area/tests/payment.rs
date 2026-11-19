use super::*;
use crate::classic_data::{roster, Suit};
use crate::{CardFace, CardZone, TriggerEvent};

const TIMINGS: &[&str] = &[
    "useCardBefore",
    "useCardBegin",
    "loseBefore",
    "loseBegin",
    "loseEnd",
    "loseAfter",
    "useCard",
    "useCardToPlayer",
    "useCardToTarget",
    "useCardToPlayered",
    "useCardToTargeted",
    "addJudgeBefore",
    "addJudgeBegin",
    "addJudgeEnd",
    "addJudgeAfter",
    "useCardEnd",
    "useCardAfter",
    "cardsDiscardBefore",
    "cardsDiscardBegin",
    "cardsDiscardEnd",
    "cardsDiscardAfter",
];
fn event_name(kind: EventKind) -> Option<&'static str> {
    match kind {
        EventKind::UseTrick { .. } | EventKind::UseVirtual { .. } => Some("useCard"),
        EventKind::Lose { .. } => Some("lose"),
        EventKind::CardTargetTiming { timing, .. } => Some(timing.name()),
        EventKind::TrickEffect {
            kind: CardKind::Lebu,
            ..
        } => Some("lebu"),
        EventKind::AddJudge { .. } => Some("addJudge"),
        EventKind::CardsDiscard { .. } => Some("cardsDiscard"),
        EventKind::OrderingDiscard => Some("orderingDiscard"),
        EventKind::Phase { .. } => Some("reference_fixture_use"),
        _ => None,
    }
}
fn observation(game: &Game, event: &TriggerEvent, category: &str, material: Card) -> Option<Value> {
    if !TIMINGS.contains(&event.timing.as_str()) {
        return None;
    }
    let kind = event_name(game.event_kind(event.subject)?)?;
    let parent = event
        .parent
        .and_then(|id| game.event_kind(id))
        .and_then(event_name);
    if kind == "lose" && parent != Some("useCard") {
        return None;
    }
    let zone = match game.card_location(material.id) {
        Some(location) => match (location.owner, location.zone) {
            (Some(0), Some(CardZone::Hand)) => "A:h",
            (Some(1), Some(CardZone::Judgment)) => "B:j",
            (_, Some(CardZone::Ordering)) => "o",
            (_, Some(CardZone::DiscardPile)) => "d",
            other => panic!("unexpected material location: {other:?}"),
        },
        None => panic!("missing material {}", material.id),
    };
    let seat = |s: Option<SeatId>| s.map(|s| ((b'A' + s as u8) as char).to_string());
    let p = game.seat(1).unwrap();
    Some(json!([
        category,
        event.timing,
        kind,
        parent,
        seat(event.player),
        seat(event.source),
        seat(event.target),
        p.judge_disabled,
        usize::from(p.judge_disabled_marker),
        p.judges
            .iter()
            .map(|&card| if game.judgment_kind(card) == CardKind::Lebu {
                "lebu"
            } else {
                "other"
            })
            .collect::<Vec<_>>(),
        zone
    ]))
}
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
#[test]
fn payment_target_placement_and_ordering_cleanup_match_real_reference() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/reference/judge-interruptions.json"
    ))
    .unwrap();
    let mut count = 0;
    for form in ["lebu", "guose"] {
        for timing in [
            "useCardToPlayer",
            "useCardToTarget",
            "useCardToPlayered",
            "useCardToTargeted",
            "addJudgeBefore",
            "addJudgeBegin",
        ] {
            let label = format!("{form}:{timing}");
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
            let requests = rows.clone();
            game.timing_observer = Some(Arc::new(move |game, event| {
                if let Some(row) = observation(game, event, "request", material) {
                    requests.lock().unwrap().push(row);
                }
            }));
            for &name in TIMINGS {
                let callbacks = rows.clone();
                let interrupted = timing == name;
                game.register_trigger(TriggerRegistration {
                    owner: 0,
                    skill: format!("payment_reference_{name}"),
                    timing: name.into(),
                    role: TriggerRole::Global,
                    order: TriggerOrder::First,
                    priority: 0,
                    label: None,
                    silent: true,
                    filter: Arc::new(|_, _, _| true),
                    effect: Arc::new(move |ctx, _, _| {
                        if let Some(row) =
                            observation(ctx.game(), ctx.event(), "callback", material)
                        {
                            callbacks.lock().unwrap().push(row);
                        }
                        if interrupted {
                            ctx.disable_judge(1).unwrap();
                        }
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
            let expected: Vec<Value> = golden["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| {
                    r[0] == label
                        && TIMINGS.contains(&r[2].as_str().unwrap())
                        && !(r[3] == "lose" && r[4] != "useCard")
                })
                .map(|r| {
                    let pos = r[10]["positions"]["material"].as_str().unwrap();
                    let normalized = if pos == "j" { "B:j" } else { pos };
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
                        r[10]["judges"],
                        normalized
                    ])
                })
                .collect();
            assert_eq!(*rows.lock().unwrap(), expected, "{label}");
            count += expected.len();
            let placed = timing.starts_with("addJudge");
            assert_eq!(
                game.discard.iter().filter(|&&c| c == material).count(),
                usize::from(!placed)
            );
            assert_eq!(
                game.seat(1)
                    .unwrap()
                    .judges
                    .iter()
                    .filter(|&&c| c == material)
                    .count(),
                usize::from(placed)
            );
            assert!(!game.resolving.contains(&material));
        }
    }
    assert_eq!(count, 408);
}

fn payment_reference() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../../tests/reference/judge-payment.json"
    ))
    .unwrap()
}
fn place_skill(game: &mut Game, material: Card) -> u64 {
    game.register_active_skill(ActiveSkillRegistration {
        owner: 0,
        skill: "external_place_fixture".into(),
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
#[test]
fn add_judge_finds_cards_in_external_and_expansion_zones() {
    let golden = payment_reference();
    for origin in ["discard", "deck", "special", "expansion"] {
        let reference = golden["placements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["origin"] == origin)
            .unwrap();
        let mut game = table();
        let material = card(51, CardKind::Lebu);
        match origin {
            "discard" => game.discard.push(material),
            "deck" => game.classic.as_mut().unwrap().deck.push(material),
            "special" => game.transferring.push(material),
            "expansion" => game.seat_mut(0).expansions.push(material),
            _ => unreachable!(),
        }
        let location = game.card_location(material.id).unwrap();
        let before = match (location.owner, location.zone) {
            (_, Some(CardZone::DiscardPile)) => "d",
            (_, Some(CardZone::DrawPile)) => "c",
            (_, Some(CardZone::Special)) => "s",
            (Some(0), Some(CardZone::Expansion)) => "A:x",
            _ => panic!("unexpected source: {location:?}"),
        };
        assert_eq!(before, reference["before"]["zone"], "{origin}");
        assert_eq!(game.can_add_judge(1, CardKind::Lebu), reference["allowed"]);
        let registration = place_skill(&mut game, material);
        game.scheduler.start(Program::event(EventKind::Phase {
            seat: 0,
            phase: Phase::Play,
        }));
        game.run(None);
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
        assert_eq!(reference["after"]["zone"], "B:j");
        assert_eq!(game.seat(1).unwrap().judges, [material]);
        assert_eq!(game.card_location(material.id).unwrap().owner, Some(1));
        assert_eq!(game.judgment_kind(material), CardKind::Lebu);
    }
}

#[test]
fn ordering_cleanup_cancellation_and_interception_preserve_reference_behavior() {
    let golden = payment_reference();
    for mode in ["cancel", "transfer"] {
        let mut game = table();
        let material = card(60, CardKind::Lebu);
        game.seat_mut(0).hand.push(material);
        game.register_trigger(TriggerRegistration {
            owner: 0,
            skill: "cleanup_disable".into(),
            timing: "useCardToTargeted".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, e, _| e.target == Some(1)),
            effect: Arc::new(|ctx, _, _| {
                ctx.disable_judge(1).unwrap();
                TriggerStep::Done
            }),
        })
        .unwrap();
        let gain_seen = Arc::new(Mutex::new(false));
        let gain = gain_seen.clone();
        game.register_trigger(TriggerRegistration {
            owner: 0,
            skill: "cleanup_gain_observer".into(),
            timing: "gainAfter".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(move |_, e, _| e.player == Some(2) && e.cards.contains(&material)),
            effect: Arc::new(move |ctx, _, _| {
                assert!(ctx.game().seat(2).unwrap().hand.contains(&material));
                *gain.lock().unwrap() = true;
                TriggerStep::Done
            }),
        })
        .unwrap();
        let cleanup_events = Arc::new(Mutex::new(Vec::new()));
        let observed = cleanup_events.clone();
        game.timing_observer = Some(Arc::new(move |_, event| {
            if event.timing.starts_with("cardsDiscard") {
                observed.lock().unwrap().push(event.timing.clone());
            }
        }));
        game.register_trigger(TriggerRegistration {
            owner: 0,
            skill: "cleanup_interruption".into(),
            timing: "cardsDiscardBefore".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(move |_, e, _| e.cards.contains(&material)),
            effect: Arc::new(move |ctx, _, _| {
                if mode == "cancel" {
                    TriggerStep::CancelSubject
                } else {
                    ctx.gain_public_pile_card(2, CardZone::Ordering, material.id)
                        .unwrap();
                    TriggerStep::Done
                }
            }),
        })
        .unwrap();
        game.scheduler.start(Program::event(EventKind::Phase {
            seat: 0,
            phase: Phase::Play,
        }));
        game.run(None);
        game.submit(Command {
            version: game.version,
            seat: 0,
            action: Action::PlayCard {
                card: material.id,
                targets: vec![1],
            },
        })
        .unwrap();
        crate::classic::tests::pass_counters(&mut game);
        let events = cleanup_events.lock().unwrap();
        let label = format!("lebu:block-{mode}");
        let expected: Vec<_> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                row[0] == label
                    && row[1] == "request"
                    && row[3] == "cardsDiscard"
                    && row[4] == "orderingDiscard"
            })
            .map(|row| row[2].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(*events, expected, "{label}");
        if mode == "cancel" {
            assert_eq!(
                game.card_location(material.id).unwrap().zone,
                Some(CardZone::Ordering)
            );
            assert_eq!(*events, ["cardsDiscardBefore", "cardsDiscardCancelled"]);
            assert!(!*gain_seen.lock().unwrap());
        } else {
            assert!(*gain_seen.lock().unwrap());
            assert_eq!(
                *events,
                [
                    "cardsDiscardBefore",
                    "cardsDiscardBegin",
                    "cardsDiscardEnd",
                    "cardsDiscardAfter"
                ]
            );
            assert!(!game.seat(2).unwrap().hand.contains(&material));
            assert_eq!(game.discard.iter().filter(|&&c| c == material).count(), 1);
        }
    }
}

#[test]
fn interrupted_payment_tracks_real_material_and_use_events() {
    let golden = payment_reference();
    let timings = [
        "useCardBefore",
        "useCardBegin",
        "loseBefore",
        "loseBegin",
        "loseEnd",
        "loseAfter",
        "loseCancelled",
        "useCard1",
        "useCard2",
        "useCard",
        "useCardToPlayer",
        "useCardToTarget",
        "useCardToPlayered",
        "useCardToTargeted",
        "addJudgeBefore",
        "addJudgeBegin",
        "addJudgeEnd",
        "addJudgeAfter",
        "useCardEnd",
        "useCardAfter",
        "cardsDiscardBefore",
        "cardsDiscardBegin",
        "cardsDiscardEnd",
        "cardsDiscardAfter",
    ];
    let mut compared = 0;
    for form in ["lebu", "guose"] {
        for mode in ["normal", "cancel", "transfer", "discard", "block"] {
            let label = format!("{form}:{mode}");
            let reference = golden["trials"]
                .as_array()
                .unwrap()
                .iter()
                .find(|trial| trial["name"] == label)
                .unwrap();
            let mut game = table();
            let material = card(
                70,
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
            let performed = Arc::new(Mutex::new(false));
            let rows = Arc::new(Mutex::new(Vec::new()));
            let requests = rows.clone();
            game.timing_observer = Some(Arc::new(move |game, event| {
                if let Some(row) = payment_row(game, event, "request") {
                    requests.lock().unwrap().push(row);
                }
            }));
            for &timing in &timings {
                let callbacks = rows.clone();
                let flag = performed.clone();
                game.register_trigger(TriggerRegistration {
                    owner: 0,
                    skill: format!("payment_interruption_{timing}"),
                    timing: timing.into(),
                    role: TriggerRole::Global,
                    order: TriggerOrder::First,
                    priority: 0,
                    label: None,
                    silent: true,
                    filter: Arc::new(|_, _, _| true),
                    effect: Arc::new(move |ctx, _, _| {
                        if let Some(row) = payment_row(ctx.game(), ctx.event(), "callback") {
                            callbacks.lock().unwrap().push(row);
                        }
                        if !*flag.lock().unwrap()
                            && timing == "loseBefore"
                            && mode != "normal"
                            && mode != "block"
                            && ctx.event().player == Some(0)
                            && ctx
                                .event()
                                .parent
                                .and_then(|id| ctx.game().event_kind(id))
                                .and_then(event_name)
                                == Some("useCard")
                        {
                            *flag.lock().unwrap() = true;
                            match mode {
                                "cancel" => return TriggerStep::CancelSubject,
                                "transfer" => ctx.give_cards(0, 2, &[material.id], false).unwrap(),
                                "discard" => ctx.discard_hand(0, &[material.id]).unwrap(),
                                _ => unreachable!(),
                            }
                        } else if !*flag.lock().unwrap()
                            && mode == "block"
                            && timing == "useCardToTargeted"
                            && ctx.event().target == Some(1)
                        {
                            *flag.lock().unwrap() = true;
                            ctx.disable_judge(1).unwrap();
                        }
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
            let expected: Vec<_> = golden["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| {
                    r[0] == label
                        && timings.contains(&r[2].as_str().unwrap())
                        && !(r[3] == "lose" && r[4] != "useCard")
                })
                .map(|r| json!([r[1], r[2], r[3], r[4], r[5], r[6], r[7]]))
                .collect();
            assert_eq!(*rows.lock().unwrap(), expected, "{label}");
            compared += expected.len();
            assert_eq!(*performed.lock().unwrap(), reference["handled"]);
            assert!(game
                .events
                .iter()
                .any(|event| event.status == EventStatus::Finished
                    && match event.kind {
                        EventKind::UseTrick { card, .. } => card == material,
                        EventKind::UseVirtual {
                            seat: 0,
                            kind: CardKind::Lebu,
                        } => form == "guose",
                        _ => false,
                    }));
            let location = game.card_location(material.id).unwrap();
            let zone = match (location.owner, location.zone) {
                (Some(0), Some(CardZone::Hand)) => "A:h",
                (Some(2), Some(CardZone::Hand)) => "C:h",
                (Some(1), Some(CardZone::Judgment)) => "B:j",
                (_, Some(CardZone::DiscardPile)) => "d",
                other => panic!("{label} unexpected location: {other:?}"),
            };
            let expected_zone = reference["after"]["zone"].as_str().unwrap();
            assert_eq!(
                zone,
                if expected_zone == "j" {
                    "B:j"
                } else {
                    expected_zone
                },
                "{label}"
            );
            assert_eq!(game.is_judge_disabled(1), reference["after"]["disabled"]);
            assert_eq!(
                game.seat(1).unwrap().judges.contains(&material),
                !reference["after"]["judges"].as_array().unwrap().is_empty()
            );
            assert_eq!(
                game.is_known_by(material.id, Some(2)),
                reference["after"]["knownC"],
                "{label}"
            );
        }
    }
    assert_eq!(compared, 324);
}
fn payment_row(game: &Game, event: &TriggerEvent, category: &str) -> Option<Value> {
    let kind = event_name(game.event_kind(event.subject)?)?;
    if kind == "reference_fixture_use" {
        return None;
    }
    let parent = event
        .parent
        .and_then(|id| game.event_kind(id))
        .and_then(event_name);
    if kind == "lose" && parent != Some("useCard") {
        return None;
    }
    let seat = |s: Option<SeatId>| s.map(|s| ((b'A' + s as u8) as char).to_string());
    Some(json!([
        category,
        event.timing,
        kind,
        parent,
        seat(event.player),
        seat(event.source),
        seat(event.target)
    ]))
}

#[test]
fn direct_add_to_dead_target_leaves_material_with_owner() {
    let golden = payment_reference();
    let expected = &golden["dead"];
    let mut game = table();
    let material = card(52, CardKind::Lebu);
    game.seat_mut(1).alive = false;
    game.seat_mut(0).hand.push(material);
    assert_eq!(game.is_alive(1), expected["alive"]);
    assert_eq!(game.is_out(1), expected["out"]);
    assert_eq!(game.is_in(1), expected["in"]);
    assert_eq!(game.can_add_judge(1, CardKind::Lebu), expected["canAdd"]);
    let registration = place_skill(&mut game, material);
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
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
    assert_eq!(expected["after"]["zone"], "A:h");
    assert_eq!(game.seat(0).unwrap().hand, [material]);
    assert!(game.seat(1).unwrap().judges.is_empty());
}
