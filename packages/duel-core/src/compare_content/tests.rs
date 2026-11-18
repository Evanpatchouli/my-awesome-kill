use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::{Role, Suit};
use crate::{Action, Card, CardKind, ChoiceKind, Command, EventKind, Phase, Program};

fn probe(
    name: &str,
    timing: &str,
    effect: impl Fn(&mut TriggerContext<'_>) -> TriggerStep + Send + Sync + 'static,
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
        effect: Arc::new(move |ctx, _, _| effect(ctx)),
    }
}

fn setup(ranks: [u8; 4], danlie: bool) -> (Game, Vec<Card>) {
    let mut game = scene(
        &[
            Role::Lord,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Rebel,
            Role::Rebel,
            Role::Rebel,
            Role::Renegade,
        ],
        0,
    );
    game.seat_mut(0).hp = if danlie { 3 } else { 4 };
    let mut bids = Vec::new();
    for (index, (&seat, rank)) in [0, 1, 3, 4].iter().zip(ranks).enumerate() {
        let bid = game
            .create_content_card(CardKind::Tao, Suit::Heart, rank, None)
            .unwrap();
        let other = game
            .create_content_card(CardKind::Shan, Suit::Club, 2, None)
            .unwrap();
        game.seat_mut(seat).hand.extend([bid, other]);
        bids.push(bid);
        assert_eq!(index, bids.len() - 1);
    }
    (game, bids)
}

fn choose(game: &mut Game, seat: SeatId, action: Action) {
    game.submit(Command {
        version: game.version,
        seat,
        action,
    })
    .unwrap();
}

