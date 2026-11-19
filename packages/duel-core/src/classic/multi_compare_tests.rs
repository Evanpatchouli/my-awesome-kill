use super::*;
use crate::classic::tests::scene;
use crate::classic_data::{Role, Suit};
use crate::{
    Action, CardKind, ChoiceKind, Command, EventStatus, TriggerContext, TriggerOrder,
    TriggerRegistration, TriggerRole, TriggerSelection, TriggerStep,
};
use std::sync::Mutex;
fn reg(
    name: &str,
    timing: &str,
    effect: impl Fn(&mut TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) -> TriggerRegistration {
    TriggerRegistration {
        owner: 0,
        skill: name.into(),
        timing: timing.into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(effect),
    }
}
fn setup(ranks: [u8; 4]) -> (Game, Vec<Card>) {
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
    let cards: Vec<_> = [0, 1, 3, 4]
        .into_iter()
        .zip(ranks)
        .map(|(seat, rank)| {
            let card = game
                .create_content_card(CardKind::Tao, Suit::Heart, rank, None)
                .unwrap();
            game.seat_mut(seat).hand.push(card);
            card
        })
        .collect();
    (game, cards)
}
fn run(game: &mut Game) {
    game.run(None);
    for _ in 0..60 {
        let view = game.view(None).unwrap();
        let Some(seat) = view.waiting_for else { break };
        let Some(choice) = game.view(Some(seat)).unwrap().choice else {
            break;
        };
        let action = if choice.compare_group.is_some() {
            Action::Select {
                event: choice.event,
                branch: 1,
                cards: vec![choice.branches[0].cards[0]],
                targets: vec![],
            }
        } else {
            match choice.kind {
                ChoiceKind::Discard => Action::Discard {
                    event: choice.event,
                    cards: choice.cards[..choice.min].to_vec(),
                },
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
        };
        game.submit(Command {
            version: view.version,
            seat,
            action,
        })
        .unwrap();
        if !game.group_compare_history().is_empty() {
            break;
        }
    }
}
#[test]
fn pairwise_fixes_persist_across_real_pairs_and_callback_awaits() {
    let (mut game, cards) = setup([7, 8, 10, 12]);
    let visible = Arc::new(Mutex::new(Vec::new()));
    let visibility = visible.clone();
    let shown_cards = cards.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.timing != "showCardsAfter" {
            return;
        }
        assert!(matches!(
            game.event_kind(event.subject),
            Some(EventKind::ShowBid { .. })
        ));
        let view = game.view(None).unwrap();
        let exposed: Vec<_> = shown_cards
            .iter()
            .enumerate()
            .filter_map(|(i, card)| {
                view.card_faces
                    .iter()
                    .any(|face| face.id == card.id)
                    .then_some(i)
            })
            .collect();
        visibility
            .lock()
            .unwrap()
            .push((event.player.unwrap(), exposed));
    }));
    let pairs = Arc::new(Mutex::new(Vec::new()));
    let log = pairs.clone();
    let callback: TriggerEffect = Arc::new(move |ctx, stage, _| {
        assert_eq!(stage, 0);
        let frame = ctx.game().group_comparison(ctx.event().subject).unwrap();
        log.lock().unwrap().push((
            frame.target.unwrap(),
            frame.numbers,
            frame.winner,
            frame.result.as_ref().unwrap().numbers.len(),
        ));
        TriggerStep::Done
    });
    let mut fix = reg("fix_a", "compareFixing", |ctx, _, _| {
        let f = ctx.game().group_comparison(ctx.event().subject).unwrap();
        if f.index == 0 {
            ctx.set_compare_number(0, 10).unwrap();
        }
        TriggerStep::Done
    });
    fix.role = TriggerRole::Global;
    game.register_trigger(fix).unwrap();
    let program = game
        .compare_many_program(0, vec![4, 1, 3], CompareMode::Pairwise, Some(callback))
        .unwrap();
    game.scheduler.start(program);
    run(&mut game);
    let frame = game.group_compare_history().last().unwrap();
    assert_eq!(
        *visible.lock().unwrap(),
        [
            (0, vec![]),
            (1, vec![0, 1]),
            (3, vec![0, 1, 2]),
            (4, vec![0, 1, 2, 3])
        ]
    );
    assert!(!frame.cancelled);
    assert_eq!(frame.winner, None);
    assert_eq!(frame.group.targets, [1, 3, 4]);
    let result = frame.result.as_ref().unwrap();
    assert_eq!(result.cards, cards);
    assert_eq!(result.numbers, [[10, 8], [10, 10], [10, 12]]);
    assert_eq!(
        *pairs.lock().unwrap(),
        [
            (1, [Some(10), Some(8)], Some(0), 1),
            (3, [Some(10), Some(10)], None, 2),
            (4, [Some(10), Some(12)], Some(4), 3)
        ]
    );
    assert!(cards
        .iter()
        .all(|card| game.discard.iter().filter(|item| *item == card).count() == 1));
}
#[test]
fn simultaneous_saves_scores_and_winner_before_later_fixing() {
    let (mut game, cards) = setup([10, 9, 10, 8]);
    game.register_trigger(reg("boost", "compare", |ctx, _, _| {
        let f = ctx.game().group_comparison(ctx.event().subject).unwrap();
        if f.player == Some(0) && f.target.is_none() {
            ctx.set_compare_number(0, 11).unwrap();
        }
        TriggerStep::Done
    }))
    .unwrap();
    game.register_trigger(reg("fix", "compareFixing", |ctx, _, _| {
        let f = ctx.game().group_comparison(ctx.event().subject).unwrap();
        if f.player == Some(0) {
            ctx.set_compare_number(0, 1).unwrap();
        } else if f.target == Some(3) {
            ctx.set_compare_number(3, 13).unwrap();
        }
        TriggerStep::Done
    }))
    .unwrap();
    let program = game
        .compare_many_program(0, vec![4, 1, 3], CompareMode::Simultaneous, None)
        .unwrap();
    game.scheduler.start(program);
    run(&mut game);
    let frame = game.group_compare_history().last().unwrap();
    let result = frame.result.as_ref().unwrap();
    assert_eq!(result.cards, cards);
    assert_eq!(result.numbers, [[11, 9], [11, 10], [11, 8]]);
    assert_eq!(result.winner, Some(0));
    assert_eq!(frame.numbers, [Some(1), Some(8)]);
}
#[test]
fn root_cancel_keeps_result_private_and_single_loss_cancel_keeps_locked_bid() {
    for cancel_root in [false, true] {
        let (mut game, cards) = setup([9, 8, 10, 12]);
        let timing = if cancel_root {
            "compareCardShowBefore"
        } else {
            "loseBefore"
        };
        let mut cancel = reg("cancel", timing, |_, _, _| TriggerStep::CancelSubject);
        cancel.filter = Arc::new(move |_, e, _| cancel_root || e.player == Some(3));
        game.register_trigger(cancel).unwrap();
        let updates = Arc::new(Mutex::new(Vec::new()));
        let seen = updates.clone();
        let payment_rows = Arc::new(Mutex::new(Vec::new()));
        let trace = payment_rows.clone();
        let locked = cards.clone();
        game.timing_observer = Some(Arc::new(move |game, e| {
            if e.timing == "chooseToCompareCancelled" {
                seen.lock()
                    .unwrap()
                    .extend(game.group_updates.iter().cloned());
            }
            if !cancel_root
                && [
                    "loseAsyncBegin",
                    "loseBefore",
                    "loseAfter",
                    "loseCancelled",
                    "loseAsyncAfter",
                    "cardsDiscardAfter",
                ]
                .contains(&e.timing.as_str())
            {
                let labels = ["A", "B", "D", "E"];
                let seats = [0, 1, 3, 4];
                let zones: Vec<_> = locked
                    .iter()
                    .enumerate()
                    .map(|(i, card)| {
                        if game.seat(seats[i]).unwrap().hand.contains(card) {
                            format!("{}:h", labels[i])
                        } else if game.resolving.contains(card) {
                            "o".into()
                        } else if game.discard.contains(card) {
                            "d".into()
                        } else {
                            "?".into()
                        }
                    })
                    .collect();
                let player = e
                    .player
                    .and_then(|seat| seats.iter().position(|&id| id == seat))
                    .map(|i| labels[i]);
                if e.timing.starts_with("loseAsync") {
                    assert!(matches!(
                        game.event_kind(e.subject),
                        Some(EventKind::LoseAsync)
                    ));
                }
                if ["loseBefore", "loseAfter", "loseCancelled"].contains(&e.timing.as_str()) {
                    assert!(matches!(
                        e.parent.and_then(|id| game.event_kind(id)),
                        Some(EventKind::LoseAsync)
                    ));
                }
                trace
                    .lock()
                    .unwrap()
                    .push(serde_json::json!([e.timing, player, zones]));
            }
        }));
        let program = game
            .compare_many_program(0, vec![4, 1, 3], CompareMode::Pairwise, None)
            .unwrap();
        game.scheduler.start(program);
        run(&mut game);
        let frame = game.group_compare_history().last().unwrap();
        assert_eq!(frame.cancelled, cancel_root);
        assert_eq!(frame.result.as_ref().unwrap().cards, cards);
        if cancel_root {
            assert!(updates.lock().unwrap().is_empty());
            assert!(frame.result.as_ref().unwrap().numbers.is_empty());
            assert!(cards.iter().all(|card| game.discard.contains(card)));
        } else {
            let golden: serde_json::Value = serde_json::from_str(include_str!(
                "../../../../tests/reference/compare-multiple.json"
            ))
            .unwrap();
            let expected: Vec<_> = golden["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| {
                    row[0] == "cancel-one-loss"
                        && row[1] == "request"
                        && [
                            "loseAsyncBegin",
                            "loseBefore",
                            "loseAfter",
                            "loseCancelled",
                            "loseAsyncAfter",
                            "cardsDiscardAfter",
                        ]
                        .contains(&row[2].as_str().unwrap_or(""))
                })
                .map(|row| serde_json::json!([row[2], row[5], row[7]]))
                .collect();
            assert_eq!(*payment_rows.lock().unwrap(), expected);
            assert!(game.seat(3).unwrap().hand.contains(&cards[2]));
            assert_eq!(
                frame.result.as_ref().unwrap().numbers,
                [[9, 8], [9, 10], [9, 12]]
            );
            assert!(cards
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != 2)
                .all(|(_, card)| game.discard.contains(card)));
        }
        assert!(!game.events.iter().any(|e| e.status == EventStatus::Started
            && matches!(e.kind, EventKind::ShowCards { seat: 0, .. })));
    }
}
#[test]
fn simultaneous_tie_has_no_unique_winner_even_with_equal_high_scores() {
    let (mut game, _) = setup([10, 11, 11, 8]);
    game.register_trigger(reg("boost_tie", "compare", |ctx, _, _| {
        let frame = ctx.game().group_comparison(ctx.event().subject).unwrap();
        if frame.player == Some(0) && frame.target.is_none() {
            ctx.set_compare_number(0, 11).unwrap();
        }
        TriggerStep::Done
    }))
    .unwrap();
    let program = game
        .compare_many_program(0, vec![4, 1, 3], CompareMode::Simultaneous, None)
        .unwrap();
    game.scheduler.start(program);
    run(&mut game);
    let result = game
        .group_compare_history()
        .last()
        .unwrap()
        .result
        .as_ref()
        .unwrap();
    assert_eq!(result.numbers, [[11, 11], [11, 11], [11, 8]]);
    assert_eq!(result.winner, None);
}
#[test]
fn cancelled_initialized_group_returns_to_calling_content_after_cleanup() {
    let (mut game, cards) = setup([10, 9, 10, 8]);
    game.register_trigger(reg("cancel_group", "compareCardShowBefore", |_, _, _| {
        TriggerStep::CancelSubject
    }))
    .unwrap();
    let resumed = Arc::new(Mutex::new(None));
    let out = resumed.clone();
    let expected = cards.clone();
    let mut caller = reg(
        "caller",
        "phaseUseBegin",
        move |ctx, stage, _| match stage {
            0 => {
                ctx.compare_many(0, &[4, 1, 3], CompareMode::Simultaneous, None)
                    .unwrap();
                TriggerStep::Continue(1)
            }
            _ => {
                let frame = ctx
                    .compare_many_result()
                    .expect("initialized cancelled result returned");
                assert!(frame.cancelled);
                assert_eq!(frame.result.as_ref().unwrap().cards, expected);
                assert!(frame.result.as_ref().unwrap().numbers.is_empty());
                assert!(expected
                    .iter()
                    .all(|card| ctx.game().discard.contains(card)));
                *out.lock().unwrap() = Some(frame.event);
                TriggerStep::Done
            }
        },
    );
    caller.role = TriggerRole::Player;
    game.register_trigger(caller).unwrap();
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: crate::Phase::Play,
    }));
    run(&mut game);
    assert!(resumed.lock().unwrap().is_some());
}
#[test]
fn forced_pair_winner_is_consumed_once_and_none_clears_it() {
    let (mut game, _) = setup([9, 8, 10, 12]);
    let winners = Arc::new(Mutex::new(Vec::new()));
    let out = winners.clone();
    let callback: TriggerEffect = Arc::new(move |ctx, _, _| {
        out.lock().unwrap().push(
            ctx.game()
                .group_comparison(ctx.event().subject)
                .unwrap()
                .winner,
        );
        TriggerStep::Done
    });
    game.register_trigger(reg("force", "compareFixing", |ctx, _, _| {
        let index = ctx
            .game()
            .group_comparison(ctx.event().subject)
            .unwrap()
            .index;
        if index == 0 {
            assert_eq!(
                ctx.force_group_winner(Some(3)),
                Err(RuleError::InvalidTarget)
            );
            ctx.force_group_winner(Some(1)).unwrap();
        }
        if index == 2 {
            ctx.force_group_winner(Some(0)).unwrap();
            ctx.force_group_winner(None).unwrap();
        }
        TriggerStep::Done
    }))
    .unwrap();
    let program = game
        .compare_many_program(0, vec![4, 1, 3], CompareMode::Pairwise, Some(callback))
        .unwrap();
    game.scheduler.start(program);
    run(&mut game);
    assert_eq!(*winners.lock().unwrap(), [Some(1), Some(3), Some(4)]);
}
#[test]
fn pair_callback_can_suspend_and_resume_before_next_pair() {
    let (mut game, _) = setup([7, 8, 10, 12]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let out = events.clone();
    let callback: TriggerEffect = Arc::new(move |ctx, stage, _| {
        let target = ctx
            .game()
            .group_comparison(ctx.event().subject)
            .unwrap()
            .target
            .unwrap();
        assert_eq!(ctx.event().timing, "compareMultiple");
        assert_eq!(ctx.set_compare_number(0, 14), Err(RuleError::WrongEvent));
        out.lock().unwrap().push((stage, target));
        if stage == 0 {
            TriggerStep::Choose {
                branches: vec![crate::ChoiceBranch {
                    id: 0,
                    label: "continue".into(),
                    cards: vec![],
                    min_cards: 0,
                    max_cards: 0,
                    targets: vec![],
                    min_targets: 0,
                    max_targets: 0,
                }],
                then: 1,
            }
        } else {
            TriggerStep::Done
        }
    });
    let program = game
        .compare_many_program(0, vec![4, 1, 3], CompareMode::Pairwise, Some(callback))
        .unwrap();
    game.scheduler.start(program);
    run(&mut game);
    assert_eq!(
        *events.lock().unwrap(),
        [(0, 1), (1, 1), (0, 3), (1, 3), (0, 4), (1, 4)]
    );
}
#[path = "multi_compare_trace.rs"]
mod trace;
#[test]
fn simultaneous_first_target_can_increase_initiator_before_winner_selection() {
    let (mut game, _) = setup([7, 9, 8, 6]);
    let first = Arc::new(Mutex::new(None));
    let observed = first.clone();
    game.register_trigger(reg("raise_at_b", "compare", move |ctx, _, _| {
        let frame = ctx.game().group_comparison(ctx.event().subject).unwrap();
        if frame.player.is_none() && frame.target == Some(1) {
            *observed.lock().unwrap() = Some(frame.winner);
            ctx.set_compare_number(0, 13).unwrap();
        }
        TriggerStep::Done
    }))
    .unwrap();
    let program = game
        .compare_many_program(0, vec![4, 1, 3], CompareMode::Simultaneous, None)
        .unwrap();
    game.scheduler.start(program);
    run(&mut game);
    assert_eq!(*first.lock().unwrap(), Some(None));
    let result = game
        .group_compare_history()
        .last()
        .unwrap()
        .result
        .as_ref()
        .unwrap();
    assert_eq!(result.numbers, [[13, 9], [13, 8], [13, 6]]);
    assert_eq!(result.winner, Some(0));
}
#[test]
fn begin_interception_that_empties_a_hand_aborts_before_any_bid() {
    let (mut game, cards) = setup([10, 9, 10, 8]);
    let stolen = cards[2];
    game.register_trigger(reg("empty_d", "chooseToCompareBegin", move |ctx, _, _| {
        ctx.discard_hand(3, &[stolen.id]).unwrap();
        TriggerStep::Done
    }))
    .unwrap();
    let resumed = Arc::new(Mutex::new(false));
    let result = resumed.clone();
    let mut caller = reg(
        "caller_empty",
        "phaseUseBegin",
        move |ctx, stage, _| match stage {
            0 => {
                ctx.compare_many(0, &[4, 1, 3], CompareMode::Simultaneous, None)
                    .unwrap();
                TriggerStep::Continue(1)
            }
            _ => {
                let frame = ctx.compare_many_result().unwrap();
                assert!(!frame.cancelled);
                let outcome = frame.result.as_ref().unwrap();
                assert!(
                    outcome.selection_cancelled
                        && outcome.cards.is_empty()
                        && outcome.numbers.is_empty()
                        && outcome.winner.is_none()
                );
                *result.lock().unwrap() = true;
                TriggerStep::Done
            }
        },
    );
    caller.role = TriggerRole::Player;
    game.register_trigger(caller).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let out = requests.clone();
    game.timing_observer = Some(Arc::new(move |_, event| {
        if [
            "chooseCardOLBegin",
            "chooseCardBegin",
            "loseAsyncBegin",
            "chooseToCompareEnd",
            "chooseToCompareAfter",
            "chooseToCompareCancelled",
        ]
        .contains(&event.timing.as_str())
        {
            out.lock().unwrap().push(event.timing.clone());
        }
    }));
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: crate::Phase::Play,
    }));
    run(&mut game);
    assert!(*resumed.lock().unwrap());
    assert_eq!(
        *requests.lock().unwrap(),
        ["chooseToCompareEnd", "chooseToCompareAfter"]
    );
    let id = game.group_compare_history().last().unwrap().event;
    assert!(game
        .events
        .iter()
        .any(|event| event.id == id && event.status == EventStatus::Finished));
    assert!(game.group_updates.is_empty());
    assert!(game.seat(0).unwrap().hand.contains(&cards[0]));
    assert!(game.seat(1).unwrap().hand.contains(&cards[1]));
    assert!(game.seat(4).unwrap().hand.contains(&cards[3]));
    assert!(game.discard.contains(&stolen));
}
#[test]
fn dead_initiator_completes_started_pair_but_no_later_pair_callbacks() {
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
        1,
    );
    for (seat, rank) in [(1, 7), (2, 8), (4, 10), (5, 12)] {
        let card = game
            .create_content_card(CardKind::Tao, Suit::Heart, rank, None)
            .unwrap();
        game.seat_mut(seat).hand.push(card);
    }
    let calls = Arc::new(Mutex::new(Vec::new()));
    let out = calls.clone();
    let callback: TriggerEffect = Arc::new(move |ctx, stage, _| {
        let target = ctx
            .game()
            .group_comparison(ctx.event().subject)
            .unwrap()
            .target
            .unwrap();
        out.lock()
            .unwrap()
            .push((stage, target, ctx.game().is_alive(1)));
        match stage {
            0 => {
                ctx.die(1, None).unwrap();
                TriggerStep::Continue(1)
            }
            1 => TriggerStep::ChooseFor {
                seat: target,
                branches: vec![crate::ChoiceBranch {
                    id: 0,
                    label: "continue".into(),
                    cards: vec![],
                    min_cards: 0,
                    max_cards: 0,
                    targets: vec![],
                    min_targets: 0,
                    max_targets: 0,
                }],
                then: 2,
            },
            _ => TriggerStep::Done,
        }
    });
    let program = game
        .compare_many_program(1, vec![5, 2, 4], CompareMode::Pairwise, Some(callback))
        .unwrap();
    game.scheduler.start(program);
    run(&mut game);
    assert_eq!(
        *calls.lock().unwrap(),
        [(0, 2, true), (1, 2, false), (2, 2, false)]
    );
    let frame = game.group_compare_history().last().unwrap();
    assert!(!frame.cancelled);
    assert_eq!(
        frame.result.as_ref().unwrap().numbers,
        [[7, 8], [7, 10], [7, 12]]
    );
}
#[test]
fn resting_initiator_skips_new_pair_callbacks_after_the_bids_are_paid() {
    let (mut game, cards) = setup([9, 8, 10, 12]);
    game.register_trigger(reg("leave_comparison", "compareFixing", |ctx, _, _| {
        ctx.rest(0, crate::RestClock::default()).unwrap();
        TriggerStep::Done
    }))
    .unwrap();
    let callback: TriggerEffect =
        Arc::new(|_, _, _| panic!("an out initiator cannot run a new pair callback"));
    let compare = game
        .compare_many_program(0, vec![1, 3, 4], CompareMode::Pairwise, Some(callback))
        .unwrap();
    game.scheduler.start(compare);
    run(&mut game);
    assert!(game.is_alive(0) && game.is_out(0));
    let finished = game.group_compare_history().last().unwrap();
    assert!(!finished.cancelled);
    assert_eq!(
        finished.result.as_ref().unwrap().numbers,
        [[9, 8], [9, 10], [9, 12]]
    );
    for bid in cards {
        assert_eq!(game.discard.iter().filter(|&&c| c == bid).count(), 1);
    }
}

