use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{Phase, Program};

fn table() -> Game {
    let mut game = scene(
        &[
            Role::Lord,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Rebel,
            Role::Rebel,
            Role::Rebel,
            Role::Rebel,
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
#[test]
fn paid_slash_loss_request_can_grant_bonus_for_that_same_use() {
    let mut game = table();
    game.seat_mut(0).hand.push(card(1, CardKind::Sha));
    let requested = Arc::new(Mutex::new(false));
    let seen = requested.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.timing != "loseAfter" || !event.cards.iter().any(|c| c.id == 1) {
            return;
        }
        assert!(!game.seat(0).unwrap().hand.iter().any(|c| c.id == 1));
        assert!(game.resolving.iter().any(|c| c.id == 1));
        assert!(matches!(
            game.scheduler.active_frame(event.subject).unwrap().1.kind(),
            EventKind::Lose { .. }
        ));
        assert!(matches!(
            game.scheduler
                .active_frame(event.parent.unwrap())
                .unwrap()
                .1
                .kind(),
            EventKind::UseSha { .. } | EventKind::UseShaTargets { .. }
        ));
        assert!(!game.has_skill(0, "qiaoshui3"));
        *seen.lock().unwrap() = true;
    }));
    let mut grant = registration(
        0,
        "gain_bonus_at_loss",
        "loseAfter",
        TriggerOrder::First,
        |ctx, _, _| {
            ctx.grant_temporary(0, crate::qiaoshui::win_skill(0))
                .unwrap();
            TriggerStep::Done
        },
    );
    grant.filter = Arc::new(|_, e, _| e.cards.iter().any(|c| c.id == 1));
    game.register_trigger(grant).unwrap();
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: 1,
            targets: vec![1],
        },
    );
    assert!(*requested.lock().unwrap());
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    let add = choice
        .branches
        .iter()
        .find(|b| b.label == "巧说：增加目标")
        .unwrap();
    assert!(add.targets.contains(&3));
    assert!(!game.has_skill(0, "qiaoshui3"));
    submit(
        &mut game,
        0,
        Action::Select {
            event: choice.event,
            branch: add.id,
            cards: vec![],
            targets: vec![3],
        },
    );
    for _ in 0..20 {
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        let Some(choice) = game.view(Some(seat)).unwrap().choice else {
            break;
        };
        match choice.kind {
            ChoiceKind::Shan | ChoiceKind::Save => submit(
                &mut game,
                seat,
                Action::Choose {
                    event: choice.event,
                    card: None,
                },
            ),
            ChoiceKind::Content => submit(
                &mut game,
                seat,
                Action::Select {
                    event: choice.event,
                    branch: 0,
                    cards: vec![],
                    targets: vec![],
                },
            ),
            _ => panic!("unexpected choice"),
        }
    }
    assert_eq!(game.seat(1).unwrap().hp, 3);
    assert_eq!(game.seat(3).unwrap().hp, 3);
    assert_eq!(game.discard.iter().filter(|c| c.id == 1).count(), 1);
}

#[test]
fn cancelled_play_phase_still_discards_and_expires_at_turn_end() {
    let mut game = Game::classic(5, 0, "guanyu", 8).unwrap();
    game.seat_mut(0).hp = 1;
    game.grant_temporary(0, crate::qiaoshui::win_skill(0))
        .unwrap();
    game.register_trigger(registration(
        0,
        "cancel_play_end",
        "phaseUseEnd",
        TriggerOrder::First,
        |_, _, _| TriggerStep::CancelSubject,
    ))
    .unwrap();
    let saw = Arc::new(Mutex::new(Vec::new()));
    let out = saw.clone();
    game.timing_observer = Some(Arc::new(move |_, event| {
        if event.player == Some(0) && ["phaseAfter"].contains(&event.timing.as_str()) {
            out.lock().unwrap().push(event.timing.clone());
        }
    }));
    let mut discarded = false;
    for _ in 0..100 {
        if !game.has_skill(0, "qiaoshui3") {
            break;
        }
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        let private = game.view(Some(seat)).unwrap();
        let action = if let Some(choice) = private.choice {
            match choice.kind {
                ChoiceKind::Discard => {
                    discarded = true;
                    Action::Discard {
                        event: choice.event,
                        cards: choice.cards[..choice.min].to_vec(),
                    }
                }
                ChoiceKind::Shan | ChoiceKind::Save => Action::Choose {
                    event: choice.event,
                    card: None,
                },
                ChoiceKind::Content => {
                    let b = &choice.branches[0];
                    Action::Select {
                        event: choice.event,
                        branch: b.id,
                        cards: b.cards[..b.min_cards].to_vec(),
                        targets: b.targets[..b.min_targets].to_vec(),
                    }
                }
            }
        } else {
            Action::EndPlay
        };
        submit(&mut game, seat, action);
    }
    assert!(!game.has_skill(0, "qiaoshui3"));
    assert!(discarded);
    assert_eq!(*saw.lock().unwrap(), ["phaseAfter"]);
}