#[test]
fn five_eight_seat_trials_match_reference_and_private_loser_choices() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/compare-multiple.json"
    ))
    .unwrap();
    for sample in golden["trials"].as_array().unwrap() {
        let name = sample["sample"].as_str().unwrap();
        let danlie_skill = sample["skill"] == "twdanlie";
        let ranks_vec: Vec<_> = sample["input"]["printedPoints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap() as u8)
            .collect();
        let (mut game, bids) = setup(ranks_vec.try_into().unwrap(), danlie_skill);
        let id = game
            .register_active_skill(if danlie_skill { danlie(0) } else { gushe(0) })
            .unwrap();
        if danlie_skill {
            for registration in danlie_number(0) {
                game.register_trigger(registration).unwrap();
            }
        }
        if name == "pairs-fixing" || name == "meanwhile-fixing" {
            let sample_name = name.to_owned();
            game.register_trigger(probe("fixture_fix", "compareFixing", move |ctx| {
                let frame = ctx.game().group_comparison(ctx.event().subject).unwrap();
                if sample_name == "pairs-fixing" && frame.target == Some(1) {
                    ctx.set_compare_number(0, 10).unwrap();
                } else if sample_name == "meanwhile-fixing" && frame.player == Some(0) {
                    ctx.set_compare_number(0, 1).unwrap();
                } else if sample_name == "meanwhile-fixing" && frame.target == Some(3) {
                    ctx.set_compare_number(3, 13).unwrap();
                }
                TriggerStep::Done
            }))
            .unwrap();
        }
        if name == "cancel-one-loss" {
            let bid = bids[2];
            game.register_trigger(probe("fixture_cancel_loss", "loseBefore", move |ctx| {
                if ctx.event().player == Some(3) && ctx.event().cards.contains(&bid) {
                    TriggerStep::CancelSubject
                } else {
                    TriggerStep::Done
                }
            }))
            .unwrap();
        }
        if name == "cancel-root" {
            game.register_trigger(probe(
                "fixture_cancel_root",
                "compareCardShowBefore",
                |_| TriggerStep::CancelSubject,
            ))
            .unwrap();
        }
        game.scheduler.start(Program::event(EventKind::Phase {
            seat: 0,
            phase: Phase::Play,
        }));
        game.run(None);
        choose(
            &mut game,
            0,
            Action::ActivateSkill {
                registration: id,
                cards: vec![],
                targets: vec![4, 1, 3],
            },
        );
        let mut loser_prompts = 0;
        for _ in 0..90 {
            let view = game.view(None).unwrap();
            let Some(seat) = view.waiting_for else { break };
            let Some(choice) = game.view(Some(seat)).unwrap().choice else {
                break;
            };
            let action = if choice.compare_group.is_some() {
                for bid in &bids {
                    assert!(
                        !game.is_known_by(bid.id, Some(2)),
                        "sealed bid visible to spectator"
                    );
                }
                let bid = bids[[0, 1, 3, 4].iter().position(|&s| s == seat).unwrap()];
                Action::Select {
                    event: choice.event,
                    branch: 1,
                    cards: vec![bid.id],
                    targets: vec![],
                }
            } else if choice.kind == ChoiceKind::Content
                && choice
                    .branches
                    .iter()
                    .any(|branch| branch.label == "弃置一张牌")
            {
                loser_prompts += 1;
                assert!(
                    game.view(Some(2)).unwrap().choice.is_none(),
                    "loser decision is private"
                );
                Action::Select {
                    event: choice.event,
                    branch: 0,
                    cards: vec![],
                    targets: vec![],
                }
            } else if choice.kind == ChoiceKind::Discard {
                Action::Discard {
                    event: choice.event,
                    cards: choice.cards[..choice.min].to_vec(),
                }
            } else {
                panic!("unexpected {name} choice {choice:?}");
            };
            choose(&mut game, seat, action);
        }
        let frame = game
            .group_compare_history()
            .last()
            .expect("comparison complete");
        assert_eq!(
            frame.cancelled,
            sample["cancelled"].as_bool().unwrap(),
            "{name}"
        );
        let result = frame.result.as_ref().unwrap();
        let actual_num1: Vec<_> = result.numbers.iter().map(|n| n[0]).collect();
        let actual_num2: Vec<_> = result.numbers.iter().map(|n| n[1]).collect();
        let nums = |key| {
            sample["result"][key]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_i64().unwrap() as i32)
                .collect::<Vec<_>>()
        };
        assert_eq!(actual_num1, nums("num1"), "{name}");
        assert_eq!(actual_num2, nums("num2"), "{name}");
        let winner = sample["result"]["winner"]
            .as_str()
            .map(|name| u32::from(name.as_bytes()[0] - b'A'));
        if danlie_skill {
            assert_eq!(result.winner, winner, "{name}");
        }
        let hp: Vec<_> = [0, 1, 3, 4]
            .iter()
            .map(|&seat| game.health(seat).unwrap().0)
            .collect();
        let expected_hp: Vec<_> = sample["hp"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_i64().unwrap() as i32)
            .collect();
        assert_eq!(hp, expected_hp, "{name}");
        if danlie_skill {
            let hits: Vec<_> = game
                .damage_history()
                .iter()
                .map(|hit| {
                    (
                        hit.source,
                        hit.target,
                        hit.amount,
                        hit.card_kind,
                        hit.cards.len(),
                    )
                })
                .collect();
            let expected = if winner == Some(0) {
                vec![
                    (Some(0), 1, 1, None, 0),
                    (Some(0), 3, 1, None, 0),
                    (Some(0), 4, 1, None, 0),
                ]
            } else {
                vec![]
            };
            assert_eq!(hits, expected, "{name} damage order and source");
        }
        assert_eq!(
            game.skill_mark_count(0, "gushe", None),
            sample["gusheMarks"].as_u64().unwrap() as u32,
            "{name}"
        );
        for (index, bid) in bids.iter().enumerate() {
            let expected = sample["zones"][index].as_str().unwrap();
            if expected == "d" {
                assert_eq!(
                    game.discard.iter().filter(|&&card| card == *bid).count(),
                    1,
                    "{name}"
                );
            } else {
                assert!(game.seat(3).unwrap().hand.contains(bid), "{name}");
            }
        }
        if !danlie_skill {
            assert!(loser_prompts >= 3);
        }
        assert!(
            game.view(Some(0))
                .unwrap()
                .active_plays
                .iter()
                .all(|play| play.registration != id),
            "once per phase"
        );
    }
}

