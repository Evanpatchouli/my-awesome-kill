use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, CardKind, ChoiceKind, Command, EventKind, Game, Program, TriggerOrder, TriggerRole,
};
use std::sync::{Arc, Mutex};

fn table_game() -> Game {
    let mut game = scene(
        &[Role::Lord, Role::Rebel, Role::Loyalist, Role::Renegade],
        0,
    );
    game.classic.as_mut().unwrap().turn = None;
    game.classic.as_mut().unwrap().deck = (100..300).map(|id| card(id, CardKind::Sha)).collect();
    game.seat_mut(0).hand.push(card(1, CardKind::Tao));
    game
}

fn select(game: &mut Game, branch: u32, cards: Vec<u32>, targets: Vec<SeatId>) {
    let view = game.view(None).unwrap();
    let seat = view.waiting_for.unwrap();
    let choice = game.view(Some(seat)).unwrap().choice.unwrap();
    game.submit(Command {
        version: view.version,
        seat,
        action: Action::Select {
            event: choice.event,
            branch,
            cards,
            targets,
        },
    })
    .unwrap();
}

#[test]
fn actual_fangquan_pays_then_targets_and_cleans_up_after_recipient_turn() {
    run_fangquan(false);
    run_fangquan(true);
}

fn run_fangquan(turned_over: bool) {
    let mut game = table_game();
    game.seat_mut(1).turned_over = turned_over;
    game.register_trigger(dangxian(0)).unwrap();
    game.register_trigger(fangquan(0)).unwrap();
    let first_seen = Arc::new(Mutex::new(false));
    let first = first_seen.clone();
    let mut observer = registration(
        1,
        "observer",
        if turned_over {
            "phaseCancelled"
        } else {
            "phaseAfter"
        },
        move |ctx, _, _| {
            if ctx
                .game()
                .turn_info(ctx.event().subject)
                .is_some_and(|turn| turn.reason.as_deref() == Some("fangquan2"))
            {
                assert!(ctx.game().has_skill(1, "fangquan3"));
                assert_eq!(ctx.game().skill_mark_count(1, "fangquan", Some(0)), 1);
                *first.lock().unwrap() = true;
            }
            TriggerStep::Done
        },
    );
    observer.order = TriggerOrder::First;
    observer.role = TriggerRole::Player;
    game.register_trigger(observer).unwrap();
    game.run(None);
    let mut offered = 0;
    for _ in 0..120 {
        if *first_seen.lock().unwrap() && !game.has_skill(1, "fangquan3") {
            break;
        }
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        match game.view(Some(seat)).unwrap().choice {
            None => game
                .submit(Command {
                    version: view.version,
                    seat,
                    action: Action::EndPlay,
                })
                .unwrap(),
            Some(choice) if choice.kind == ChoiceKind::Content => {
                if choice.branches.iter().any(|b| b.label == "发动放权") {
                    offered += 1;
                    select(&mut game, u32::from(offered == 1), vec![], vec![]);
                } else if let Some(pay) = choice.branches.iter().find(|b| b.label == "弃置一张手牌")
                {
                    select(&mut game, pay.id, vec![pay.cards[0]], vec![]);
                    assert!(game.discard.iter().any(|card| card.id == pay.cards[0]));
                    assert!(game
                        .turn_history()
                        .iter()
                        .all(|turn| turn.reason.as_deref() != Some("fangquan2")));
                } else if let Some(target) = choice
                    .branches
                    .iter()
                    .find(|b| b.label == "选择额外回合角色")
                {
                    select(&mut game, target.id, vec![], vec![1]);
                } else {
                    let branch = &choice.branches[0];
                    select(
                        &mut game,
                        branch.id,
                        branch.cards[..branch.min_cards].to_vec(),
                        branch.targets[..branch.min_targets].to_vec(),
                    );
                }
            }
            Some(choice) if choice.kind == ChoiceKind::Discard => game
                .submit(Command {
                    version: view.version,
                    seat,
                    action: Action::Discard {
                        event: choice.event,
                        cards: choice.cards[..choice.min].to_vec(),
                    },
                })
                .unwrap(),
            Some(choice) => game
                .submit(Command {
                    version: view.version,
                    seat,
                    action: Action::Choose {
                        event: choice.event,
                        card: None,
                    },
                })
                .unwrap(),
        }
    }
    assert_eq!(offered, 2);
    assert!(*first_seen.lock().unwrap());
    assert_eq!(game.skill_mark_count(1, "fangquan", Some(0)), 0);
    assert_eq!(game.skill_mark_count(0, "fangquan2", None), 0);
    assert!(!game.has_skill(1, "fangquan3"));
}