#[test]
fn borrowed_sword_positional_self_victim_cannot_offer_lord_or_hand_attack() {
    let mut game = table();
    for (seat, hero) in [(3, "liubei"), (5, "guanyu")] {
        game.seat_mut(seat).hero = crate::classic_data::roster()
            .iter()
            .find(|general| general.id == hero);
    }
    game.seat_mut(0).role = Some(Role::Rebel);
    game.seat_mut(3).role = Some(Role::Lord);
    game.classic.as_mut().unwrap().lord = 3;
    game.seat_mut(0).hand.push(card(1, CardKind::Jiedao));
    game.seat_mut(3).hand.push(card(3, CardKind::Sha));
    game.seat_mut(5).hand.push(card(5, CardKind::Sha));
    game.seat_mut(1)
        .equipment
        .push(card(11, CardKind::Qinglong));
    game.seat_mut(3)
        .equipment
        .push(card(13, CardKind::Qinglong));
    game.grant_temporary(0, crate::qiaoshui::win_skill(0))
        .unwrap();
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: 1,
            targets: vec![3, 4],
        },
    );
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    let add = choice
        .branches
        .iter()
        .find(|b| b.label == "巧说：增加目标")
        .unwrap();
    submit(
        &mut game,
        0,
        Action::Select {
            event: choice.event,
            branch: add.id,
            cards: vec![],
            targets: vec![1],
        },
    );
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    assert_eq!(choice.target, Some(1));
    let branch = &choice.branches[0];
    assert!(branch.targets.contains(&3));
    submit(
        &mut game,
        0,
        Action::Select {
            event: choice.event,
            branch: branch.id,
            cards: vec![],
            targets: vec![3],
        },
    );
    let mut declined = false;
    for _ in 0..40 {
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        let Some(choice) = game.view(Some(seat)).unwrap().choice else {
            break;
        };
        if seat == 3 && choice.counter.is_none() {
            assert_eq!(choice.target, Some(3));
            assert_eq!(choice.branches.len(), 1);
            assert_eq!(choice.branches[0].id, 0);
            let before = game.view(Some(3)).unwrap();
            for (branch, cards) in [(1, vec![3]), (1000, vec![])] {
                assert_eq!(
                    game.submit(Command {
                        version: before.version,
                        seat: 3,
                        action: Action::Select {
                            event: choice.event,
                            branch,
                            cards,
                            targets: vec![]
                        }
                    }),
                    Err(RuleError::InvalidSelection)
                );
                assert_eq!(game.view(Some(3)).unwrap(), before);
            }
        }
        if seat == 1 && choice.counter.is_none() {
            assert_eq!(choice.target, Some(4));
            declined = true;
        }
        match choice.kind {
            ChoiceKind::Content => submit(
                &mut game,
                seat,
                Action::Select {
                    event: choice.event,
                    branch: 0,
                    cards: vec![],
                    targets: vec![],
                },
            ),
            ChoiceKind::Shan | ChoiceKind::Save => submit(
                &mut game,
                seat,
                Action::Choose {
                    event: choice.event,
                    card: None,
                },
            ),
            _ => panic!("unexpected choice"),
        }
    }
    assert!(declined);
    assert!(game
        .seat(0)
        .unwrap()
        .hand
        .contains(&card(13, CardKind::Qinglong)));
    assert!(game.seat(3).unwrap().hand.contains(&card(3, CardKind::Sha)));
    assert!(game.seat(5).unwrap().hand.contains(&card(5, CardKind::Sha)));
}

#[test]
fn multi_target_duel_never_requests_a_dead_source_to_respond() {
    let mut game = table();
    game.seat_mut(0).role = Some(Role::Rebel);
    game.seat_mut(7).role = Some(Role::Lord);
    game.classic.as_mut().unwrap().lord = 7;
    game.seat_mut(0).hp = 1;
    game.seat_mut(0).hand.push(card(1, CardKind::Juedou));
    game.seat_mut(1).hand.push(card(2, CardKind::Sha));
    game.grant_temporary(0, crate::qiaoshui::win_skill(0))
        .unwrap();
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: 1,
            targets: vec![1],
        },
    );
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    let add = choice
        .branches
        .iter()
        .find(|b| b.label == "巧说：增加目标")
        .unwrap();
    submit(
        &mut game,
        0,
        Action::Select {
            event: choice.event,
            branch: add.id,
            cards: vec![],
            targets: vec![3],
        },
    );
    let mut responded = false;
    let mut observed_dead = false;
    for _ in 0..80 {
        let view = game.view(None).unwrap();
        if !game.seat(0).unwrap().alive {
            observed_dead = true;
        }
        if view.waiting_for.is_none() {
            break;
        }
        let seat = view.waiting_for.unwrap();
        let Some(choice) = game.view(Some(seat)).unwrap().choice else {
            break;
        };
        if observed_dead {
            assert_ne!(seat, 0, "dead duel source must not receive a choice");
        }
        if seat == 1
            && choice.counter.is_none()
            && choice.branches.iter().any(|b| b.cards.contains(&2))
        {
            let branch = choice
                .branches
                .iter()
                .find(|b| b.cards.contains(&2))
                .unwrap();
            submit(
                &mut game,
                seat,
                Action::Select {
                    event: choice.event,
                    branch: branch.id,
                    cards: vec![2],
                    targets: vec![],
                },
            );
            responded = true;
            continue;
        }
        match choice.kind {
            ChoiceKind::Content => submit(
                &mut game,
                seat,
                Action::Select {
                    event: choice.event,
                    branch: 0,
                    cards: vec![],
                    targets: vec![],
                },
            ),
            ChoiceKind::Shan | ChoiceKind::Save => submit(
                &mut game,
                seat,
                Action::Choose {
                    event: choice.event,
                    card: None,
                },
            ),
            _ => panic!("unexpected choice"),
        }
    }
    assert!(responded && observed_dead);
    assert!(game.match_result().is_none());
}