#[test]
fn gushe_discard_is_paid_before_next_pair_is_revealed() {
    let (mut game, bids) = setup([13, 8, 10, 12], false);
    let armor = card(999, CardKind::Bagua);
    game.seat_mut(1).equipment.push(armor);
    let paid_before_next = Arc::new(std::sync::Mutex::new(false));
    let observed = paid_before_next.clone();
    game.register_trigger(probe("payment_order", "showCardsAfter", move |ctx| {
        if ctx.event().player == Some(3) {
            assert!(ctx.game().discard.contains(&armor));
            *observed.lock().unwrap() = true;
        }
        TriggerStep::Done
    }))
    .unwrap();
    let id = game.register_active_skill(gushe(0)).unwrap();
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    choose(
        &mut game,
        0,
        Action::ActivateSkill {
            registration: id,
            cards: vec![],
            targets: vec![1, 3, 4],
        },
    );
    for _ in 0..50 {
        let view = game.view(None).unwrap();
        let Some(seat) = view.waiting_for else { break };
        let Some(choice) = game.view(Some(seat)).unwrap().choice else {
            break;
        };
        let action = if choice.compare_group.is_some() {
            let bid = bids[[0, 1, 3, 4].iter().position(|&s| s == seat).unwrap()];
            Action::Select {
                event: choice.event,
                branch: 1,
                cards: vec![bid.id],
                targets: vec![],
            }
        } else if choice
            .branches
            .iter()
            .any(|branch| branch.cards.contains(&armor.id))
        {
            assert_eq!(seat, 1);
            assert!(
                !game.is_known_by(bids[2].id, Some(2)),
                "next bid remains hidden during payment"
            );
            Action::Select {
                event: choice.event,
                branch: 1,
                cards: vec![armor.id],
                targets: vec![],
            }
        } else {
            Action::Select {
                event: choice.event,
                branch: 0,
                cards: vec![],
                targets: vec![],
            }
        };
        choose(&mut game, seat, action);
    }
    assert!(*paid_before_next.lock().unwrap());
    assert_eq!(
        game.discard.iter().filter(|&&card| card == armor).count(),
        1
    );
}

#[test]
fn seventh_gushe_mark_causes_death_before_later_pairs() {
    let (mut game, bids) = setup([2, 13, 10, 12], false);
    game.add_skill_mark(0, "gushe", None, 6).unwrap();
    let id = game.register_active_skill(gushe(0)).unwrap();
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    choose(
        &mut game,
        0,
        Action::ActivateSkill {
            registration: id,
            cards: vec![],
            targets: vec![1, 3, 4],
        },
    );
    for _ in 0..30 {
        if !game.is_alive(0) {
            break;
        }
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        let choice = game.view(Some(seat)).unwrap().choice.unwrap();
        assert!(
            choice.compare_group.is_some(),
            "death occurs before loser choices"
        );
        let bid = bids[[0, 1, 3, 4].iter().position(|&s| s == seat).unwrap()];
        choose(
            &mut game,
            seat,
            Action::Select {
                event: choice.event,
                branch: 1,
                cards: vec![bid.id],
                targets: vec![],
            },
        );
    }
    assert!(!game.is_alive(0));
    assert!(!game.is_known_by(bids[2].id, Some(2)));
}

#[test]
fn danlie_number_applies_to_both_ordinary_compare_roles_and_caps_at_thirteen() {
    let (mut game, bids) = setup([12, 12, 10, 8], false);
    game.seat_mut(0).hp = 2;
    game.seat_mut(1).hp = 3;
    for owner in [0, 1] {
        for reg in danlie_number(owner) {
            game.register_trigger(reg).unwrap();
        }
    }
    let program = game.compare_program(0, 1).unwrap();
    game.scheduler.start(program);
    game.run(None);
    for _ in 0..12 {
        if !game.compare_history().is_empty() {
            break;
        }
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        let choice = game.view(Some(seat)).unwrap().choice.unwrap();
        let bid = if seat == 0 { bids[0] } else { bids[1] };
        choose(
            &mut game,
            seat,
            Action::Select {
                event: choice.event,
                branch: 1,
                cards: vec![bid.id],
                targets: vec![],
            },
        );
    }
    assert_eq!(game.compare_history().last().unwrap().numbers, [13, 13]);
}

#[test]
fn gushe_once_per_phase_resets_for_the_next_inserted_play_phase() {
    let (mut game, bids) = setup([13, 8, 10, 12], false);
    game.classic.as_mut().unwrap().turn = None;
    game.classic.as_mut().unwrap().deck = (1000..1100).map(|id| card(id, CardKind::Sha)).collect();
    game.register_trigger(crate::phase_content::dangxian(0))
        .unwrap();
    let id = game.register_active_skill(gushe(0)).unwrap();
    game.run(None);
    choose(
        &mut game,
        0,
        Action::ActivateSkill {
            registration: id,
            cards: vec![],
            targets: vec![1],
        },
    );
    for _ in 0..30 {
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        let Some(choice) = game.view(Some(seat)).unwrap().choice else {
            break;
        };
        let action = if choice.compare_group.is_some() {
            Action::Select {
                event: choice.event,
                branch: 1,
                cards: vec![if seat == 0 { bids[0].id } else { bids[1].id }],
                targets: vec![],
            }
        } else {
            Action::Select {
                event: choice.event,
                branch: 0,
                cards: vec![],
                targets: vec![],
            }
        };
        choose(&mut game, seat, action);
    }
    assert!(game
        .view(Some(0))
        .unwrap()
        .active_plays
        .iter()
        .all(|play| play.registration != id));
    choose(&mut game, 0, Action::EndPlay);
    let view = game.view(Some(0)).unwrap();
    assert_eq!(view.waiting_for, Some(0));
    assert!(view.choice.is_none());
    assert!(view.active_plays.iter().any(|play| play.registration == id));
}

