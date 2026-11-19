use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, ActiveSkillRegistration, Card, ChoiceBranch, Command, EventStatus, Phase, TriggerOrder,
    TriggerRegistration, TriggerRole, TriggerStep,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const TIMINGS: &[&str] = &[
    "disableJudgeBefore",
    "disableJudgeBegin",
    "disableJudgeEnd",
    "disableJudgeAfter",
    "enableJudgeBefore",
    "enableJudgeBegin",
    "enableJudgeEnd",
    "enableJudgeAfter",
    "discardBefore",
    "discardBegin",
    "discard",
    "discardEnd",
    "discardAfter",
    "discardCancelled",
    "loseBefore",
    "loseBegin",
    "loseEnd",
    "loseAfter",
    "loseCancelled",
];

fn kind(kind: EventKind) -> &'static str {
    match kind {
        EventKind::DisableJudge { .. } => "disableJudge",
        EventKind::EnableJudge { .. } => "enableJudge",
        EventKind::DiscardGroup { .. } => "discard",
        EventKind::Lose { .. } => "lose",
        EventKind::ActivateSkill { .. } | EventKind::ActiveSkillEffect { .. } => {
            "reference_fixture_use"
        }
        _ => "other",
    }
}
fn name(card: Card) -> &'static str {
    match card.id {
        1 => "hand",
        2 => "equip",
        3 => "guose-material",
        4 => "lightning",
        _ => panic!("unexpected tracked card"),
    }
}
fn zone(game: &Game, card: Card) -> Value {
    let loc = game
        .card_location(card.id)
        .expect("tracked card has a location");
    match (loc.owner, loc.zone) {
        (Some(1), Some(crate::CardZone::Hand)) => json!("B:h"),
        (Some(1), Some(crate::CardZone::Equipment)) => json!("B:e"),
        (Some(1), Some(crate::CardZone::Judgment)) => json!("B:j"),
        (_, Some(crate::CardZone::DiscardPile)) => json!("d"),
        other => panic!("unexpected location {other:?}"),
    }
}
fn zones(game: &Game, cards: &[Card]) -> Value {
    let mut result = serde_json::Map::new();
    for &card in cards {
        result.insert(name(card).into(), zone(game, card));
    }
    Value::Object(result)
}
fn judge_names(game: &Game) -> Vec<&'static str> {
    game.seat(1)
        .unwrap()
        .judges
        .iter()
        .rev()
        .map(|&card| {
            if game.judgment_kind(card) == CardKind::Lebu {
                "lebu"
            } else {
                name(card)
            }
        })
        .collect()
}
fn row(game: &Game, e: &crate::TriggerEvent, trial: &str, category: &str, cards: &[Card]) -> Value {
    let event = kind(game.event_kind(e.subject).unwrap());
    let parent = e.parent.and_then(|p| game.event_kind(p)).map(kind);
    let seat = |s: Option<SeatId>| s.map(|s| ((b'A' + s as u8) as char).to_string());
    let player = game.seat(1).unwrap();
    json!([
        trial,
        category,
        e.timing,
        event,
        parent,
        seat(e.player),
        seat(e.source),
        e.cards.iter().copied().map(name).collect::<Vec<_>>(),
        player.judge_disabled,
        usize::from(player.judge_disabled_marker),
        judge_names(game),
        zones(game, cards)
    ])
}
fn check_stage(game: &Game, expected: &Value, cards: &[Card]) {
    let p = game.seat(1).unwrap();
    let view = game.view(None).unwrap();
    let v = view.seats.iter().find(|s| s.id == 1).unwrap();
    let can_add: Vec<_> = [CardKind::Lebu, CardKind::Shandian]
        .iter()
        .map(|&k| game.can_add_judge(1, k))
        .collect();
    for (field, actual) in [
        ("disabled", json!(game.is_judge_disabled(1))),
        ("placeholder", json!(usize::from(v.judge_disabled_marker))),
        ("judge", json!(judge_names(game))),
        (
            "virtual",
            json!(p
                .judges
                .iter()
                .map(|&c| match game.judgment_kind(c) {
                    CardKind::Lebu => "lebu",
                    CardKind::Shandian => "shandian",
                    _ => panic!("unexpected judge"),
                })
                .collect::<Vec<_>>()),
        ),
        (
            "hand",
            json!(p.hand.iter().copied().map(name).collect::<Vec<_>>()),
        ),
        (
            "equip",
            json!(p.equipment.iter().copied().map(name).collect::<Vec<_>>()),
        ),
        ("cards", zones(game, cards)),
    ] {
        assert_eq!(actual, expected[field], "{} {field}", expected["name"]);
    }
    assert_eq!(
        json!(can_add),
        json!(expected["canAdd"].as_array().unwrap()[..2])
    );
    assert_eq!(v.judge_disabled, game.is_judge_disabled(1));
    for &c in cards {
        let count = game
            .seats
            .iter()
            .map(|s| {
                s.hand
                    .iter()
                    .chain(&s.equipment)
                    .chain(&s.judges)
                    .filter(|x| **x == c)
                    .count()
            })
            .sum::<usize>()
            + game.discard.iter().filter(|x| **x == c).count();
        assert_eq!(
            count, 1,
            "tracked physical card must have exactly one location: {c:?}"
        );
    }
}
fn register(game: &mut Game, disable: bool) -> u64 {
    game.register_active_skill(ActiveSkillRegistration {
        owner: 0,
        skill: if disable {
            "fixture_disable"
        } else {
            "fixture_enable"
        }
        .into(),
        limited: false,
        choice: Arc::new(|_, _| {
            Some(ChoiceBranch {
                id: 0,
                label: "fixture".into(),
                cards: vec![],
                min_cards: 0,
                max_cards: 0,
                targets: vec![],
                min_targets: 0,
                max_targets: 0,
            })
        }),
        effect: Arc::new(move |ctx, _, _| {
            if disable {
                ctx.disable_judge(1).unwrap();
            } else {
                ctx.enable_judge(1).unwrap();
            }
            TriggerStep::Done
        }),
    })
    .unwrap()
}
fn submit(game: &mut Game, registration: u64) {
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

#[test]
fn four_reference_trials_match_real_skill_requests_callbacks_and_stages() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/judge-area.json"
    ))
    .unwrap();
    let mut observations = 0;
    for trial in golden["trials"].as_array().unwrap() {
        let trial_name = trial["name"].as_str().unwrap();
        let mut game = scene(
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
        );
        let mut cards = vec![card(1, CardKind::Sha), card(2, CardKind::Zhuge)];
        game.seat_mut(1).hand.push(cards[0]);
        game.seat_mut(1).equipment.install_test(cards[1]);
        if trial_name != "empty" {
            let material = card(3, CardKind::Sha);
            let lightning = card(4, CardKind::Shandian);
            game.seat_mut(0).hand.push(material);
            game.classic.as_mut().unwrap().turn.as_mut().unwrap().seat = 2;
            let program = game
                .virtual_trick_program(0, CardKind::Lebu, vec![material], vec![1])
                .unwrap();
            game.scheduler.start(program);
            while let Some(work) = game.scheduler.poll() {
                match work {
                    crate::scheduler::Work::Started(event, program) => {
                        game.record(event, &program, EventStatus::Started, false)
                    }
                    crate::scheduler::Work::Run(event, program, child) => {
                        game.resolve(event, program, child, &mut None)
                    }
                    crate::scheduler::Work::Ended(event, program, outcome) => game.record(
                        event,
                        &program,
                        if matches!(outcome, crate::scheduler::Outcome::Cancelled) {
                            EventStatus::Cancelled
                        } else {
                            EventStatus::Finished
                        },
                        false,
                    ),
                }
                assert!(game.decision.is_none(), "setup must not prompt");
            }
            game.seat_mut(1).judges.push(lightning);
            cards.extend([material, lightning]);
            game.classic.as_mut().unwrap().turn.as_mut().unwrap().seat = 0;
        }
        let disable = register(&mut game, true);
        let enable = register(&mut game, false);
        game.scheduler.start(Program::event(EventKind::Phase {
            seat: 0,
            phase: Phase::Play,
        }));
        game.run(None);
        let rows = Arc::new(Mutex::new(Vec::new()));
        let shared_cards = cards.clone();
        let log = rows.clone();
        let trial_label = trial_name.to_owned();
        game.timing_observer = Some(Arc::new(move |game, e| {
            if TIMINGS.contains(&e.timing.as_str()) {
                log.lock()
                    .unwrap()
                    .push(row(game, e, &trial_label, "request", &shared_cards));
            }
        }));
        let cancel = Arc::new(Mutex::new(match trial_name {
            "cancel-discard" => Some("discardBefore"),
            "cancel-lose" => Some("loseBefore"),
            _ => None,
        }));
        for &timing in TIMINGS {
            let log = rows.clone();
            let tracked = cards.clone();
            let label = trial_name.to_owned();
            let cancel = cancel.clone();
            game.register_trigger(TriggerRegistration {
                owner: 0,
                skill: format!("probe_{timing}"),
                timing: timing.into(),
                role: TriggerRole::Global,
                order: TriggerOrder::First,
                priority: 0,
                label: None,
                silent: true,
                filter: Arc::new(|_, _, _| true),
                effect: Arc::new(move |ctx, _, _| {
                    log.lock().unwrap().push(row(
                        ctx.game(),
                        ctx.event(),
                        &label,
                        "callback",
                        &tracked,
                    ));
                    let mut cancellation = cancel.lock().unwrap();
                    if *cancellation == Some(ctx.event().timing.as_str()) {
                        *cancellation = None;
                        TriggerStep::CancelSubject
                    } else {
                        TriggerStep::Done
                    }
                }),
            })
            .unwrap();
        }
        let stages = trial["stages"].as_array().unwrap();
        check_stage(&game, &stages[0], &cards);
        let mut discard_statuses = Vec::new();
        let mut loss_statuses = Vec::new();
        for (i, registration) in [disable, disable, enable, enable].iter().enumerate() {
            submit(&mut game, *registration);
            check_stage(&game, &stages[i + 1], &cards);
            discard_statuses.extend(
                game.events
                    .iter()
                    .filter(|e| {
                        matches!(e.kind, EventKind::DiscardGroup { seat: 1, .. })
                            && e.status != EventStatus::Started
                    })
                    .map(|e| e.status),
            );
            loss_statuses.extend(
                game.events
                    .iter()
                    .filter(|e| {
                        matches!(e.kind, EventKind::Lose { seat: 1, .. })
                            && e.status != EventStatus::Started
                    })
                    .map(|e| e.status),
            );
        }
        let expected: Vec<Value> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r[0] == trial_name)
            .map(|r| Value::Array(r.as_array().unwrap()[..12].to_vec()))
            .collect();
        let actual = rows.lock().unwrap();
        assert_eq!(*actual, expected, "reference trial {trial_name}");
        observations += actual.len();
        let losses: Vec<_> = game
            .loss_history()
            .iter()
            .filter(|loss| loss.seat == 1)
            .collect();
        if trial_name == "empty" {
            assert!(losses.is_empty());
        } else {
            assert_eq!(losses.len(), 1, "only committed losses enter history");
            assert_eq!(losses[0].stockcards, [cards[3], cards[2]]);
            assert_eq!(
                losses[0].cards,
                [
                    (crate::Zone::Judgment, cards[3]),
                    (crate::Zone::Judgment, cards[2])
                ]
            );
        }
        match trial_name {
            "empty" => {
                assert!(discard_statuses.is_empty());
                assert!(loss_statuses.is_empty());
            }
            "cancel-discard" => {
                assert_eq!(
                    discard_statuses,
                    [EventStatus::Cancelled, EventStatus::Finished]
                );
                assert_eq!(loss_statuses, [EventStatus::Finished]);
            }
            "cancel-lose" => {
                assert_eq!(
                    discard_statuses,
                    [EventStatus::Finished, EventStatus::Finished]
                );
                assert_eq!(
                    loss_statuses,
                    [EventStatus::Cancelled, EventStatus::Finished]
                );
            }
            _ => {
                assert_eq!(discard_statuses, [EventStatus::Finished]);
                assert_eq!(loss_statuses, [EventStatus::Finished]);
            }
        }
    }
    assert_eq!(observations, 200);
}

