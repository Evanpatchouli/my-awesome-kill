use duel_core::{
    Action, Card, CardKind, Command, EventKind, EventStatus, Game, GameEvent, GameSetup, GameView,
    RuleError, SeatId, SeatSetup,
};

const TARGET: SeatId = 19;
const OBSERVER: SeatId = 80;
const SOURCE: SeatId = 3;
const SHA: Card = Card {
    id: 41,
    kind: CardKind::Sha,
};
const NEXT_SHA: Card = Card {
    id: 42,
    kind: CardKind::Sha,
};
const SHAN: Card = Card {
    id: 75,
    kind: CardKind::Shan,
};
const OTHER_CARD: Card = Card {
    id: 91,
    kind: CardKind::Sha,
};

fn setup(target_card: Card) -> GameSetup {
    GameSetup {
        active_seat: SOURCE,
        seats: vec![
            SeatSetup {
                id: TARGET,
                hp: 3,
                hand: vec![target_card],
            },
            SeatSetup {
                id: OBSERVER,
                hp: 4,
                hand: vec![],
            },
            SeatSetup {
                id: SOURCE,
                hp: 4,
                hand: vec![SHA, NEXT_SHA],
            },
        ],
    }
}

fn start_attack(game: &mut Game) {
    game.submit(Command {
        version: 0,
        seat: SOURCE,
        action: Action::PlaySha {
            card: SHA.id,
            target: TARGET,
        },
    })
    .unwrap();
}

fn event(id: u64, parent: Option<u64>, kind: EventKind, status: EventStatus) -> GameEvent {
    GameEvent {
        id,
        parent,
        kind,
        status,
    }
}

#[test]
fn shan_resumes_the_existing_choice_and_cancels_the_effect_before_cleaning_up_sha() {
    use EventKind::*;
    use EventStatus::*;

    let mut game = Game::new(setup(SHAN)).unwrap();
    assert_eq!(
        game.seat_order_from(OBSERVER).unwrap(),
        vec![OBSERVER, SOURCE, TARGET]
    );
    start_attack(&mut game);

    let use_sha = UseSha {
        seat: SOURCE,
        target: TARGET,
        card: SHA,
    };
    let sha_effect = ShaEffect {
        seat: SOURCE,
        target: TARGET,
    };
    let choose_shan = ChooseShan { seat: TARGET };
    let held = game.view(Some(TARGET)).unwrap();
    let choice = held.choice.unwrap();
    assert_eq!(held.version, 1);
    assert_eq!(held.waiting_for, Some(TARGET));
    assert_eq!(choice.event, 3);
    assert_eq!(choice.cards, vec![SHAN.id]);
    assert_eq!(held.seats[0].hand, Some(vec![SHAN]));
    assert_eq!(held.seats[0].hp, 3);
    assert_eq!(held.seats[2].hand, None);
    assert_eq!(held.resolving, vec![SHA]);
    assert!(held.discard.is_empty());
    assert_eq!(
        held.events,
        [
            event(1, None, use_sha, Started),
            event(2, Some(1), sha_effect, Started),
            event(3, Some(2), choose_shan, Started),
        ]
    );

    game.submit(Command {
        version: held.version,
        seat: TARGET,
        action: Action::Choose {
            event: choice.event,
            card: Some(SHAN.id),
        },
    })
    .unwrap();

    let use_shan = UseShan {
        seat: TARGET,
        card: SHAN,
    };
    let discard_shan = Discard { card: SHAN };
    let discard_sha = Discard { card: SHA };
    let resolved = game.view(None).unwrap();
    assert_eq!(resolved.version, 2);
    assert_eq!(resolved.waiting_for, None);
    assert_eq!(resolved.seats[0].hp, 3);
    assert_eq!(resolved.seats[0].hand_count, 0);
    assert!(resolved.resolving.is_empty());
    assert_eq!(resolved.discard, vec![SHAN, SHA]);
    assert_eq!(
        resolved.events,
        [
            event(4, Some(3), use_shan, Started),
            event(4, Some(3), use_shan, Finished),
            event(5, Some(4), discard_shan, Started),
            event(5, Some(4), discard_shan, Finished),
            event(3, Some(2), choose_shan, Finished),
            event(2, Some(1), sha_effect, Cancelled),
            event(1, None, use_sha, Finished),
            event(6, Some(1), discard_sha, Started),
            event(6, Some(1), discard_sha, Finished),
        ]
    );
}