#[test]
fn non_lord_gushe_death_keeps_current_pair_choice_and_later_scores() {
    let roles = [
        Role::Lord,
        Role::Rebel,
        Role::Loyalist,
        Role::Rebel,
        Role::Rebel,
        Role::Loyalist,
        Role::Rebel,
        Role::Renegade,
    ];
    let mut game = scene(&roles, 1);
    let mut bids = Vec::new();
    let mut extras = Vec::new();
    for (seat, rank) in [(1, 10), (3, 10), (4, 11), (0, 12)] {
        let bid = game
            .create_content_card(CardKind::Tao, Suit::Heart, rank, None)
            .unwrap();
        let extra = game
            .create_content_card(CardKind::Shan, Suit::Club, 2, None)
            .unwrap();
        game.seat_mut(seat).hand.extend([bid, extra]);
        bids.push(bid);
        extras.push(extra);
    }
    game.add_skill_mark(1, "gushe", None, 6).unwrap();
    let death_mark = Arc::new(std::sync::Mutex::new(None));
    let death_mark_probe = death_mark.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.timing == "die" && event.player == Some(1) {
            *death_mark_probe.lock().unwrap() = Some(game.skill_mark_count(1, "gushe", None));
        }
    }));
    let id = game.register_active_skill(gushe(1)).unwrap();
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 1,
        phase: Phase::Play,
    }));
    game.run(None);
    choose(
        &mut game,
        1,
        Action::ActivateSkill {
            registration: id,
            cards: vec![],
            targets: vec![0, 4, 3],
        },
    );
    let mut callbacks = Vec::new();
    let mut pairs = Vec::new();
    let mut paid = false;
    for _ in 0..40 {
        for event in &game.events {
            if let EventKind::ComparePairCallback { seat, target, .. } = event.kind {
                if event.status == crate::EventStatus::Started {
                    callbacks.push((seat, target));
                }
            }
        }
        for announcement in &game.group_updates {
            if let crate::GroupAnnouncement::Pair {
                target,
                numbers,
                winner,
                ..
            } = announcement
            {
                pairs.push((*target, *numbers, *winner));
            }
        }
        if !game.group_compare_history().is_empty() {
            break;
        }
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.expect("active group decision");
        let choice = game
            .view(Some(seat))
            .unwrap()
            .choice
            .expect("private decision");
        let action = if choice.compare_group.is_some() {
            let index = [1, 3, 4, 0]
                .iter()
                .position(|&candidate| candidate == seat)
                .unwrap();
            Action::Select {
                event: choice.event,
                branch: 1,
                cards: vec![bids[index].id],
                targets: vec![],
            }
        } else {
            assert_eq!(seat, 3, "only the living first opponent has a loser choice");
            assert!(
                !game.is_alive(1),
                "the tie's seventh mark killed the non-lord owner"
            );
            assert!(choice
                .branches
                .iter()
                .any(|branch| branch.cards.contains(&extras[1].id)));
            paid = true;
            Action::Select {
                event: choice.event,
                branch: 1,
                cards: vec![extras[1].id],
                targets: vec![],
            }
        };
        choose(&mut game, seat, action);
    }
    assert!(paid);
    assert_eq!(*death_mark.lock().unwrap(), Some(7));
    assert!(!game.is_alive(1));
    assert!(game.match_result().is_none());
    assert_eq!(callbacks, [(1, 3)], "no new pair callback after death");
    assert_eq!(
        pairs,
        [
            (3, [10, 10], None),
            (4, [10, 11], Some(4)),
            (0, [10, 12], Some(0))
        ]
    );
    let frame = game
        .group_compare_history()
        .last()
        .expect("group continued through cleanup");
    assert!(!frame.cancelled);
    assert_eq!(
        frame.result.as_ref().unwrap().numbers,
        [[10, 10], [10, 11], [10, 12]]
    );
    assert_eq!(
        game.skill_mark_count(1, "gushe", None),
        0,
        "death cleared marks and later pairs did not add any"
    );
    for item in bids.iter().chain(std::iter::once(&extras[1])) {
        assert_eq!(
            game.discard.iter().filter(|&&card| card == *item).count(),
            1
        );
        assert!(!game.resolving.contains(item));
    }
    assert!(game.seat(4).unwrap().hand.contains(&extras[2]));
    assert!(game.seat(0).unwrap().hand.contains(&extras[3]));
}
