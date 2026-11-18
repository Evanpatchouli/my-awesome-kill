use crate::classic::tests::scene;
use crate::classic_data::{Role, Suit};
use crate::*;
use std::sync::{Arc, Mutex};

fn table() -> Game {
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
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    game
}

fn queue_choice(
    game: &mut Game,
    request: MoveChoiceRequest,
    results: Arc<Mutex<Vec<MoveResult>>>,
) -> ActiveSkillId {
    game.register_active_skill(ActiveSkillRegistration {
        owner: 0,
        skill: "arrange_test".into(),
        limited: false,
        choice: Arc::new(|_, _| {
            Some(ChoiceBranch {
                id: 1,
                label: "分组".into(),
                cards: vec![],
                min_cards: 0,
                max_cards: 0,
                targets: vec![],
                min_targets: 0,
                max_targets: 0,
            })
        }),
        effect: Arc::new(move |ctx, stage, _| match stage {
            0 => TriggerStep::MoveCards {
                request: request.clone(),
                then: 1,
            },
            1 => {
                results
                    .lock()
                    .unwrap()
                    .push(ctx.move_result().unwrap().clone());
                TriggerStep::Done
            }
            _ => unreachable!(),
        }),
    })
    .unwrap()
}

fn start_choice(game: &mut Game, registration: ActiveSkillId) {
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

fn observe(game: &mut Game) -> Arc<Mutex<Vec<String>>> {
    let timings = Arc::new(Mutex::new(Vec::new()));
    let observer = timings.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.timing.starts_with("chooseToMove") {
            assert!(matches!(
                event.parent.and_then(|id| game.event_kind(id)),
                Some(EventKind::ActiveSkillEffect { .. })
            ));
            assert!(event.cards.is_empty());
            observer.lock().unwrap().push(event.timing.clone());
        }
    }));
    timings
}

#[test]
fn validates_each_step_and_final_layout_before_consuming_a_private_choice() {
    let mut game = table();
    let cards: Vec<_> = (1..=4)
        .map(|rank| {
            game.create_content_card(CardKind::Sha, Suit::Spade, rank, None)
                .unwrap()
        })
        .collect();
    game.classic.as_mut().unwrap().deck.extend(&cards);
    let ids: Vec<_> = cards.iter().map(|card| card.id).collect();
    let forbidden = cards[3].id;
    let final_first = cards[1];
    let results = Arc::new(Mutex::new(Vec::new()));
    let id = queue_choice(
        &mut game,
        MoveChoiceRequest {
            seat: 0,
            prompt: "分组".into(),
            rows: vec![
                MoveRow {
                    label: "上".into(),
                    cards: cards.clone(),
                    min: 2,
                    max: 4,
                },
                MoveRow {
                    label: "下".into(),
                    cards: vec![],
                    min: 2,
                    max: 2,
                },
            ],
            allow_decline: false,
            filter_move: Some(Arc::new(move |_, _, step| {
                step.row != 1 || step.card != forbidden
            })),
            filter_ok: Some(Arc::new(move |_, rows| {
                rows[1].first() == Some(&final_first)
            })),
            bot: None,
        },
        results.clone(),
    );
    let timings = observe(&mut game);
    start_choice(&mut game, id);
    let private = game.view(Some(0)).unwrap();
    let choice = private.choice.as_ref().unwrap();
    assert_eq!(choice.kind, ChoiceKind::Content);
    assert_eq!(choice.move_cards.as_ref().unwrap().rows[0].cards, ids);
    assert_eq!(private.private_choice_cards, cards);
    for viewer in [Some(1), Some(2), None] {
        let view = game.view(viewer).unwrap();
        assert_eq!(view.waiting_for, Some(0));
        assert!(view.choice.is_none());
        assert!(view.private_choice_cards.is_empty());
        assert!(view.card_faces.iter().all(|face| !ids.contains(&face.id)));
    }
    let step = |i: usize, row, index| MoveStep {
        card: ids[i],
        row,
        index,
    };
    let correct = vec![step(0, 1, 0), step(1, 1, 0), step(2, 0, 1)];
    let failures = [
        (0, choice.event, None),
        (0, choice.event, Some(vec![step(0, 1, 0)])),
        (0, choice.event, Some(vec![step(3, 1, 0), step(1, 1, 0)])),
        (0, choice.event, Some(vec![step(1, 1, 0), step(0, 1, 0)])),
        (
            0,
            choice.event,
            Some(vec![
                step(0, 1, 0),
                step(1, 1, 0),
                step(2, 1, 2),
                step(2, 0, 0),
            ]),
        ),
        (0, choice.event, Some(vec![step(0, 2, 0)])),
        (0, choice.event, Some(vec![step(0, 1, 1)])),
        (
            0,
            choice.event,
            Some(vec![MoveStep {
                card: 50000,
                row: 1,
                index: 0,
            }]),
        ),
        (1, choice.event, Some(correct.clone())),
        (0, choice.event + 100, Some(correct.clone())),
    ];
    for (seat, event, moves) in failures {
        assert!(game
            .submit(Command {
                version: private.version,
                seat,
                action: Action::MoveCards { event, moves }
            })
            .is_err());
        assert_eq!(game.view(Some(0)).unwrap(), private);
    }
    let deck = game.classic.as_ref().unwrap().deck.clone();
    game.submit(Command {
        version: private.version,
        seat: 0,
        action: Action::MoveCards {
            event: choice.event,
            moves: Some(correct),
        },
    })
    .unwrap();
    assert_eq!(
        *results.lock().unwrap(),
        [MoveResult {
            event: choice.event,
            rows: Some(vec![vec![cards[3], cards[2]], vec![cards[1], cards[0]]]),
            cancelled: false,
        }]
    );
    assert_eq!(game.classic.as_ref().unwrap().deck, deck);
    assert_eq!(
        *timings.lock().unwrap(),
        [
            "chooseToMoveBefore",
            "chooseToMoveBegin",
            "chooseToMoveEnd",
            "chooseToMoveAfter"
        ]
    );
}