#[test]
fn passing_applies_one_damage_and_allows_a_new_root_event() {
    let mut game = Game::new(setup(SHAN)).unwrap();
    start_attack(&mut game);
    let held = game.view(Some(TARGET)).unwrap();
    game.submit(Command {
        version: held.version,
        seat: TARGET,
        action: Action::Choose {
            event: held.choice.unwrap().event,
            card: None,
        },
    })
    .unwrap();

    let hit = game.view(Some(TARGET)).unwrap();
    assert_eq!(hit.version, 2);
    assert_eq!(hit.seats[0].hp, 2);
    assert_eq!(hit.seats[0].hand, Some(vec![SHAN]));
    assert_eq!(hit.discard, vec![SHA]);
    assert!(hit.resolving.is_empty());
    assert!(hit.choice.is_none());
    assert_eq!(
        hit.events[1],
        event(
            4,
            Some(2),
            EventKind::Damage {
                source: Some(SOURCE),
                target: TARGET,
                amount: 1,
            },
            EventStatus::Started
        )
    );
    assert_eq!(hit.events[2].status, EventStatus::Finished);

    game.submit(Command {
        version: hit.version,
        seat: SOURCE,
        action: Action::PlaySha {
            card: NEXT_SHA.id,
            target: OBSERVER,
        },
    })
    .unwrap();
    let next = game.view(Some(OBSERVER)).unwrap();
    assert_eq!(next.waiting_for, Some(OBSERVER));
    assert!(next.choice.unwrap().cards.is_empty());
    assert_eq!(
        next.events[0],
        event(
            6,
            None,
            EventKind::UseSha {
                seat: SOURCE,
                target: OBSERVER,
                card: NEXT_SHA,
            },
            EventStatus::Started
        )
    );
}

#[test]
fn malformed_and_stale_commands_leave_the_paused_game_unchanged() {
    let mut game = Game::new(setup(SHAN)).unwrap();
    start_attack(&mut game);
    let before = game.view(Some(TARGET)).unwrap();
    let event = before.choice.as_ref().unwrap().event;
    let response = Action::Choose {
        event,
        card: Some(SHAN.id),
    };
    let cases = [
        (
            0,
            TARGET,
            response.clone(),
            RuleError::WrongVersion {
                expected: 1,
                received: 0,
            },
        ),
        (
            1,
            SOURCE,
            response.clone(),
            RuleError::WrongActor { expected: TARGET },
        ),
        (
            1,
            TARGET,
            Action::Choose {
                event: event + 1,
                card: None,
            },
            RuleError::WrongEvent,
        ),
        (
            1,
            TARGET,
            Action::Choose {
                event,
                card: Some(NEXT_SHA.id),
            },
            RuleError::InvalidCard,
        ),
        (
            1,
            SOURCE,
            Action::PlaySha {
                card: NEXT_SHA.id,
                target: OBSERVER,
            },
            RuleError::ChoicePending,
        ),
    ];
    for (version, seat, action, error) in cases {
        assert_eq!(
            game.submit(Command {
                version,
                seat,
                action
            }),
            Err(error)
        );
        assert_eq!(game.view(Some(TARGET)).unwrap(), before);
    }

    let valid = Command {
        version: 1,
        seat: TARGET,
        action: response,
    };
    game.submit(valid.clone()).unwrap();
    let after = game.view(None).unwrap();
    assert_eq!(
        game.submit(valid),
        Err(RuleError::WrongVersion {
            expected: 2,
            received: 1
        })
    );
    assert_eq!(game.view(None).unwrap(), after);
}