#[test]
fn indexed_fangquan_repeats_after_a_decline_and_interleaves_another_skill() {
    let mut game = table_game();
    game.register_trigger(registration(0, "setup", "phaseBegin", |ctx, _, _| {
        ctx.grant_temporary(0, fangquan2(0)).unwrap();
        ctx.add_skill_mark(0, "fangquan2", None, 2).unwrap();
        TriggerStep::Done
    }))
    .unwrap();
    let calls = Arc::new(Mutex::new(0));
    let calls2 = calls.clone();
    let mut other = registration(0, "other", "phaseEnd", move |_, _, _| {
        *calls2.lock().unwrap() += 1;
        TriggerStep::Done
    });
    other.silent = false;
    game.register_trigger(other).unwrap();
    game.run(None);
    let mut offered = 0;
    let mut order_choices = 0;
    let mut paid = false;
    for _ in 0..80 {
        if paid && game.skill_mark_count(1, "fangquan", Some(0)) == 1 {
            break;
        }
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        match game.view(Some(seat)).unwrap().choice {
            None => game
                .submit(Command {
                    version: view.version,
                    seat,
                    action: Action::EndPlay,
                })
                .unwrap(),
            Some(choice) if choice.kind == ChoiceKind::Content => {
                if choice.branches.iter().any(|b| b.label == "other") {
                    order_choices += 1;
                    let name = if order_choices == 1 {
                        "fangquan2"
                    } else {
                        "other"
                    };
                    let branch = choice.branches.iter().find(|b| b.label == name).unwrap();
                    select(&mut game, branch.id, vec![], vec![]);
                } else if let Some(pay) = choice.branches.iter().find(|b| b.label == "弃置一张手牌")
                {
                    offered += 1;
                    if offered == 1 {
                        select(&mut game, 0, vec![], vec![]);
                    } else {
                        select(&mut game, pay.id, vec![pay.cards[0]], vec![]);
                        paid = true;
                    }
                } else if let Some(target) = choice
                    .branches
                    .iter()
                    .find(|b| b.label == "选择额外回合角色")
                {
                    select(&mut game, target.id, vec![], vec![1]);
                } else {
                    panic!("unexpected choice: {:?}", choice.branches);
                }
            }
            Some(choice) if choice.kind == ChoiceKind::Discard => game
                .submit(Command {
                    version: view.version,
                    seat,
                    action: Action::Discard {
                        event: choice.event,
                        cards: choice.cards[..choice.min].to_vec(),
                    },
                })
                .unwrap(),
            Some(choice) => game
                .submit(Command {
                    version: view.version,
                    seat,
                    action: Action::Choose {
                        event: choice.event,
                        card: None,
                    },
                })
                .unwrap(),
        }
    }
    assert_eq!(offered, 2);
    assert_eq!(order_choices, 2);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(game.skill_mark_count(1, "fangquan", Some(0)), 1);
}

#[test]
fn indexed_snapshot_deduplicates_same_skill_and_rechecks_unregistration() {
    for unregister in [false, true] {
        let mut game = table_game();
        let log = Arc::new(Mutex::new(Vec::new()));
        let id_slot = Arc::new(Mutex::new(None));
        let output = log.clone();
        let id_for_effect = id_slot.clone();
        let indexed = registration(0, "indexed", "phaseEnd", move |ctx, _, _| {
            output.lock().unwrap().push("indexed");
            if unregister {
                ctx.unregister_trigger(id_for_effect.lock().unwrap().unwrap());
            }
            TriggerStep::Done
        });
        let id = game
            .register_indexed_trigger(indexed, Arc::new(|_, _, _| 2))
            .unwrap();
        *id_slot.lock().unwrap() = Some(id);
        let output = log.clone();
        game.register_trigger(registration(0, "indexed", "phaseEnd", move |_, _, _| {
            output.lock().unwrap().push("duplicate");
            TriggerStep::Done
        }))
        .unwrap();
        game.run(None);
        for _ in 0..30 {
            if game.turn_number() > 1 {
                break;
            }
            let view = game.view(None).unwrap();
            let seat = view.waiting_for.unwrap();
            match game.view(Some(seat)).unwrap().choice {
                None => game
                    .submit(Command {
                        version: view.version,
                        seat,
                        action: Action::EndPlay,
                    })
                    .unwrap(),
                Some(choice) if choice.kind == ChoiceKind::Discard => game
                    .submit(Command {
                        version: view.version,
                        seat,
                        action: Action::Discard {
                            event: choice.event,
                            cards: choice.cards[..choice.min].to_vec(),
                        },
                    })
                    .unwrap(),
                Some(choice) => game
                    .submit(Command {
                        version: view.version,
                        seat,
                        action: Action::Choose {
                            event: choice.event,
                            card: None,
                        },
                    })
                    .unwrap(),
            }
        }
        assert_eq!(
            *log.lock().unwrap(),
            if unregister {
                vec!["indexed"]
            } else {
                vec!["indexed", "indexed"]
            }
        );
    }
}

