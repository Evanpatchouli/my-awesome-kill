use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, ActiveSkillRegistration, ChoiceBranch, Command, EventStatus, Phase, TriggerOrder,
    TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::Mutex;

#[test]
fn batch_recast_intercepts_one_of_three_selected_cards() {
    // Reference: tests/reference/recast-batch.json, case "intercept-one".
    let mut game = scene(
        &[
            Role::Lord,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
        ],
        0,
    );
    let shown = card(1, CardKind::Sha);
    let hidden = card(2, CardKind::Tao);
    let equipped = card(3, CardKind::Bagua);
    let control = card(4, CardKind::Shan);
    let selected = [shown, hidden, equipped];
    game.seat_mut(0).hand.extend([shown, hidden, control]);
    game.seat_mut(0).equipment.push(equipped);
    game.known_to_all.extend([shown.id, control.id]);
    let initial_deck = game.classic.as_ref().unwrap().deck.clone();

    let factory_calls = Arc::new(Mutex::new(Vec::new()));
    let loss_calls = factory_calls.clone();
    let gain_calls = factory_calls.clone();
    let rule = RecastBatchRule {
        loss: Arc::new(move |_, seat, cards| {
            assert_eq!(seat, 0);
            loss_calls.lock().unwrap().push(("loss", cards.to_vec()));
            RecastLoss::Discard
        }),
        gain: Arc::new(move |_, seat, cards, movement| {
            assert_eq!(seat, 0);
            assert_eq!(movement.moved, [shown, equipped]);
            gain_calls.lock().unwrap().push(("gain", cards.to_vec()));
            RecastBatchGain::Draw
        }),
    };
    let active = game
        .register_active_skill(ActiveSkillRegistration {
            owner: 0,
            skill: "batch_recast_fixture".into(),
            limited: false,
            choice: Arc::new(move |_, _| {
                Some(ChoiceBranch {
                    id: 0,
                    label: "recast".into(),
                    cards: selected.iter().map(|c| c.id).collect(),
                    min_cards: 3,
                    max_cards: 3,
                    targets: vec![],
                    min_targets: 0,
                    max_targets: 0,
                })
            }),
            effect: Arc::new(move |ctx, stage, selection| {
                assert_eq!(stage, 0);
                ctx.recast_batch(0, &selection.unwrap().cards, rule.clone())
                    .unwrap();
                TriggerStep::Done
            }),
        })
        .unwrap();
    let loss_after = Arc::new(Mutex::new(Vec::new()));
    let observed_loss = loss_after.clone();
    for timing in ["loseBefore", "loseAfter"] {
        let observed_loss = observed_loss.clone();
        game.register_trigger(TriggerRegistration {
            owner: 2,
            skill: format!("batch_intercept_{timing}"),
            timing: timing.into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(move |ctx, _, _| {
                let event = ctx.event();
                let parent_is_discard = event
                    .parent
                    .and_then(|id| ctx.game().scheduler.active_frame(id))
                    .is_some_and(|(_, program)| {
                        matches!(program.kind(), EventKind::LoseToDiscardpile { seat: 0, .. })
                    });
                if parent_is_discard {
                    if timing == "loseBefore" {
                        assert_eq!(event.cards, selected);
                        ctx.give_hand(0, 1, hidden.id).unwrap();
                    } else {
                        let record = ctx
                            .game()
                            .loss_record(event.subject)
                            .expect("completed loss has a record");
                        assert_eq!(record.stockcards, selected);
                        assert_eq!(record.cards.len(), 2);
                        assert!(matches!(record.cards[0], (Zone::Hand, c) if c == shown));
                        assert!(matches!(record.cards[1], (Zone::Equipment, c) if c == equipped));
                        observed_loss.lock().unwrap().push(event.cards.clone());
                    }
                }
                TriggerStep::Done
            }),
        })
        .unwrap();
    }
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.timing == "recastingLose" {
            let view = game.view(Some(2)).unwrap();
            assert!(view.seats[0].known_hand.contains(&shown));
            assert!(view.seats[0].known_hand.contains(&control));
            assert!(!view.seats[0].known_hand.contains(&hidden));
        }
    }));
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    let before = game.view(Some(2)).unwrap();
    assert!(before.seats[0].known_hand.contains(&shown));
    assert!(!before.seats[0].known_hand.contains(&hidden));
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::ActivateSkill {
            registration: active,
            cards: selected.iter().map(|c| c.id).collect(),
            targets: vec![],
        },
    })
    .unwrap();

    assert_eq!(
        *factory_calls.lock().unwrap(),
        [("loss", selected.to_vec()), ("gain", selected.to_vec())]
    );
    assert_eq!(*loss_after.lock().unwrap(), [vec![shown, equipped]]);
    let finished: Vec<_> = game
        .events
        .iter()
        .filter(|e| e.status == EventStatus::Finished)
        .collect();
    assert_eq!(
        finished
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Recast { seat: 0 }))
            .count(),
        1
    );
    assert_eq!(
        finished
            .iter()
            .filter(|e| matches!(
                e.kind,
                EventKind::Draw {
                    seat: 0,
                    amount: 3,
                    ..
                }
            ))
            .count(),
        1
    );
    assert_eq!(
        finished
            .iter()
            .filter(|e| matches!(
                e.kind,
                EventKind::Gain {
                    seat: 0,
                    amount: 3,
                    ..
                }
            ))
            .count(),
        1
    );
    let recast = finished
        .iter()
        .find(|e| matches!(e.kind, EventKind::Recast { seat: 0 }))
        .unwrap();
    let draw = finished
        .iter()
        .find(|e| {
            matches!(
                e.kind,
                EventKind::Draw {
                    seat: 0,
                    amount: 3,
                    ..
                }
            )
        })
        .unwrap();
    let gain = finished
        .iter()
        .find(|e| {
            matches!(
                e.kind,
                EventKind::Gain {
                    seat: 0,
                    amount: 3,
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(draw.parent, Some(recast.id));
    assert_eq!(gain.parent, Some(draw.id));
    assert_eq!(game.discard, [shown, equipped]);
    assert_eq!(game.seat(1).unwrap().hand, [hidden]);
    assert!(game.seat(0).unwrap().equipment.is_empty());
    assert_eq!(game.seat(0).unwrap().hand.len(), 4);
    let spectator = game.view(Some(2)).unwrap();
    assert!(!spectator.seats[1].known_hand.contains(&hidden));
    assert!(game.view(Some(0)).unwrap().seats[1]
        .known_hand
        .contains(&hidden));
    assert!(game.is_known_by(hidden.id, Some(0)));
    assert!(spectator.seats[0].hand.is_none());
    assert!(spectator.seats[1].hand.is_none());
    for c in initial_deck
        .into_iter()
        .chain([shown, hidden, equipped, control])
    {
        let occurrences = game
            .seats
            .iter()
            .map(|s| {
                s.hand
                    .iter()
                    .chain(&s.equipment)
                    .chain(&s.judges)
                    .chain(&s.expansions)
                    .filter(|&&x| x == c)
                    .count()
            })
            .sum::<usize>()
            + game
                .classic
                .as_ref()
                .unwrap()
                .deck
                .iter()
                .filter(|&&x| x == c)
                .count()
            + game
                .discard
                .iter()
                .chain(&game.transferring)
                .chain(&game.resolving)
                .chain(&game.reserved_draws)
                .filter(|&&x| x == c)
                .count();
        assert_eq!(
            occurrences, 1,
            "card {c:?} must occupy exactly one physical zone"
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BatchCase {
    CancelWrapper,
    InterceptAll,
    CancelLose,
    CancelDraw,
    PublicGain,
    CancelGain,
}

#[test]
fn batch_recast_cancellation_interception_and_public_gain_boundaries() {
    for case in [
        BatchCase::CancelWrapper,
        BatchCase::InterceptAll,
        BatchCase::CancelLose,
        BatchCase::CancelDraw,
        BatchCase::PublicGain,
        BatchCase::CancelGain,
    ] {
        let mut game = scene(
            &[
                Role::Lord,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
            ],
            0,
        );
        let shown = card(1, CardKind::Sha);
        let hidden = card(2, CardKind::Tao);
        let equipped = card(3, CardKind::Bagua);
        let control = card(4, CardKind::Shan);
        let selected = [shown, hidden, equipped];
        let ids: Vec<_> = selected.iter().map(|c| c.id).collect();
        game.seat_mut(0).hand.extend([shown, hidden, control]);
        game.seat_mut(0).equipment.push(equipped);
        game.known_to_all.extend([shown.id, control.id]);
        let original_deck = game.classic.as_ref().unwrap().deck.clone();
        let public_cards: Vec<_> = original_deck.iter().rev().take(3).copied().collect();
        let public_ids: Vec<_> = public_cards.iter().map(|c| c.id).collect();
        let steps = Arc::new(Mutex::new(Vec::<String>::new()));
        let loss_steps = steps.clone();
        let gain_steps = steps.clone();
        let rule = RecastBatchRule {
            loss: Arc::new(move |_, _, cards| {
                assert_eq!(cards, selected);
                loss_steps.lock().unwrap().push("factory:loss".into());
                RecastLoss::Discard
            }),
            gain: Arc::new(move |_, _, cards, movement| {
                assert_eq!(cards, selected);
                assert_eq!(
                    movement.moved,
                    if matches!(
                        case,
                        BatchCase::CancelWrapper | BatchCase::InterceptAll | BatchCase::CancelLose
                    ) {
                        vec![]
                    } else {
                        selected.to_vec()
                    }
                );
                gain_steps.lock().unwrap().push("factory:gain".into());
                if case == BatchCase::PublicGain {
                    RecastBatchGain::FromDeck {
                        cards: public_ids.clone(),
                    }
                } else {
                    RecastBatchGain::Draw
                }
            }),
        };
        let active = game
            .register_active_skill(ActiveSkillRegistration {
                owner: 0,
                skill: "batch_boundaries".into(),
                limited: false,
                choice: Arc::new(move |_, _| {
                    Some(ChoiceBranch {
                        id: 0,
                        label: "recast".into(),
                        cards: selected.iter().map(|c| c.id).collect(),
                        min_cards: 3,
                        max_cards: 3,
                        targets: vec![],
                        min_targets: 0,
                        max_targets: 0,
                    })
                }),
                effect: Arc::new(move |ctx, _, selection| {
                    let cards = &selection.unwrap().cards;
                    assert!(ctx
                        .recast_batch(0, &[cards[0], cards[0]], rule.clone())
                        .is_err());
                    assert!(ctx.recast_batch(1, cards, rule.clone()).is_err());
                    ctx.recast_batch(0, cards, rule.clone()).unwrap();
                    assert!(ctx.recast_batch(0, cards, rule.clone()).is_err());
                    TriggerStep::Done
                }),
            })
            .unwrap();
        let requests = steps.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if [
                "recastingLose",
                "recastingGain",
                "loseEnd",
                "loseAfter",
                "drawBefore",
                "gainBefore",
            ]
            .contains(&event.timing.as_str())
            {
                requests
                    .lock()
                    .unwrap()
                    .push(format!("request:{}", event.timing));
                if event.timing == "recastingLose" {
                    let view = game.view(Some(2)).unwrap();
                    assert!(view.seats[0].known_hand.contains(&shown));
                    assert!(view.seats[0].known_hand.contains(&control));
                    assert!(!view.seats[0].known_hand.contains(&hidden));
                }
            }
        }));
        let losses = Arc::new(Mutex::new(Vec::<(Vec<Card>, Vec<(Zone, Card)>)>::new()));
        for timing in [
            "loseToDiscardpileBefore",
            "loseBefore",
            "loseEnd",
            "loseAfter",
            "drawBefore",
            "gainBefore",
            "recastingLose",
            "recastingGain",
        ] {
            let callbacks = steps.clone();
            let losses = losses.clone();
            game.register_trigger(TriggerRegistration {
                owner: 2,
                skill: format!("batch_boundary_{timing}"),
                timing: timing.into(),
                role: TriggerRole::Global,
                order: TriggerOrder::First,
                priority: 0,
                label: None,
                silent: true,
                filter: Arc::new(|_, _, _| true),
                effect: Arc::new(move |ctx, _, _| {
                    let e = ctx.event();
                    let kind = ctx
                        .game()
                        .scheduler
                        .active_frame(e.subject)
                        .unwrap()
                        .1
                        .kind();
                    let parent = e
                        .parent
                        .and_then(|id| ctx.game().scheduler.active_frame(id))
                        .map(|(_, p)| p.kind());
                    let outer_loss = matches!(kind, EventKind::Lose { seat: 0, .. })
                        && matches!(parent, Some(EventKind::LoseToDiscardpile { seat: 0, .. }));
                    callbacks.lock().unwrap().push(format!("callback:{timing}"));
                    if outer_loss && timing == "loseAfter" {
                        let r = ctx
                            .game()
                            .loss_record(e.subject)
                            .expect("completed lose has history");
                        assert_eq!(r.stockcards, selected);
                        assert_eq!(e.cards, r.cards.iter().map(|(_, c)| *c).collect::<Vec<_>>());
                        losses
                            .lock()
                            .unwrap()
                            .push((r.stockcards.clone(), r.cards.clone()));
                    }
                    if case == BatchCase::CancelWrapper && timing == "loseToDiscardpileBefore" {
                        return TriggerStep::CancelSubject;
                    }
                    if case == BatchCase::CancelLose && timing == "loseBefore" && outer_loss {
                        return TriggerStep::CancelSubject;
                    }
                    if case == BatchCase::InterceptAll && timing == "loseBefore" && outer_loss {
                        ctx.give_cards(0, 1, &[shown.id, hidden.id, equipped.id], false)
                            .unwrap();
                    }
                    if case == BatchCase::CancelDraw
                        && timing == "drawBefore"
                        && matches!(kind, EventKind::Draw { seat: 0, .. })
                    {
                        return TriggerStep::CancelSubject;
                    }
                    if case == BatchCase::CancelGain
                        && timing == "gainBefore"
                        && matches!(parent, Some(EventKind::Draw { seat: 0, .. }))
                    {
                        return TriggerStep::CancelSubject;
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
        game.submit(Command {
            version: game.version,
            seat: 0,
            action: Action::ActivateSkill {
                registration: active,
                cards: ids,
                targets: vec![],
            },
        })
        .unwrap();
        let trace = steps.lock().unwrap();
        let index = |name: &str| {
            trace
                .iter()
                .position(|s| s == name)
                .unwrap_or_else(|| panic!("{case:?}: missing {name}; {trace:?}"))
        };
        assert!(index("request:recastingLose") < index("factory:loss"));
        assert!(index("factory:loss") < index("request:recastingGain"));
        assert!(index("request:recastingGain") < index("factory:gain"));
        assert!(index("request:recastingLose") < index("callback:recastingLose"));
        assert!(index("request:recastingGain") < index("callback:recastingGain"));
        if case == BatchCase::InterceptAll {
            assert!(index("request:loseEnd") < index("request:loseAfter"));
            let records = losses.lock().unwrap();
            assert_eq!(records.len(), 1);
            assert!(records[0].1.is_empty());
            let spectator = game.view(Some(2)).unwrap();
            assert!(!spectator.seats[1].known_hand.contains(&shown));
            assert!(!spectator.seats[1].known_hand.contains(&hidden));
            assert!(spectator.seats[1].known_hand.contains(&equipped));
            assert_eq!(game.view(Some(0)).unwrap().seats[1].known_hand, selected);
            let transfer = game
                .loss_history
                .iter()
                .find(|r| r.stockcards == selected && r.cards.len() == 3)
                .expect("transfer has one loss record");
            assert!(matches!(transfer.cards[0], (Zone::Hand, c) if c == shown));
            assert!(matches!(transfer.cards[1], (Zone::Hand, c) if c == hidden));
            assert!(matches!(transfer.cards[2], (Zone::Equipment, c) if c == equipped));
        }
        if case == BatchCase::CancelWrapper {
            assert!(game.loss_history.is_empty());
        }
        if matches!(case, BatchCase::CancelDraw | BatchCase::CancelGain) {
            assert_eq!(
                game.classic.as_ref().unwrap().deck,
                original_deck,
                "{case:?} restores the original deck order"
            );
        }
        if case == BatchCase::PublicGain {
            let spectator = game.view(Some(2)).unwrap();
            for c in &public_cards {
                assert!(spectator.seats[0].known_hand.contains(c));
            }
            assert!(spectator.seats[0].known_hand.contains(&control));
        }
        let recast = game
            .events
            .iter()
            .find(|e| {
                e.status == EventStatus::Finished && matches!(e.kind, EventKind::Recast { seat: 0 })
            })
            .unwrap();
        if case != BatchCase::PublicGain {
            let draw = game
                .events
                .iter()
                .find(|e| {
                    matches!(
                        e.kind,
                        EventKind::Draw {
                            seat: 0,
                            amount: 3,
                            ..
                        }
                    ) && e.status == EventStatus::Started
                })
                .unwrap();
            assert_eq!(draw.parent, Some(recast.id));
            let (draw_status, drawn) = match case {
                BatchCase::CancelDraw => (EventStatus::Cancelled, 0),
                BatchCase::CancelGain => (EventStatus::Finished, 0),
                _ => (EventStatus::Finished, 3),
            };
            assert!(game.events.iter().any(|e| {
                e.id == draw.id
                    && e.status == draw_status
                    && matches!(e.kind, EventKind::Draw { amount, .. } if amount == drawn)
            }));
            if !matches!(case, BatchCase::CancelDraw) {
                let gain = game
                    .events
                    .iter()
                    .find(|e| {
                        matches!(
                            e.kind,
                            EventKind::Gain {
                                seat: 0,
                                amount: 3,
                                ..
                            }
                        ) && e.status == EventStatus::Started
                    })
                    .unwrap();
                assert_eq!(gain.parent, Some(draw.id));
                assert!(game.events.iter().any(|e| {
                    e.id == gain.id
                        && match case {
                            BatchCase::CancelGain => {
                                e.status == EventStatus::Cancelled
                                    && matches!(e.kind, EventKind::Gain { amount: 0, .. })
                            }
                            _ => {
                                e.status == EventStatus::Finished
                                    && matches!(e.kind, EventKind::Gain { amount: 3, .. })
                            }
                        }
                }));
            }
        }
    }
}

#[test]
fn give_cards_handles_cancelled_loss_and_recipient_death_after_loss() {
    for recipient_dies in [false, true] {
        let mut game = scene(
            &[
                Role::Lord,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
            ],
            0,
        );
        let hidden = card(1, CardKind::Tao);
        let equipped = card(2, CardKind::Qinggang);
        let selected = [hidden, equipped];
        game.seat_mut(0).hand.push(hidden);
        game.seat_mut(0).equipment.push(equipped);
        let active = game
            .register_active_skill(ActiveSkillRegistration {
                owner: 0,
                skill: "give_cards_interrupted_recipient".into(),
                limited: false,
                choice: Arc::new(move |_, _| {
                    Some(ChoiceBranch {
                        id: 0,
                        label: "give".into(),
                        cards: selected.iter().map(|c| c.id).collect(),
                        min_cards: 2,
                        max_cards: 2,
                        targets: vec![],
                        min_targets: 0,
                        max_targets: 0,
                    })
                }),
                effect: Arc::new(move |ctx, _, selection| {
                    ctx.give_cards(0, 1, &selection.unwrap().cards, false)
                        .unwrap();
                    TriggerStep::Done
                }),
            })
            .unwrap();
        let seen = Arc::new(Mutex::new(Vec::<(String, Vec<Card>)>::new()));
        let observed = seen.clone();
        game.timing_observer = Some(Arc::new(move |game, e| {
            if [
                "gainBefore",
                "gainBegin",
                "gainEnd",
                "gainAfter",
                "gainCancelled",
            ]
            .contains(&e.timing.as_str())
                && matches!(
                    game.scheduler
                        .active_frame(e.subject)
                        .map(|(_, p)| p.kind()),
                    Some(EventKind::Gain {
                        seat: 1,
                        source: Some(0),
                        ..
                    })
                )
            {
                observed
                    .lock()
                    .unwrap()
                    .push((e.timing.clone(), e.cards.clone()));
            }
        }));
        let loss_subject = Arc::new(Mutex::new(None));
        for timing in ["loseBefore", "loseAfter"] {
            let loss_subject = loss_subject.clone();
            game.register_trigger(TriggerRegistration {
                owner: 2,
                skill: format!("give_interruption_{timing}"),
                timing: timing.into(),
                role: TriggerRole::Global,
                order: TriggerOrder::First,
                priority: 0,
                label: None,
                silent: true,
                filter: Arc::new(|_, _, _| true),
                effect: Arc::new(move |ctx, _, _| {
                    let event = ctx.event();
                    let parent = event
                        .parent
                        .and_then(|id| ctx.game().scheduler.active_frame(id))
                        .map(|(_, p)| p.kind());
                    if matches!(
                        parent,
                        Some(EventKind::Gain {
                            seat: 1,
                            source: Some(0),
                            ..
                        })
                    ) {
                        if timing == "loseBefore" {
                            *loss_subject.lock().unwrap() = Some(event.subject);
                            if !recipient_dies {
                                return TriggerStep::CancelSubject;
                            }
                        } else if recipient_dies {
                            assert_eq!(event.cards, selected);
                            let record = ctx
                                .game()
                                .loss_record(event.subject)
                                .expect("completed loss was recorded");
                            assert_eq!(record.stockcards, selected);
                            assert!(matches!(record.cards[0], (Zone::Hand, c) if c == hidden));
                            assert!(
                                matches!(record.cards[1], (Zone::Equipment, c) if c == equipped)
                            );
                            ctx.die(1, None).unwrap();
                        }
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
        game.submit(Command {
            version: game.version,
            seat: 0,
            action: Action::ActivateSkill {
                registration: active,
                cards: selected.iter().map(|c| c.id).collect(),
                targets: vec![],
            },
        })
        .unwrap();
        let lose_id = loss_subject.lock().unwrap().expect("nested loseBefore ran");
        let gains: Vec<_> = game
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    EventKind::Gain {
                        seat: 1,
                        source: Some(0),
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(gains.len(), 2, "one started and one settled gain");
        assert!(gains.iter().any(|e| e.status == EventStatus::Started
            && matches!(e.kind, EventKind::Gain { amount: 2, .. })));
        let final_gain = gains
            .iter()
            .find(|e| e.status != EventStatus::Started)
            .unwrap();
        assert!(matches!(final_gain.kind, EventKind::Gain { amount: 0, .. }));
        assert!(game.seat(1).unwrap().hand.is_empty());
        if recipient_dies {
            assert!(!game.is_alive(1));
            assert_eq!(final_gain.status, EventStatus::Finished);
            assert_eq!(
                *seen.lock().unwrap(),
                ["gainBefore", "gainBegin", "gainEnd", "gainAfter"]
                    .map(|timing| (timing.to_owned(), selected.to_vec()))
            );
            for c in selected {
                let location = game.card_location(c.id).unwrap();
                assert_eq!(location.zone, Some(crate::CardZone::Special));
                assert_eq!(location.owner, None);
            }
            for viewer in [None, Some(0), Some(1), Some(2)] {
                assert!(!game.is_known_by(hidden.id, viewer));
                assert!(game.is_known_by(equipped.id, viewer));
            }
            assert_eq!(game.special_cards(), selected);
        } else {
            assert!(game.is_alive(1));
            assert!(game.loss_record(lose_id).is_none());
            let hand = game.card_location(hidden.id).unwrap();
            assert_eq!(
                (hand.zone, hand.owner),
                (Some(crate::CardZone::Hand), Some(0))
            );
            let equipment = game.card_location(equipped.id).unwrap();
            assert_eq!(
                (equipment.zone, equipment.owner),
                (Some(crate::CardZone::Equipment), Some(0))
            );
            assert!(game.special_cards().is_empty());
        }
    }
}
