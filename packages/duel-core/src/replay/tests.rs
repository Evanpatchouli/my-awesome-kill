use super::*;
use crate::{Action, Card, CardKind, GameView, SeatSetup};

fn projected(game: &Game, seats: &[SeatId]) -> Vec<GameView> {
    std::iter::once(None)
        .chain(seats.iter().copied().map(Some))
        .map(|seat| game.view(seat).unwrap())
        .collect()
}

#[test]
fn prescribed_replay_preserves_each_private_frame_and_rejects_an_invalid_timeline() {
    let mut session = RecordedGame::new(ReplaySetup::Prescribed {
        setup: GameSetup {
            active_seat: 3,
            seats: vec![
                SeatSetup {
                    id: 3,
                    hp: 4,
                    hand: vec![Card {
                        id: 10,
                        kind: CardKind::Sha,
                    }],
                },
                SeatSetup {
                    id: 7,
                    hp: 3,
                    hand: vec![Card {
                        id: 20,
                        kind: CardKind::Shan,
                    }],
                },
                SeatSetup {
                    id: 9,
                    hp: 4,
                    hand: vec![],
                },
            ],
        },
    })
    .unwrap();
    let observers = [3, 7, 9];
    let mut frames = vec![projected(session.game(), &observers)];
    session
        .submit(Command {
            version: 0,
            seat: 3,
            action: Action::PlaySha {
                card: 10,
                target: 7,
            },
        })
        .unwrap();
    let event = session.game().view(Some(7)).unwrap().choice.unwrap().event;
    assert!(session.game().view(Some(9)).unwrap().choice.is_none());
    frames.push(projected(session.game(), &observers));
    let response = Action::Choose { event, card: None };
    assert_eq!(
        session.submit(Command {
            version: 0,
            seat: 7,
            action: response.clone()
        }),
        Err(RuleError::WrongVersion {
            expected: 1,
            received: 0
        }),
    );
    assert_eq!(session.log().commands.len(), 1);
    assert_eq!(projected(session.game(), &observers), frames[1]);
    session
        .submit(Command {
            version: 1,
            seat: 7,
            action: response,
        })
        .unwrap();
    frames.push(projected(session.game(), &observers));
    assert_eq!(frames[2][0].seats[1].hp, 2);

    let encoded = serde_json::to_string(session.log()).unwrap();
    let log: ReplayLog = serde_json::from_str(&encoded).unwrap();
    for (accepted, expected) in frames.iter().enumerate() {
        assert_eq!(
            &projected(&log.restore_at(accepted).unwrap(), &observers),
            expected
        );
    }
    assert_eq!(projected(&log.restore().unwrap(), &observers), frames[2]);
    assert!(matches!(
        log.restore_at(3),
        Err(ReplayError::OutOfRange {
            requested: 3,
            total: 2
        })
    ));
    let mut invalid = log;
    invalid.commands[1].version = 0;
    assert!(matches!(
        invalid.restore(),
        Err(ReplayError::Command {
            index: 1,
            error: RuleError::WrongVersion {
                expected: 1,
                received: 0
            }
        })
    ));
    invalid.schema += 1;
    assert!(matches!(
        invalid.restore(),
        Err(ReplayError::UnsupportedSchema { received: 2 })
    ));
}

#[test]
fn an_entire_seeded_table_reconstructs_its_bot_commands_and_terminal_state() {
    const CONTENT: &str = "automated-table-v1";
    let factory = |setup: &ReplaySetup| {
        let mut game = setup.start()?;
        game.classic.as_mut().unwrap().human = SeatId::MAX;
        Ok(game)
    };
    let mut session = RecordedGame::with_factory(
        ReplaySetup::Seeded {
            seed: 20,
            human: 0,
            hero: "zhugeliang".into(),
            seats: 8,
        },
        CONTENT,
        factory,
    )
    .unwrap();
    let mut checkpoints = Vec::new();
    for _ in 0..2500 {
        if !session.bot().unwrap() {
            break;
        }
        let count = session.log().commands.len();
        if count % 71 == 0 {
            checkpoints.push((count, projected(session.game(), &[0, 1])));
        }
    }
    assert!(session.game().match_result().is_some());
    let log = session.log();
    assert!(log
        .commands
        .iter()
        .any(|cmd| matches!(cmd.action, Action::MoveCards { .. })));
    assert_eq!(
        log.commands
            .iter()
            .map(|cmd| cmd.version)
            .collect::<Vec<_>>(),
        (0..log.commands.len() as u32).collect::<Vec<_>>()
    );
    assert!(matches!(log.restore(), Err(ReplayError::ContentMismatch)));
    assert!(matches!(
        log.restore_with(0, "different", |_| panic!(
            "a mismatched factory cannot start"
        )),
        Err(ReplayError::ContentMismatch)
    ));
    let replayed = log
        .restore_with(log.commands.len(), CONTENT, factory)
        .unwrap();
    assert_eq!(
        projected(&replayed, &[0, 1, 2, 3, 4, 5, 6, 7]),
        projected(session.game(), &[0, 1, 2, 3, 4, 5, 6, 7])
    );
    for (accepted, expected) in checkpoints {
        assert_eq!(
            projected(
                &log.restore_with(accepted, CONTENT, factory).unwrap(),
                &[0, 1]
            ),
            expected
        );
    }
}

#[test]
fn indexed_registrations_consume_shared_content_state_in_registration_order_on_replay() {
    use crate::{TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn factory(setup: &ReplaySetup) -> Result<Game, RuleError> {
        let mut game = setup.start()?;
        let claimed = Arc::new(AtomicBool::new(false));
        for name in ["first", "second", "third", "fourth"] {
            let claimed = claimed.clone();
            game.register_indexed_trigger(
                TriggerRegistration {
                    owner: 0,
                    skill: name.into(),
                    timing: "useCard2".into(),
                    role: TriggerRole::Player,
                    order: TriggerOrder::First,
                    priority: 0,
                    label: None,
                    silent: true,
                    filter: Arc::new(|_, _, _| true),
                    effect: Arc::new(move |ctx, _, _| {
                        ctx.add_skill_mark(0, name, None, 1).unwrap();
                        TriggerStep::Done
                    }),
                },
                Arc::new(move |_, _, _| u32::from(!claimed.swap(true, Ordering::SeqCst))),
            )?;
        }
        Ok(game)
    }
    let mut recording = RecordedGame::with_factory(
        ReplaySetup::Seeded {
            seed: 317,
            human: 0,
            hero: "guanyu".into(),
            seats: 8,
        },
        "indexed-v1",
        factory,
    )
    .unwrap();
    recording
        .submit(Command {
            seat: 0,
            version: 0,
            action: Action::PlayCard {
                card: 28,
                targets: vec![1],
            },
        })
        .unwrap();
    let expected = projected(recording.game(), &[0, 1]);
    assert_eq!(recording.game().skill_mark_count(0, "first", None), 1);
    for _ in 0..8 {
        let replayed = recording
            .log()
            .restore_with(1, "indexed-v1", factory)
            .unwrap();
        assert_eq!(replayed.skill_mark_count(0, "first", None), 1);
        assert_eq!(projected(&replayed, &[0, 1]), expected);
    }
}