#[test]
fn declining_and_cancelling_have_distinct_results_and_the_bot_uses_checked_moves() {
    for case in [
        "decline",
        "cancel",
        "bot",
        "auto",
        "bot-decline",
        "impossible",
    ] {
        let mut game = table();
        let automated = matches!(case, "bot" | "auto" | "bot-decline" | "impossible");
        let expected_cancel = matches!(case, "cancel" | "impossible");
        let should_move = matches!(case, "bot" | "auto");
        if automated {
            game.classic.as_mut().unwrap().human = 1;
        }
        let card = game.classic.as_ref().unwrap().deck[0];
        let results = Arc::new(Mutex::new(Vec::new()));
        let timings = observe(&mut game);
        let id = queue_choice(
            &mut game,
            MoveChoiceRequest {
                seat: 0,
                prompt: "分组".into(),
                rows: vec![
                    MoveRow {
                        label: "上".into(),
                        cards: vec![card],
                        min: 0,
                        max: 1,
                    },
                    MoveRow {
                        label: "下".into(),
                        cards: vec![],
                        min: if case == "bot-decline" { 0 } else { 1 },
                        max: 1,
                    },
                ],
                allow_decline: matches!(case, "decline" | "cancel" | "bot-decline"),
                filter_move: (case == "impossible").then(|| {
                    Arc::new(|_: &Game, _: &[Vec<Card>], _: MoveStep| false) as MoveFilter
                }),
                filter_ok: None,
                bot: match case {
                    "bot" => Some(Arc::new(move |_, _| {
                        Some(vec![MoveStep {
                            card: card.id,
                            row: 1,
                            index: 0,
                        }])
                    })),
                    "bot-decline" => Some(Arc::new(|_, _| None)),
                    _ => None,
                },
            },
            results.clone(),
        );
        if case == "cancel" {
            game.register_trigger(TriggerRegistration {
                owner: 0,
                skill: "cancel_move".into(),
                timing: "chooseToMoveBegin".into(),
                role: TriggerRole::Player,
                order: TriggerOrder::First,
                priority: 0,
                label: None,
                silent: true,
                filter: Arc::new(|_, _, _| true),
                effect: Arc::new(|_, _, _| TriggerStep::CancelSubject),
            })
            .unwrap();
        }
        start_choice(&mut game, id);
        let choice = game.view(Some(0)).unwrap().choice;
        if case == "decline" {
            let event = choice.unwrap().event;
            game.submit(Command {
                version: game.version,
                seat: 0,
                action: Action::MoveCards { event, moves: None },
            })
            .unwrap();
        } else if automated && !expected_cancel {
            let command = game.bot_command().expect("one checked bot action");
            if should_move {
                assert!(
                    matches!(command.action, Action::MoveCards { moves: Some(ref moves), .. } if moves.len() == 1)
                );
            } else {
                assert!(matches!(
                    command.action,
                    Action::MoveCards { moves: None, .. }
                ));
            }
            game.submit(command).unwrap();
        } else {
            assert!(choice.is_none());
        }
        let results = results.lock().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].cancelled, expected_cancel);
        assert_eq!(
            results[0].rows,
            should_move.then_some(vec![vec![], vec![card]])
        );
        let expected: &[&str] = if expected_cancel {
            &[
                "chooseToMoveBefore",
                "chooseToMoveBegin",
                "chooseToMoveCancelled",
            ]
        } else {
            &[
                "chooseToMoveBefore",
                "chooseToMoveBegin",
                "chooseToMoveEnd",
                "chooseToMoveAfter",
            ]
        };
        assert_eq!(*timings.lock().unwrap(), expected);
        let status = if expected_cancel {
            EventStatus::Cancelled
        } else {
            EventStatus::Finished
        };
        assert!(game
            .events
            .iter()
            .any(|event| event.id == results[0].event && event.status == status));
    }
}

#[test]
fn a_terminal_effect_during_begin_cancels_the_choice_for_a_surviving_seat() {
    let mut game = table();
    let card = game.classic.as_ref().unwrap().deck[0];
    let results = Arc::new(Mutex::new(Vec::new()));
    let id = queue_choice(
        &mut game,
        MoveChoiceRequest {
            seat: 1,
            prompt: "分组".into(),
            rows: vec![MoveRow {
                label: "上".into(),
                cards: vec![card],
                min: 0,
                max: 1,
            }],
            allow_decline: false,
            filter_move: None,
            filter_ok: None,
            bot: None,
        },
        results.clone(),
    );
    game.register_trigger(TriggerRegistration {
        owner: 1,
        skill: "move_test_terminal".into(),
        timing: "chooseToMoveBegin".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|ctx, _, _| {
            ctx.die(0, None).unwrap();
            TriggerStep::Done
        }),
    })
    .unwrap();
    start_choice(&mut game, id);
    let view = game.view(Some(1)).unwrap();
    assert!(view.seats[1].alive);
    assert!(view.winner.is_some());
    assert!(view.waiting_for.is_none());
    assert!(view.choice.is_none());
    assert!(results.lock().unwrap()[0].cancelled);
}