#[test]
fn non_responders_cannot_infer_which_hidden_card_is_available() {
    let mut with_shan = Game::new(setup(SHAN)).unwrap();
    let mut without_shan = Game::new(setup(OTHER_CARD)).unwrap();
    start_attack(&mut with_shan);
    start_attack(&mut without_shan);

    for viewer in [None, Some(SOURCE), Some(OBSERVER)] {
        let projection = with_shan.view(viewer).unwrap();
        assert_eq!(projection, without_shan.view(viewer).unwrap());
        assert_eq!(projection.waiting_for, Some(TARGET));
        assert!(projection.choice.is_none());
        assert!(projection.seats[0].hand.is_none());
        assert_eq!(projection.seats[0].hand_count, 1);
    }

    let held = without_shan.view(Some(TARGET)).unwrap();
    let choice = held.choice.unwrap();
    assert!(choice.cards.is_empty());
    assert_eq!(held.seats[0].hand, Some(vec![OTHER_CARD]));
    without_shan
        .submit(Command {
            version: held.version,
            seat: TARGET,
            action: Action::Choose {
                event: choice.event,
                card: None,
            },
        })
        .unwrap();
    assert_eq!(without_shan.view(Some(TARGET)).unwrap().seats[0].hp, 2);
}

#[test]
fn protocol_uses_tagged_commands_and_events_with_camel_case_fields() {
    use serde_json::{json, Value};

    let mut game = Game::new(setup(SHAN)).unwrap();
    let command: Command = serde_json::from_value(json!({
        "version": 0, "seat": 3, "action": {"type": "playSha", "card": 41, "target": 19},
    }))
    .unwrap();
    game.submit(command).unwrap();
    let view = game.view(Some(TARGET)).unwrap();
    let payload = serde_json::to_value(&view).unwrap();
    assert_eq!(payload["activeSeat"], SOURCE);
    assert_eq!(payload["waitingFor"], TARGET);
    assert_eq!(
        payload["choice"],
        json!({"event": 3, "seat": 19, "kind": "shan", "target": null, "min": 0, "max": 1, "cards": [75]})
    );
    assert_eq!(payload["seats"][2]["hand"], Value::Null);
    assert_eq!(
        payload["events"][0],
        json!({
            "id": 1, "parent": null, "status": "started", "type": "useSha", "seat": 3,
            "target": 19, "card": {"id": 41, "kind": "sha"},
        })
    );
    assert_eq!(serde_json::from_value::<GameView>(payload).unwrap(), view);
}

#[test]
fn inconsistent_setup_and_invalid_attack_targets_are_rejected() {
    let mut duplicate_card = setup(SHAN);
    duplicate_card.seats[1].hand.push(SHAN);
    assert!(matches!(
        Game::new(duplicate_card),
        Err(RuleError::DuplicateCard { card: 75 })
    ));

    let mut duplicate_seat = setup(SHAN);
    duplicate_seat.seats[1].id = TARGET;
    assert!(matches!(
        Game::new(duplicate_seat),
        Err(RuleError::DuplicateSeat { seat: TARGET })
    ));

    let mut inactive_observer = setup(SHAN);
    inactive_observer.seats[1].hp = 0;
    let mut game = Game::new(inactive_observer).unwrap();
    let before = game.view(None).unwrap();
    for target in [SOURCE, OBSERVER] {
        assert_eq!(
            game.submit(Command {
                version: 0,
                seat: SOURCE,
                action: Action::PlaySha {
                    card: SHA.id,
                    target
                },
            }),
            Err(RuleError::InvalidTarget)
        );
        assert_eq!(game.view(None).unwrap(), before);
    }
}