#[test]
fn death_during_fangquan_payment_never_prompts_for_a_target() {
    let mut game = table_game();
    game.seat_mut(0).hp = 1;
    game.register_trigger(fangquan(0)).unwrap();
    game.register_trigger(registration(
        0,
        "skip_discard",
        "phaseDiscardBefore",
        |_, _, _| TriggerStep::CancelSubject,
    ))
    .unwrap();
    game.register_trigger(registration(
        0,
        "payment_death",
        "loseAfter",
        |ctx, _, _| {
            ctx.lose_hp(0, 1).unwrap();
            TriggerStep::Done
        },
    ))
    .unwrap();
    game.run(None);
    let mut paid = false;
    for _ in 0..80 {
        if !game.is_alive(0) {
            break;
        }
        let view = game.view(None).unwrap();
        let Some(seat) = view.waiting_for else { break };
        match game.view(Some(seat)).unwrap().choice {
            None => game
                .submit(Command {
                    version: view.version,
                    seat,
                    action: Action::EndPlay,
                })
                .unwrap(),
            Some(choice) if choice.kind == ChoiceKind::Content => {
                assert!(!choice
                    .branches
                    .iter()
                    .any(|branch| branch.label == "选择额外回合角色"));
                if choice
                    .branches
                    .iter()
                    .any(|branch| branch.label == "发动放权")
                {
                    select(&mut game, 1, vec![], vec![]);
                } else if let Some(pay) = choice
                    .branches
                    .iter()
                    .find(|branch| branch.label == "弃置一张手牌")
                {
                    paid = true;
                    select(&mut game, pay.id, vec![pay.cards[0]], vec![]);
                } else {
                    panic!("unexpected choice");
                }
            }
            Some(choice) if choice.kind == ChoiceKind::Discard => game
                .submit(Command {
                    version: view.version,
                    seat,
                    action: Action::Discard {
                        event: choice.event,
                        cards: choice.cards[..choice.min].to_vec(),
                    },
                })
                .unwrap(),
            Some(choice) => game
                .submit(Command {
                    version: view.version,
                    seat,
                    action: Action::Choose {
                        event: choice.event,
                        card: None,
                    },
                })
                .unwrap(),
        }
    }
    assert!(paid);
    assert!(!game.is_alive(0));
    assert!(game
        .turn_history()
        .iter()
        .all(|turn| turn.reason.as_deref() != Some("fangquan2")));
    assert!(game
        .skill_marks(1)
        .iter()
        .all(|mark| mark.skill != "fangquan"));
}

#[test]
fn indexed_counts_capture_request_before_flip_and_damage_resolution() {
    let mut game = table_game();
    game.seat_mut(0).turned_over = true;
    let seen = Arc::new(Mutex::new(0));
    let log = seen.clone();
    game.register_indexed_trigger(
        registration(0, "flipped_count", "phaseCancelled", move |ctx, _, _| {
            assert!(!ctx.game().seat(0).unwrap().turned_over);
            *log.lock().unwrap() += 1;
            TriggerStep::Done
        }),
        Arc::new(|game, _, _| {
            if game.seat(0).unwrap().turned_over {
                2
            } else {
                0
            }
        }),
    )
    .unwrap();
    game.run(None);
    assert_eq!(*seen.lock().unwrap(), 2);

    let mut game = table_game();
    game.classic = None;
    let seen = Arc::new(Mutex::new(0));
    let log = seen.clone();
    game.register_indexed_trigger(
        registration(1, "damage_count", "damage", move |ctx, _, _| {
            assert_eq!(ctx.game().seat(1).unwrap().hp, 3);
            *log.lock().unwrap() += 1;
            TriggerStep::Done
        }),
        Arc::new(|game, _, _| if game.seat(1).unwrap().hp == 4 { 2 } else { 0 }),
    )
    .unwrap();
    game.scheduler.start(Program::event(EventKind::Damage {
        source: Some(0),
        target: 1,
        amount: 1,
    }));
    game.run(None);
    assert_eq!(*seen.lock().unwrap(), 2);
}

#[test]
fn indexed_registration_added_during_dispatch_samples_when_it_joins() {
    let mut game = table_game();
    let calls = Arc::new(Mutex::new(0));
    let results = calls.clone();
    game.register_trigger(registration(
        0,
        "install_late",
        "phaseBegin",
        move |ctx, _, _| {
            ctx.add_skill_mark(0, "late_count", None, 2).unwrap();
            let calls = results.clone();
            let callback = registration(0, "late", "phaseBegin", move |ctx, _, _| {
                *calls.lock().unwrap() += 1;
                ctx.clear_skill_mark(0, "late_count", None);
                TriggerStep::Done
            });
            ctx.register_indexed_trigger(
                callback,
                Arc::new(|game, _, _| game.skill_mark_count(0, "late_count", None)),
            )
            .unwrap();
            TriggerStep::Done
        },
    ))
    .unwrap();
    game.run(None);
    assert_eq!(*calls.lock().unwrap(), 2);
}