#[test]
fn cancelled_loss_batch_does_not_cancel_group_or_consume_locked_bids() {
    let (mut game, cards) = setup([9, 8, 10, 12]);
    game.register_trigger(reg("cancel_batch", "loseAsyncBegin", |_, _, _| {
        TriggerStep::CancelSubject
    }))
    .unwrap();
    let cancelled = Arc::new(Mutex::new(false));
    let seen = cancelled.clone();
    game.register_trigger(reg(
        "unexpected_root_cancel",
        "chooseToCompareCancelled",
        move |_, _, _| {
            *seen.lock().unwrap() = true;
            TriggerStep::Done
        },
    ))
    .unwrap();
    let program = game
        .compare_many_program(0, vec![4, 1, 3], CompareMode::Pairwise, None)
        .unwrap();
    game.scheduler.start(program);
    run(&mut game);
    assert!(!*cancelled.lock().unwrap());
    let frame = game.group_compare_history().last().unwrap();
    assert!(!frame.cancelled);
    assert_eq!(
        frame.result.as_ref().unwrap().numbers,
        [[9, 8], [9, 10], [9, 12]]
    );
    for (index, seat) in [0, 1, 3, 4].into_iter().enumerate() {
        assert!(game.seat(seat).unwrap().hand.contains(&cards[index]));
        assert!(!game.discard.contains(&cards[index]));
    }
}
#[test]
fn accepted_danlie_with_material_removed_at_skill_begin_finishes_without_winner() {
    let (mut game, cards) = setup([10, 9, 10, 8]);
    game.seat_mut(0).hp = 3;
    let active = game
        .register_active_skill(crate::compare_content::danlie(0))
        .unwrap();
    let lost = cards[2];
    game.register_trigger(reg(
        "lose_before_body",
        "useSkillBegin",
        move |ctx, _, _| {
            ctx.discard_hand(3, &[lost.id]).unwrap();
            TriggerStep::Done
        },
    ))
    .unwrap();
    let times = Arc::new(Mutex::new(Vec::new()));
    let out = times.clone();
    game.timing_observer = Some(Arc::new(move |_, event| {
        if [
            "chooseCardOLBegin",
            "chooseToCompareEnd",
            "chooseToCompareAfter",
            "chooseToCompareCancelled",
        ]
        .contains(&event.timing.as_str())
        {
            out.lock().unwrap().push(event.timing.clone());
        }
    }));
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: crate::Phase::Play,
    }));
    game.run(None);
    let view = game.view(None).unwrap();
    game.submit(Command {
        version: view.version,
        seat: 0,
        action: Action::ActivateSkill {
            registration: active,
            cards: vec![],
            targets: vec![4, 1, 3],
        },
    })
    .unwrap();
    let frame = game.group_compare_history().last().unwrap();
    assert!(!frame.cancelled);
    assert!(frame.result.as_ref().unwrap().selection_cancelled);
    assert_eq!(
        *times.lock().unwrap(),
        ["chooseToCompareEnd", "chooseToCompareAfter"]
    );
    assert_eq!(game.seat(0).unwrap().hp, 2);
    assert!(game.discard.contains(&lost));
    assert!([0, 1, 4]
        .into_iter()
        .zip([cards[0], cards[1], cards[3]])
        .all(|(seat, card)| game.seat(seat).unwrap().hand.contains(&card)));
}