#[test]
fn disabled_area_changes_delayed_trick_targets_and_lightning_route() {
    use crate::{classic_data::Suit, CardFace};
    for disabled in [true, false] {
        let mut game = scene(
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
        );
        game.classic.as_mut().unwrap().turn.as_mut().unwrap().seat = 2;
        let lightning = card(20, CardKind::Shandian);
        let material = card(21, CardKind::Sha);
        game.seat_mut(1).judge_disabled = disabled;
        game.seat_mut(1).judge_disabled_marker = disabled;
        game.seat_mut(0).judges.push(lightning);
        game.seat_mut(0).hand.push(material);
        assert_eq!(
            game.virtual_trick_program(0, CardKind::Lebu, vec![material], vec![1])
                .is_ok(),
            !disabled
        );
        game.classic.as_mut().unwrap().deck = vec![card(119, CardKind::Sha)];
        game.classic.as_mut().unwrap().faces.push(CardFace {
            id: 119,
            suit: Suit::Heart,
            rank: 5,
        });
        game.scheduler.start(Program::event(EventKind::Phase {
            seat: 0,
            phase: Phase::Judgment,
        }));
        game.run(None);
        crate::classic::tests::pass_counters(&mut game);
        assert_eq!(
            game.seat(if disabled { 2 } else { 1 }).unwrap().judges,
            [lightning]
        );
        assert_eq!(
            game.card_location(lightning.id).unwrap().owner,
            Some(if disabled { 2 } else { 1 })
        );
    }
}

#[test]
fn real_lebu_use_is_rejected_when_disabled_then_lands_after_restore() {
    let mut game = scene(
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
    );
    let lebu = card(30, CardKind::Lebu);
    game.seat_mut(0).hand.push(lebu);
    let disable = register(&mut game, true);
    let enable = register(&mut game, false);
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    submit(&mut game, disable);
    let action = || Action::PlayCard {
        card: lebu.id,
        targets: vec![1],
    };
    assert!(game
        .submit(Command {
            version: game.version,
            seat: 0,
            action: action()
        })
        .is_err());
    assert!(game.seat(0).unwrap().hand.contains(&lebu));
    submit(&mut game, enable);
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: action(),
    })
    .unwrap();
    crate::classic::tests::pass_counters(&mut game);
    assert_eq!(game.seat(1).unwrap().judges, [lebu]);
    assert!(!game.can_add_judge(1, CardKind::Lebu));
    assert!(game.can_add_judge(1, CardKind::Shandian));
}

mod placement;

mod payment;
