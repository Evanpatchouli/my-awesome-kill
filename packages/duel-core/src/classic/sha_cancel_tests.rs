//! Root Sha cancellation/exclusion contract. The upstream golden samples root
//! cancellation and all-excluded with one target; multi-target, single-target
//! exclusion and direct useCard2/useCard entry points are additional contracts.
use crate::classic::tests::{card, scene};
use crate::classic_data::{Role, Suit};
use crate::{
    Action, Card, CardFace, CardKind, ChoiceKind, Command, EventId, EventKind, EventStatus, Game,
    Phase, Program, SeatId, TriggerContext, TriggerOrder, TriggerRegistration, TriggerRole,
    TriggerStep,
};
use std::sync::{Arc, Mutex};

const SHA: Card = Card {
    id: 7101,
    kind: CardKind::Sha,
};
const SWORD: Card = Card {
    id: 7102,
    kind: CardKind::Qinggang,
};
const FANG: Card = Card {
    id: 7103,
    kind: CardKind::Fangtian,
};
const RED: Card = Card {
    id: 7104,
    kind: CardKind::Tao,
};

#[derive(Clone, Debug)]
struct Row {
    timing: String,
    target: Option<SeatId>,
    subject: EventId,
    parent: Option<EventId>,
    marks: [bool; 2],
    ordering: bool,
}
type Rows = Arc<Mutex<Vec<Row>>>;
fn register(
    game: &mut Game,
    timing: &str,
    order: TriggerOrder,
    effect: impl Fn(&mut TriggerContext<'_>) -> TriggerStep + Send + Sync + 'static,
) {
    game.register_trigger(TriggerRegistration {
        owner: 3,
        skill: format!("root_contract_{timing}_{order:?}"),
        timing: timing.into(),
        role: TriggerRole::Global,
        order,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |ctx, _, _| effect(ctx)),
    })
    .unwrap();
}
fn table() -> (Game, Rows) {
    let mut game = scene(
        &[
            Role::Lord,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Rebel,
        ],
        0,
    );
    game.seat_mut(0).hand.push(SHA);
    game.seat_mut(0).equipment_expanded[0] = 1;
    game.seat_mut(0).equipment.install_test(FANG);
    game.seat_mut(0).equipment.install_test(SWORD);
    for seat in [1, 2] {
        game.seat_mut(seat).hp = 12;
        game.seat_mut(seat).max_hp = 12;
        game.seat_mut(seat)
            .equipment
            .install_test(card(7110 + seat, CardKind::Bagua));
    }
    let classic = game.classic.as_mut().unwrap();
    classic.deck.push(RED);
    classic.faces.push(CardFace {
        id: RED.id,
        suit: Suit::Heart,
        rank: 9,
    });
    let rows: Rows = Arc::new(Mutex::new(Vec::new()));
    let seen = rows.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if matches!(
            event.timing.as_str(),
            "useCard2"
                | "useCard"
                | "useCardToPlayered"
                | "useCardToPlayeredCancelled"
                | "useCardToExcluded"
                | "useCardToExcludedCancelled"
                | "useCardCancelled"
                | "useCardEnd"
                | "useCardAfter"
                | "shaDamage"
                | "judge"
        ) {
            seen.lock().unwrap().push(Row {
                timing: event.timing.clone(),
                target: event.target,
                subject: event.subject,
                parent: event.parent,
                marks: ([1, 2].map(|seat| !game.seat(seat).unwrap().qinggang_uses.is_empty())),
                ordering: game.resolving.contains(&SHA),
            });
        }
    }));
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    (game, rows)
}
fn play(game: &mut Game, targets: &[SeatId]) {
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::PlayCard {
            card: SHA.id,
            targets: targets.to_vec(),
        },
    })
    .unwrap();
}
fn decline(game: &mut Game, seat: SeatId) {
    let choice = game.view(Some(seat)).unwrap().choice.expect("Shan prompt");
    assert_eq!(choice.kind, ChoiceKind::Shan);
    game.submit(Command {
        version: game.version,
        seat,
        action: Action::Choose {
            event: choice.event,
            card: None,
        },
    })
    .unwrap();
}
fn choose_bagua(game: &mut Game, seat: SeatId) {
    let choice = game.view(Some(seat)).unwrap().choice.expect("Bagua prompt");
    let branch = choice
        .branches
        .iter()
        .find(|branch| branch.label.contains("八卦"))
        .unwrap();
    game.submit(Command {
        version: game.version,
        seat,
        action: Action::Select {
            event: choice.event,
            branch: branch.id,
            cards: vec![],
            targets: vec![],
        },
    })
    .unwrap();
}
fn assert_paid_once(game: &Game) {
    assert_eq!(game.discard.iter().filter(|&&card| card == SHA).count(), 1);
    assert!(!game.resolving.contains(&SHA));
    assert!(!game.seat(0).unwrap().hand.contains(&SHA));
}
fn names(rows: &Rows) -> Vec<String> {
    rows.lock()
        .unwrap()
        .iter()
        .map(|row| row.timing.clone())
        .collect()
}

#[test]
fn cancel_root_from_second_target_keeps_prior_mark_and_finishes_paid_material_once() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/qinggang-expiry.json"
    ))
    .unwrap();
    let sample = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            row[0] == "cancel-root" && row[1] == "request" && row[2] == "useCardCancelled"
        })
        .count();
    assert_eq!(sample, 1, "upstream single-target cancellation sample");
    let (mut game, rows) = table();
    register(&mut game, "useCardToPlayered", TriggerOrder::Last, |ctx| {
        if ctx.event().target == Some(2) {
            assert!(ctx.game().resolving.contains(&SHA));
            assert!(!ctx.game().seat(1).unwrap().qinggang_uses.is_empty());
            ctx.cancel_card_use().unwrap();
        }
        TriggerStep::Done
    });
    play(&mut game, &[1, 2]);
    let got = names(&rows);
    assert_eq!(
        got.iter()
            .filter(|&name| name == "useCardCancelled")
            .count(),
        1
    );
    assert!(!got
        .iter()
        .any(|name| matches!(name.as_str(), "useCardEnd" | "useCardAfter")));
    assert!(game.view(None).unwrap().choice.is_none());
    assert_eq!(
        [game.seat(1).unwrap().hp, game.seat(2).unwrap().hp],
        [12, 12]
    );
    assert!(!game.seat(1).unwrap().qinggang_uses.is_empty());
    assert!(!game.seat(2).unwrap().qinggang_uses.is_empty());
    assert!(game.events.iter().any(
        |event| matches!(event.kind, EventKind::UseShaTargets { .. })
            && event.status == EventStatus::Cancelled
    ));
    assert!(rows
        .lock()
        .unwrap()
        .iter()
        .any(|row| row.timing == "useCardCancelled" && row.ordering));
    assert_paid_once(&game);
}

#[test]
fn exclude_all_at_first_target_finishes_root_without_effects_and_keeps_mark() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/qinggang-expiry.json"
    ))
    .unwrap();
    let sample = golden["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == "exclude-all")
        .unwrap();
    assert_eq!(sample[1]["contains"], serde_json::json!([true, false]));
    let (mut game, rows) = table();
    register(&mut game, "useCardToPlayered", TriggerOrder::Last, |ctx| {
        if ctx.event().target == Some(1) {
            assert!(ctx.game().resolving.contains(&SHA));
            ctx.exclude_all_card_use_targets().unwrap();
            ctx.exclude_all_card_use_targets().unwrap();
        }
        TriggerStep::Done
    });
    play(&mut game, &[1, 2]);
    let got = names(&rows);
    assert!(!got
        .iter()
        .any(|name| name == "useCardCancelled" || name == "useCardToExcluded"));
    assert_eq!(got.iter().filter(|&name| name == "useCardEnd").count(), 1);
    assert_eq!(got.iter().filter(|&name| name == "useCardAfter").count(), 1);
    assert!(!game
        .events
        .iter()
        .any(|event| matches!(event.kind, EventKind::ShaEffect { .. })));
    assert_eq!(
        [game.seat(1).unwrap().hp, game.seat(2).unwrap().hp],
        [12, 12]
    );
    assert!(!game.seat(1).unwrap().qinggang_uses.is_empty());
    assert_paid_once(&game);
}

#[test]
fn cancelling_only_target_timing_still_allows_armor_and_other_target() {
    let (mut game, rows) = table();
    register(&mut game, "useCardToPlayered", TriggerOrder::First, |ctx| {
        if ctx.event().target == Some(1) {
            TriggerStep::CancelSubject
        } else {
            TriggerStep::Done
        }
    });
    play(&mut game, &[1, 2]);
    choose_bagua(&mut game, 1);
    decline(&mut game, 2);
    assert_eq!(
        [game.seat(1).unwrap().hp, game.seat(2).unwrap().hp],
        [12, 11]
    );
    assert!(game.discard.contains(&RED), "real red Bagua judgment");
    assert!(names(&rows).contains(&"useCardToPlayeredCancelled".into()));
    assert!(!names(&rows).contains(&"useCardCancelled".into()));
    assert_paid_once(&game);
}

#[test]
fn exclude_one_visits_excluded_target_and_cleans_its_mark_before_other_effect() {
    let (mut game, rows) = table();
    register(&mut game, "useCardToExcluded", TriggerOrder::Last, |ctx| {
        assert!(
            ctx.cancel_card_use().is_err(),
            "direct target child is already in effect traversal"
        );
        assert!(ctx.exclude_card_use_target(2).is_err());
        assert!(ctx.exclude_all_card_use_targets().is_err());
        TriggerStep::Done
    });
    register(&mut game, "useCardToPlayered", TriggerOrder::Last, |ctx| {
        if ctx.event().target == Some(1) {
            ctx.exclude_card_use_target(1).unwrap();
            ctx.exclude_card_use_target(1).unwrap();
        }
        TriggerStep::Done
    });
    play(&mut game, &[1, 2]);
    decline(&mut game, 2);
    let captured = rows.lock().unwrap();
    let excluded: Vec<_> = captured
        .iter()
        .filter(|row| row.timing == "useCardToExcluded")
        .collect();
    assert_eq!(excluded.len(), 1);
    assert_eq!(excluded[0].target, Some(1));
    assert!(excluded[0].marks[0]);
    assert!(excluded[0].parent.is_some());
    let excluded_at = captured
        .iter()
        .position(|row| row.timing == "useCardToExcluded")
        .unwrap();
    let damage_at = captured
        .iter()
        .position(|row| row.timing == "shaDamage")
        .unwrap();
    assert!(excluded_at < damage_at);
    assert!(!game
        .seat(1)
        .unwrap()
        .qinggang_uses
        .contains(&excluded[0].parent.unwrap()));
    assert!(!game.has_skill(1, "qinggang2"));
    assert_eq!(
        [game.seat(1).unwrap().hp, game.seat(2).unwrap().hp],
        [12, 11]
    );
    assert_paid_once(&game);
}

#[test]
fn direct_root_timings_accept_cancel_and_single_exclusion() {
    // useCard2 and useCard are API contracts; the upstream sample uses a target callback.
    for timing in ["useCard2", "useCard"] {
        let (mut game, rows) = table();
        register(&mut game, timing, TriggerOrder::Last, |ctx| {
            assert!(ctx.game().resolving.contains(&SHA));
            ctx.cancel_card_use().unwrap();
            ctx.cancel_card_use().unwrap();
            TriggerStep::Done
        });
        play(&mut game, &[1]);
        let got = names(&rows);
        assert_eq!(
            got.iter()
                .filter(|&name| name == "useCardCancelled")
                .count(),
            1
        );
        assert!(!got
            .iter()
            .any(|name| name == "useCardEnd" || name == "useCardAfter"));
        assert_paid_once(&game);

        let (mut game, rows) = table();
        register(&mut game, timing, TriggerOrder::Last, |ctx| {
            assert!(ctx.exclude_card_use_target(2).is_err(), "non-member target");
            ctx.exclude_card_use_target(1).unwrap();
            ctx.exclude_card_use_target(1).unwrap();
            TriggerStep::Done
        });
        play(&mut game, &[1]);
        let got = names(&rows);
        assert_eq!(
            got.iter()
                .filter(|&name| name == "useCardToExcluded")
                .count(),
            1
        );
        assert!(got.contains(&"useCardEnd".into()) && got.contains(&"useCardAfter".into()));
        assert_eq!(game.seat(1).unwrap().hp, 12);
        assert_paid_once(&game);
    }
}

#[test]
fn operations_reject_non_sha_wrong_parent_and_effect_or_end_timing() {
    let (mut game, _) = table();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mark = seen.clone();
    register(&mut game, "damageBegin1", TriggerOrder::Last, move |ctx| {
        assert!(
            ctx.cancel_card_use().is_err(),
            "damage is not the direct use target child"
        );
        assert!(ctx.exclude_card_use_target(1).is_err());
        assert!(ctx.exclude_all_card_use_targets().is_err());
        mark.lock().unwrap().push("damage");
        TriggerStep::Done
    });
    let mark = seen.clone();
    register(&mut game, "useCardEnd", TriggerOrder::Last, move |ctx| {
        assert!(ctx.cancel_card_use().is_err(), "use has entered effects");
        assert!(ctx.exclude_card_use_target(1).is_err());
        assert!(ctx.exclude_all_card_use_targets().is_err());
        mark.lock().unwrap().push("end");
        TriggerStep::Done
    });
    play(&mut game, &[1]);
    decline(&mut game, 1);
    assert_eq!(*seen.lock().unwrap(), ["damage", "end"]);
    assert_eq!(game.seat(1).unwrap().hp, 11);
    assert_paid_once(&game);

    let (mut game, _) = table();
    let tao = card(7120, CardKind::Tao);
    game.seat_mut(0).hand.push(tao);
    game.seat_mut(0).hp = 3;
    let seen = Arc::new(Mutex::new(false));
    let checked = seen.clone();
    register(&mut game, "useCard2", TriggerOrder::Last, move |ctx| {
        if ctx.event().card_kind == Some(CardKind::Tao) {
            assert!(ctx.cancel_card_use().is_err());
            assert!(ctx.exclude_card_use_target(0).is_err());
            assert!(ctx.exclude_all_card_use_targets().is_err());
            *checked.lock().unwrap() = true;
        }
        TriggerStep::Done
    });
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::PlayCard {
            card: tao.id,
            targets: vec![0],
        },
    })
    .unwrap();
    assert!(*seen.lock().unwrap(), "non-Sha useCard2 was exercised");
}

#[test]
fn cancelling_nested_sha_does_not_cancel_outer_sha_or_its_marks() {
    let (mut game, rows) = table();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = seen.clone();
    register(
        &mut game,
        "useCardToPlayered",
        TriggerOrder::Last,
        move |ctx| {
            let event = ctx.event();
            if event.player == Some(0) && event.target == Some(1) {
                observed
                    .lock()
                    .unwrap()
                    .push(("outer", event.parent.unwrap()));
                ctx.use_virtual_sha(2, 1, None).unwrap();
            } else if event.player == Some(2) && event.target == Some(1) {
                observed
                    .lock()
                    .unwrap()
                    .push(("inner", event.parent.unwrap()));
                ctx.cancel_card_use().unwrap();
            }
            TriggerStep::Done
        },
    );
    play(&mut game, &[1, 2]);
    decline(&mut game, 1);
    decline(&mut game, 2);
    let scopes = seen.lock().unwrap();
    assert_eq!(scopes.len(), 2);
    assert_ne!(scopes[0].1, scopes[1].1);
    let trace = rows.lock().unwrap();
    assert_eq!(
        trace
            .iter()
            .filter(|row| row.timing == "useCardCancelled" && row.subject == scopes[1].1)
            .count(),
        1
    );
    assert_eq!(
        trace
            .iter()
            .filter(|row| row.timing == "useCardAfter" && row.subject == scopes[0].1)
            .count(),
        1
    );
    assert_eq!(
        [game.seat(1).unwrap().hp, game.seat(2).unwrap().hp],
        [11, 11]
    );
    assert_paid_once(&game);
}

#[test]
fn root_subject_cancel_takes_precedence_over_earlier_all_excluded() {
    let (mut game, rows) = table();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let first = calls.clone();
    register(&mut game, "useCard", TriggerOrder::First, move |ctx| {
        assert!(ctx.game().resolving.contains(&SHA));
        ctx.exclude_all_card_use_targets().unwrap();
        first.lock().unwrap().push("exclude-all");
        TriggerStep::Done
    });
    let last = calls.clone();
    register(&mut game, "useCard", TriggerOrder::Last, move |ctx| {
        assert!(ctx.game().resolving.contains(&SHA));
        last.lock().unwrap().push("cancel-subject");
        TriggerStep::CancelSubject
    });
    play(&mut game, &[1, 2]);
    assert_eq!(*calls.lock().unwrap(), ["exclude-all", "cancel-subject"]);
    assert!(game.events.iter().any(
        |event| matches!(event.kind, EventKind::UseShaTargets { .. })
            && event.status == EventStatus::Cancelled
    ));
    let timing = names(&rows);
    assert_eq!(
        timing
            .iter()
            .filter(|name| name.as_str() == "useCard")
            .count(),
        1
    );
    assert!(!timing.iter().any(|name| matches!(
        name.as_str(),
        "useCardEnd" | "useCardAfter" | "useCardToPlayered" | "useCardToExcluded"
    )));
    assert_eq!(
        [game.seat(1).unwrap().hp, game.seat(2).unwrap().hp],
        [12, 12]
    );
    assert_paid_once(&game);
}

#[test]
fn excluded_child_cancellation_emits_its_cancelled_timing_and_other_target_continues() {
    let (mut game, rows) = table();
    register(&mut game, "useCardToPlayered", TriggerOrder::Last, |ctx| {
        if ctx.event().target == Some(1) {
            ctx.exclude_card_use_target(1).unwrap();
        }
        TriggerStep::Done
    });
    register(&mut game, "useCardToExcluded", TriggerOrder::First, |ctx| {
        assert_eq!(ctx.event().target, Some(1));
        TriggerStep::CancelSubject
    });
    play(&mut game, &[1, 2]);
    decline(&mut game, 2);
    let trace = rows.lock().unwrap();
    let original = trace
        .iter()
        .find(|row| row.timing == "useCardToExcluded")
        .unwrap();
    let cancelled: Vec<_> = trace
        .iter()
        .filter(|row| row.timing == "useCardToExcludedCancelled")
        .collect();
    assert_eq!(cancelled.len(), 1);
    assert_eq!(
        (
            cancelled[0].subject,
            cancelled[0].parent,
            cancelled[0].target
        ),
        (original.subject, original.parent, Some(1))
    );
    assert_eq!(
        trace
            .iter()
            .filter(|row| row.timing == "useCardEnd")
            .count(),
        1
    );
    assert_eq!(
        trace
            .iter()
            .filter(|row| row.timing == "useCardAfter")
            .count(),
        1
    );
    assert!(!trace.iter().any(|row| row.timing == "useCardCancelled"));
    assert_eq!(
        [game.seat(1).unwrap().hp, game.seat(2).unwrap().hp],
        [12, 11]
    );
    assert_paid_once(&game);
}

#[test]
fn root_cancel_request_and_callback_order_matches_nine_reference_trials() {
    use serde_json::{json, Value};
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/sha-exclusion.json"
    ))
    .unwrap();
    for timing in ["useCard2", "useCard", "useCardToPlayered"] {
        for (group, order) in [
            ("first", TriggerOrder::First),
            ("normal", TriggerOrder::Normal),
            ("last", TriggerOrder::Last),
        ] {
            let (mut game, _) = table();
            let trace = Arc::new(Mutex::new(Vec::new()));
            let record = |game: &Game, e: &crate::TriggerEvent, name: &str| {
                json!([
                    name,
                    e.timing,
                    e.target
                        .map(|seat| ((b'A' + seat as u8) as char).to_string()),
                    ([1, 2].map(|seat| !game.seat(seat).unwrap().qinggang_uses.is_empty())),
                    game.resolving.contains(&SHA),
                    e.subject,
                    e.parent,
                    e.cards,
                ])
            };
            let observed = trace.clone();
            game.timing_observer = Some(Arc::new(move |game, e| {
                if e.timing == timing || e.timing == "useCardCancelled" {
                    observed.lock().unwrap().push(record(game, e, "request"));
                }
            }));
            for (label, callback_order, priority) in [
                ("first-high", TriggerOrder::First, 15),
                ("first-low", TriggerOrder::First, -15),
                ("normal-high", TriggerOrder::Normal, 15),
                ("normal-low", TriggerOrder::Normal, -15),
                ("last-high", TriggerOrder::Last, 15),
                ("last-low", TriggerOrder::Last, -15),
            ] {
                for event_timing in [timing, "useCardCancelled"] {
                    let observed = trace.clone();
                    game.register_trigger(TriggerRegistration {
                        owner: 3,
                        skill: format!("order_{label}_{event_timing}"),
                        timing: event_timing.into(),
                        role: TriggerRole::Global,
                        order: callback_order,
                        priority,
                        label: None,
                        silent: true,
                        filter: Arc::new(|_, _, _| true),
                        effect: Arc::new(move |ctx, _, _| {
                            observed
                                .lock()
                                .unwrap()
                                .push(record(ctx.game(), ctx.event(), label));
                            TriggerStep::Done
                        }),
                    })
                    .unwrap();
                }
            }
            let observed = trace.clone();
            game.register_trigger(TriggerRegistration {
                owner: 3,
                skill: "cancel_twice".into(),
                timing: timing.into(),
                role: TriggerRole::Global,
                order,
                priority: 0,
                label: None,
                silent: true,
                filter: Arc::new(|_, e, _| e.target.is_none() || e.target == Some(1)),
                effect: Arc::new(move |ctx, _, _| {
                    observed.lock().unwrap().push(record(
                        ctx.game(),
                        ctx.event(),
                        &format!("cancel-{group}"),
                    ));
                    ctx.cancel_card_use().unwrap();
                    ctx.cancel_card_use().unwrap();
                    TriggerStep::Done
                }),
            })
            .unwrap();
            play(&mut game, &[1, 2]);
            let trial = format!("cancel-{timing}-{group}");
            let expected: Vec<_> = golden["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row[0] == trial && (row[2] == timing || row[2] == "useCardCancelled"))
                .map(|row| {
                    json!([
                        row[1],
                        row[2],
                        row[9],
                        row[13]["contains"],
                        row[13]["cards"][format!("{trial}-sha")] == "o"
                    ])
                })
                .collect();
            let actual = trace.lock().unwrap();
            let common: Vec<_> = actual
                .iter()
                .map(|row| json!([row[0], row[1], row[2], row[3], row[4]]))
                .collect();
            assert_eq!(common, expected, "{trial} request/callback ordering");
            let cancelled = actual
                .iter()
                .find(|row| row[1] == "useCardCancelled" && row[0] == "request")
                .unwrap();
            let root = cancelled[5].as_u64().unwrap();
            for row in actual.iter() {
                assert_eq!(row[7], json!([SHA]), "{trial} actual Sha material");
                if row[1] == "useCardToPlayered" {
                    assert_eq!(row[6].as_u64(), Some(root));
                } else {
                    assert_eq!(row[5].as_u64(), Some(root));
                }
            }
            drop(actual);
            assert_paid_once(&game);
        }
    }
}

#[test]
fn root_cancel_request_is_immediate_even_without_registered_cancelled_callbacks() {
    let (mut game, _) = table();
    let trace = Arc::new(Mutex::new(Vec::new()));
    let observed = trace.clone();
    game.timing_observer = Some(Arc::new(move |_, e| {
        if e.timing == "useCardCancelled" {
            observed.lock().unwrap().push("request");
        }
    }));
    let observed = trace.clone();
    register(
        &mut game,
        "useCardToPlayered",
        TriggerOrder::First,
        move |ctx| {
            observed.lock().unwrap().push("before");
            ctx.cancel_card_use().unwrap();
            ctx.cancel_card_use().unwrap();
            observed.lock().unwrap().push("after");
            TriggerStep::Done
        },
    );
    play(&mut game, &[1]);
    assert_eq!(*trace.lock().unwrap(), ["before", "request", "after"]);
    assert_paid_once(&game);
}

#[test]
fn root_cancel_request_expires_temporary_skill_before_next_same_group_callback() {
    let (mut game, _) = table();
    game.grant_temporary(
        1,
        crate::TemporarySkill::new(
            "expires_on_cancel",
            vec![crate::SkillExpiry::new(
                TriggerRole::Global,
                "useCardCancelled",
            )],
        ),
    )
    .unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = seen.clone();
    game.register_trigger(TriggerRegistration {
        owner: 3,
        skill: "cancel_for_expiry".into(),
        timing: "useCardToPlayered".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 10,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |ctx, _, _| {
            assert!(ctx.game().has_skill(1, "expires_on_cancel"));
            ctx.cancel_card_use().unwrap();
            TriggerStep::Done
        }),
    })
    .unwrap();
    game.register_trigger(TriggerRegistration {
        owner: 3,
        skill: "observe_expiry".into(),
        timing: "useCardToPlayered".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: -10,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |ctx, _, _| {
            observed
                .lock()
                .unwrap()
                .push(ctx.game().has_skill(1, "expires_on_cancel"));
            TriggerStep::Done
        }),
    })
    .unwrap();
    play(&mut game, &[1]);
    assert_eq!(*seen.lock().unwrap(), [false]);
    assert_paid_once(&game);
}

#[test]
fn nested_sha_cancel_request_is_immediate_and_scoped_to_inner_root() {
    let (mut game, _) = table();
    let trace = Arc::new(Mutex::new(Vec::new()));
    let observed = trace.clone();
    game.timing_observer = Some(Arc::new(move |_, event| {
        if event.timing == "useCardCancelled" {
            observed.lock().unwrap().push(("request", event.subject));
        }
    }));
    let roots = Arc::new(Mutex::new(Vec::new()));
    let captured = roots.clone();
    let observed = trace.clone();
    register(
        &mut game,
        "useCardToPlayered",
        TriggerOrder::First,
        move |ctx| {
            let event = ctx.event();
            if event.target != Some(1) {
                return TriggerStep::Done;
            }
            let root = event.parent.unwrap();
            if event.player == Some(0) {
                captured.lock().unwrap().push(("outer", root));
                ctx.use_virtual_sha(2, 1, None).unwrap();
            } else if event.player == Some(2) {
                captured.lock().unwrap().push(("inner", root));
                observed.lock().unwrap().push(("before", root));
                ctx.cancel_card_use().unwrap();
                ctx.cancel_card_use().unwrap();
                observed.lock().unwrap().push(("after", root));
            }
            TriggerStep::Done
        },
    );
    play(&mut game, &[1, 2]);
    let roots = roots.lock().unwrap();
    assert_eq!(roots.len(), 2);
    assert_ne!(roots[0].1, roots[1].1);
    assert_eq!(
        *trace.lock().unwrap(),
        [
            ("before", roots[1].1),
            ("request", roots[1].1),
            ("after", roots[1].1)
        ]
    );
    drop(roots);
    decline(&mut game, 1);
    decline(&mut game, 2);
    assert_eq!(
        [game.seat(1).unwrap().hp, game.seat(2).unwrap().hp],
        [11, 11]
    );
    assert_paid_once(&game);
}

#[test]
fn direct_root_cancel_subject_requests_once_before_later_dispatch_callbacks() {
    // Rust contract: the upstream fixture calls root.cancel(), not this direct subject operation.
    let (mut game, _) = table();
    let trace = Arc::new(Mutex::new(Vec::new()));
    let observed = trace.clone();
    game.timing_observer = Some(Arc::new(move |_, event| {
        if event.timing == "useCardCancelled" {
            observed.lock().unwrap().push("request");
        }
    }));
    let observed = trace.clone();
    game.register_trigger(TriggerRegistration {
        owner: 3,
        skill: "direct_root_cancel".into(),
        timing: "useCard".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 10,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |_, _, _| {
            observed.lock().unwrap().push("cancel");
            TriggerStep::CancelSubject
        }),
    })
    .unwrap();
    let observed = trace.clone();
    game.register_trigger(TriggerRegistration {
        owner: 3,
        skill: "direct_root_later".into(),
        timing: "useCard".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: -10,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |_, _, _| {
            observed.lock().unwrap().push("later");
            TriggerStep::Done
        }),
    })
    .unwrap();
    play(&mut game, &[1]);
    assert_eq!(*trace.lock().unwrap(), ["cancel", "request"]);
    assert_paid_once(&game);
}

fn setup_active(
    game: &mut Game,
    name: &str,
    effect: impl Fn(&mut TriggerContext<'_>) + Send + Sync + 'static,
) -> u64 {
    game.register_active_skill(crate::ActiveSkillRegistration {
        owner: 0,
        skill: name.into(),
        limited: false,
        choice: Arc::new(|_, _| {
            Some(crate::ChoiceBranch {
                id: 0,
                label: "setup".into(),
                cards: vec![],
                min_cards: 0,
                max_cards: 0,
                targets: vec![],
                min_targets: 0,
                max_targets: 0,
            })
        }),
        effect: Arc::new(move |ctx, _, _| {
            effect(ctx);
            TriggerStep::Done
        }),
    })
    .unwrap()
}
fn register_owner(
    game: &mut Game,
    owner: SeatId,
    timing: &str,
    order: TriggerOrder,
    effect: impl Fn(&mut TriggerContext<'_>) -> TriggerStep + Send + Sync + 'static,
) {
    game.register_trigger(TriggerRegistration {
        owner,
        skill: format!("owner_{owner}_{timing}_{order:?}"),
        timing: timing.into(),
        role: TriggerRole::Global,
        order,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |ctx, _, _| effect(ctx)),
    })
    .unwrap();
}
fn activate_setup(game: &mut Game, registration: u64) {
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
fn cancelled_listener_out_at_request_can_run_after_real_rest_end() {
    // New engine contract; no upstream golden has yet sampled this scenario.
    let (mut game, _) = table();
    let trace = Arc::new(Mutex::new(Vec::new()));
    let callbacks = trace.clone();
    let listener = game
        .register_trigger(TriggerRegistration {
            owner: 3,
            skill: "resting_cancel_listener".into(),
            timing: "useCardCancelled".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::Normal,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, event, _| event.card_kind == Some(CardKind::Sha)),
            effect: Arc::new(move |ctx, _, _| {
                let event = ctx.event();
                assert!(ctx.game().is_in(3));
                assert!(ctx.game().resolving.contains(&SHA));
                assert!(matches!(
                    ctx.game().event_kind(event.subject),
                    Some(EventKind::UseSha { seat: 0, .. })
                ));
                assert!(!ctx.game().seat(1).unwrap().qinggang_uses.is_empty());
                callbacks
                    .lock()
                    .unwrap()
                    .push(("callback", event.subject, event.parent));
                TriggerStep::Done
            }),
        })
        .unwrap();
    assert!(!game.force_out_triggers.contains(&listener));
    let rest = setup_active(&mut game, "rest_listener", |ctx| {
        ctx.rest(3, crate::RestClock::default()).unwrap()
    });
    activate_setup(&mut game, rest);
    assert!(game.is_out(3) && game.is_alive(3) && game.rest_clock(3).is_some());
    let requested = trace.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.timing == "useCardCancelled" {
            assert!(game.is_out(3), "the request happens before restEnd");
            assert!(game.resolving.contains(&SHA));
            assert!(game.seat(1).unwrap().qinggang_uses.is_empty());
            assert!(event.parent.is_some());
            assert_ne!(event.parent, Some(event.subject));
            requested
                .lock()
                .unwrap()
                .push(("request", event.subject, event.parent));
        }
    }));
    let first = trace.clone();
    register_owner(
        &mut game,
        0,
        "useCardToPlayered",
        TriggerOrder::First,
        move |ctx| {
            if ctx.event().target == Some(1) {
                assert!(ctx.game().is_out(3));
                ctx.cancel_card_use().unwrap();
                first
                    .lock()
                    .unwrap()
                    .push(("first", ctx.event().parent.unwrap(), None));
            }
            TriggerStep::Done
        },
    );
    let last = trace.clone();
    register_owner(
        &mut game,
        0,
        "useCardToPlayered",
        TriggerOrder::Last,
        move |ctx| {
            if ctx.event().target == Some(1) {
                assert!(ctx.game().is_out(3));
                ctx.rest_end(3, None).unwrap();
                last.lock()
                    .unwrap()
                    .push(("rest-end-queued", ctx.event().parent.unwrap(), None));
            }
            TriggerStep::Done
        },
    );
    play(&mut game, &[1]);
    let rows = trace.lock().unwrap();
    assert!(game.is_in(3) && game.rest_clock(3).is_none());
    assert!(!game.seat(1).unwrap().qinggang_uses.is_empty());
    assert!(game
        .events
        .iter()
        .any(|event| event.id == rows[0].1 && event.status == EventStatus::Cancelled));
    assert_paid_once(&game);
    assert_eq!(
        rows.iter().map(|row| row.0).collect::<Vec<_>>(),
        ["request", "first", "rest-end-queued", "callback"]
    );
    assert_eq!(rows[0].1, rows[3].1);
    assert_eq!(rows[0].2, rows[3].2);
}

#[test]
fn queued_expansion_gain_and_cancel_cleanup_follow_call_order() {
    // The reverse-order result is specific to gain_expansion: unlike upstream's
    // general gain operation it cannot recover material from the discard pile.
    for gain_first in [false, true] {
        let (mut game, _) = table();
        let material = card(7190, CardKind::Tao);
        game.seat_mut(1).hand.push(material);
        let setup = setup_active(&mut game, "queue_expansion", move |ctx| {
            ctx.add_to_expansion(1, material.id, "cancel-queue")
                .unwrap();
        });
        activate_setup(&mut game, setup);
        assert_eq!(game.expansions(1, "cancel-queue"), [material]);
        let mut temporary = crate::TemporarySkill::new(
            "expire_with_cleanup",
            vec![crate::SkillExpiry::new(
                TriggerRole::Global,
                "useCardCancelled",
            )],
        );
        temporary.on_remove = Some(Arc::new(|cleanup| {
            cleanup.discard_expansions("cancel-queue")
        }));
        game.grant_temporary(1, temporary).unwrap();
        let trace = Arc::new(Mutex::new(Vec::new()));
        let observed = trace.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if event.timing == "useCardCancelled" {
                assert!(
                    game.has_skill(1, "expire_with_cleanup"),
                    "observer sees the skill before request expiry"
                );
                assert_eq!(game.expansions(1, "cancel-queue"), [material]);
                observed.lock().unwrap().push("request");
            }
        }));
        let callback = trace.clone();
        register(
            &mut game,
            "useCardToPlayered",
            TriggerOrder::First,
            move |ctx| {
                if ctx.event().target != Some(1) {
                    return TriggerStep::Done;
                }
                if gain_first {
                    ctx.gain_expansion(1, 2, material.id).unwrap();
                }
                ctx.cancel_card_use().unwrap();
                assert!(!ctx.game().has_skill(1, "expire_with_cleanup"));
                assert_eq!(
                    ctx.game().expansions(1, "cancel-queue"),
                    [material],
                    "both movements are still queued"
                );
                if !gain_first {
                    ctx.gain_expansion(1, 2, material.id).unwrap();
                }
                callback.lock().unwrap().push("callback-done");
                TriggerStep::Done
            },
        );
        play(&mut game, &[1]);
        assert_eq!(*trace.lock().unwrap(), ["request", "callback-done"]);
        assert!(!game.has_skill(1, "expire_with_cleanup"));
        assert!(game.expansions(1, "cancel-queue").is_empty());
        let gain = game.events.iter().position(|event| {
            matches!(event.kind, EventKind::Gain { seat: 2, .. })
                && event.status == EventStatus::Started
        });
        let cleanup = game
            .events
            .iter()
            .position(|event| {
                matches!(event.kind, EventKind::LoseToDiscardpile { seat: 1, .. })
                    && event.status == EventStatus::Started
            })
            .expect("real cleanup child");
        if gain_first {
            assert!(
                gain.expect("valid expansion gain has a child") < cleanup,
                "earlier gain must resolve before expiry cleanup"
            );
            assert!(game.seat(2).unwrap().hand.contains(&material));
            assert!(!game.discard.contains(&material));
        } else {
            // Revalidation happens after cleanup. gain_expansion only accepts
            // material still in this expansion, so it cannot create a gain.
            assert!(
                gain.is_none(),
                "discarded material is no longer in the expansion"
            );
            assert!(!game.seat(2).unwrap().hand.contains(&material));
            assert_eq!(
                game.discard
                    .iter()
                    .filter(|&&card| card == material)
                    .count(),
                1
            );
        }
        let physical = game
            .seats
            .iter()
            .map(|p| {
                p.hand
                    .iter()
                    .chain(&p.equipment)
                    .chain(&p.judges)
                    .chain(&p.expansions)
                    .filter(|&&card| card == material)
                    .count()
            })
            .sum::<usize>()
            + game
                .discard
                .iter()
                .chain(&game.resolving)
                .chain(&game.transferring)
                .filter(|&&card| card == material)
                .count();
        assert_eq!(physical, 1);
        assert_paid_once(&game);
    }
}

#[test]
fn deferred_cancel_rebinds_out_indexed_registration_and_freezes_first_round() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let (mut game, _) = table();
    let rest = setup_active(&mut game, "deferred_index_rest", |ctx| {
        ctx.rest(3, crate::RestClock::default()).unwrap()
    });
    activate_setup(&mut game, rest);
    assert!(game.is_out(3));
    let original_samples = Arc::new(AtomicUsize::new(0));
    let sampled = original_samples.clone();
    let old = game
        .register_indexed_trigger(
            TriggerRegistration {
                owner: 3,
                skill: "deferred_seed".into(),
                timing: "useCardCancelled".into(),
                role: TriggerRole::Global,
                order: TriggerOrder::Normal,
                priority: 4,
                label: None,
                silent: true,
                filter: Arc::new(|_, _, _| true),
                effect: Arc::new(|_, _, _| panic!("unregistered callback must not execute")),
            },
            Arc::new(move |game, _, owner| {
                assert!(
                    game.is_out(owner),
                    "count is sampled at the request, while owner is out"
                );
                sampled.fetch_add(1, Ordering::SeqCst);
                2
            }),
        )
        .unwrap();
    let trace = Arc::new(Mutex::new(Vec::new()));
    let new_samples = Arc::new(AtomicUsize::new(0));
    let installed_dynamic = Arc::new(AtomicBool::new(false));
    register_owner(
        &mut game,
        0,
        "useCardToPlayered",
        TriggerOrder::First,
        |ctx| {
            if ctx.event().target == Some(1) {
                ctx.cancel_card_use().unwrap();
            }
            TriggerStep::Done
        },
    );
    let seen = trace.clone();
    let resampled = new_samples.clone();
    let installed = installed_dynamic.clone();
    register_owner(
        &mut game,
        0,
        "useCardToPlayered",
        TriggerOrder::Last,
        move |ctx| {
            if ctx.event().target != Some(1) {
                return TriggerStep::Done;
            }
            assert!(ctx.unregister_trigger(old));
            let seen_seed = seen.clone();
            let dynamic_once = installed.clone();
            let sampled_new = resampled.clone();
            ctx.register_indexed_trigger(
                TriggerRegistration {
                    owner: 3,
                    skill: "deferred_seed".into(),
                    timing: "useCardCancelled".into(),
                    role: TriggerRole::Global,
                    order: TriggerOrder::Normal,
                    priority: -20,
                    label: None,
                    silent: true,
                    filter: Arc::new(|game, _, owner| game.is_in(owner)),
                    effect: Arc::new(move |ctx, _, _| {
                        assert!(ctx.game().is_in(3));
                        seen_seed.lock().unwrap().push("seed");
                        if !dynamic_once.swap(true, Ordering::SeqCst) {
                            let seen_dynamic = seen_seed.clone();
                            ctx.register_trigger(TriggerRegistration {
                                owner: 0,
                                skill: "during_dispatch".into(),
                                timing: "useCardCancelled".into(),
                                role: TriggerRole::Global,
                                order: TriggerOrder::Last,
                                priority: 0,
                                label: None,
                                silent: true,
                                filter: Arc::new(|_, _, _| true),
                                effect: Arc::new(move |_, _, _| {
                                    seen_dynamic.lock().unwrap().push("dynamic");
                                    TriggerStep::Done
                                }),
                            })
                            .unwrap();
                        }
                        TriggerStep::Done
                    }),
                },
                Arc::new(move |_, _, _| {
                    sampled_new.fetch_add(1, Ordering::SeqCst);
                    9
                }),
            )
            .unwrap();
            let intruder = seen.clone();
            ctx.register_trigger(TriggerRegistration {
                owner: 0,
                skill: "after_request_before_dispatch".into(),
                timing: "useCardCancelled".into(),
                role: TriggerRole::Global,
                order: TriggerOrder::Last,
                priority: 0,
                label: None,
                silent: true,
                filter: Arc::new(|_, _, _| true),
                effect: Arc::new(move |_, _, _| {
                    intruder.lock().unwrap().push("intruder");
                    TriggerStep::Done
                }),
            })
            .unwrap();
            ctx.rest_end(3, None).unwrap();
            TriggerStep::Done
        },
    );
    play(&mut game, &[1]);
    assert_eq!(original_samples.load(Ordering::SeqCst), 1);
    assert_eq!(
        new_samples.load(Ordering::SeqCst),
        0,
        "rebound seed keeps original sampled count"
    );
    assert_eq!(*trace.lock().unwrap(), ["seed", "seed", "dynamic"]);
    assert_paid_once(&game);
}

fn deferred_reference_row(
    game: &Game,
    event: &crate::TriggerEvent,
    kind: &str,
) -> serde_json::Value {
    let seat = |id: Option<SeatId>| id.map(|id| ((b'A' + id as u8) as char).to_string());
    serde_json::json!([
        kind,
        event.timing,
        seat(event.player),
        seat(event.target),
        game.is_out(3),
        !game.has_skill(1, "reference_deferred_expire"),
        game.has_skill(1, "qinggang2"),
        game.resolving.contains(&SHA)
    ])
}
fn deferred_reference_registration(
    trace: Arc<Mutex<Vec<serde_json::Value>>>,
    skill: &str,
    label: &'static str,
) -> TriggerRegistration {
    TriggerRegistration {
        owner: 3,
        skill: skill.into(),
        timing: "useCardCancelled".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |ctx, _, _| {
            trace
                .lock()
                .unwrap()
                .push(deferred_reference_row(ctx.game(), ctx.event(), label));
            TriggerStep::Done
        }),
    }
}

#[test]
fn deferred_cancel_listener_and_index_match_upstream_request_and_callback_fields() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/cancel-deferred.json"
    ))
    .unwrap();
    for trial in [
        "out-return",
        "new-listener",
        "index-change",
        "index-remove",
        "index-readd",
        "index-out-return",
    ] {
        let (mut game, _) = table();
        let trace = Arc::new(Mutex::new(Vec::new()));
        let count = Arc::new(AtomicUsize::new(2));
        let original_id = if trial == "out-return" {
            Some(
                game.register_trigger(deferred_reference_registration(
                    trace.clone(),
                    "reference_deferred_existing",
                    "existing",
                ))
                .unwrap(),
            )
        } else if trial.starts_with("index-") {
            let index_trace = trace.clone();
            let sampled = count.clone();
            Some(
                game.register_indexed_trigger(
                    deferred_reference_registration(
                        trace.clone(),
                        "reference_deferred_index",
                        "indexed",
                    ),
                    Arc::new(move |game, event, _| {
                        index_trace
                            .lock()
                            .unwrap()
                            .push(deferred_reference_row(game, event, "getIndex"));
                        sampled.load(Ordering::SeqCst) as u32
                    }),
                )
                .unwrap(),
            )
        } else {
            None
        };
        if trial == "out-return" || trial == "index-out-return" {
            let rest = setup_active(&mut game, "reference_deferred_rest", |ctx| {
                ctx.rest(3, crate::RestClock::default()).unwrap()
            });
            activate_setup(&mut game, rest);
        }
        game.grant_temporary(
            1,
            crate::TemporarySkill::new(
                "reference_deferred_expire",
                vec![crate::SkillExpiry::new(
                    TriggerRole::Global,
                    "useCardCancelled",
                )],
            ),
        )
        .unwrap();
        let requests = trace.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if matches!(
                event.timing.as_str(),
                "useCardToPlayered" | "useCardCancelled"
            ) {
                requests
                    .lock()
                    .unwrap()
                    .push(deferred_reference_row(game, event, "request"));
            }
        }));
        let observer = trace.clone();
        game.register_trigger(TriggerRegistration {
            owner: 4,
            skill: "reference_deferred_observer".into(),
            timing: "useCardCancelled".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 15,
            label: None,
            silent: true,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(move |ctx, _, _| {
                observer.lock().unwrap().push(deferred_reference_row(
                    ctx.game(),
                    ctx.event(),
                    "observer",
                ));
                TriggerStep::Done
            }),
        })
        .unwrap();
        let first = trace.clone();
        register_owner(
            &mut game,
            0,
            "useCardToPlayered",
            TriggerOrder::First,
            move |ctx| {
                if ctx.event().target == Some(1) {
                    first.lock().unwrap().push(deferred_reference_row(
                        ctx.game(),
                        ctx.event(),
                        "before-cancel",
                    ));
                    ctx.cancel_card_use().unwrap();
                    first.lock().unwrap().push(deferred_reference_row(
                        ctx.game(),
                        ctx.event(),
                        "after-cancel",
                    ));
                }
                TriggerStep::Done
            },
        );
        let last = trace.clone();
        let changed = count.clone();
        register_owner(
            &mut game,
            0,
            "useCardToPlayered",
            TriggerOrder::Last,
            move |ctx| {
                if ctx.event().target != Some(1) {
                    return TriggerStep::Done;
                }
                last.lock().unwrap().push(deferred_reference_row(
                    ctx.game(),
                    ctx.event(),
                    "before-mutation",
                ));
                if trial == "index-change" {
                    changed.store(5, Ordering::SeqCst);
                }
                if trial == "index-remove" || trial == "index-readd" {
                    assert!(ctx.unregister_trigger(original_id.unwrap()));
                    if trial == "index-readd" {
                        let index_trace = last.clone();
                        let sampled = changed.clone();
                        ctx.register_indexed_trigger(
                            deferred_reference_registration(
                                last.clone(),
                                "reference_deferred_index",
                                "indexed",
                            ),
                            Arc::new(move |game, event, _| {
                                index_trace
                                    .lock()
                                    .unwrap()
                                    .push(deferred_reference_row(game, event, "getIndex"));
                                sampled.load(Ordering::SeqCst) as u32
                            }),
                        )
                        .unwrap();
                    }
                }
                if trial == "new-listener" {
                    ctx.register_trigger(deferred_reference_registration(
                        last.clone(),
                        "reference_deferred_late",
                        "late",
                    ))
                    .unwrap();
                }
                if trial == "out-return" || trial == "index-out-return" {
                    ctx.rest_end(3, None).unwrap();
                    let after = last.clone();
                    return TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
                        after.lock().unwrap().push(deferred_reference_row(
                            ctx.game(),
                            ctx.event(),
                            "after-mutation",
                        ));
                        TriggerStep::Done
                    }));
                }
                last.lock().unwrap().push(deferred_reference_row(
                    ctx.game(),
                    ctx.event(),
                    "after-mutation",
                ));
                TriggerStep::Done
            },
        );
        play(&mut game, &[1]);
        let expected: Vec<_> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                row[0] == trial
                    && matches!(
                        row[1].as_str(),
                        Some(
                            "request"
                                | "before-cancel"
                                | "getIndex"
                                | "after-cancel"
                                | "before-mutation"
                                | "after-mutation"
                                | "existing"
                                | "indexed"
                                | "late"
                        )
                    )
                    && matches!(
                        row[2].as_str(),
                        Some("useCardToPlayered" | "useCardCancelled")
                    )
            })
            .map(|row| {
                serde_json::json!([
                    row[1],
                    row[2],
                    row[7],
                    row[8],
                    row[10]["out"],
                    row[10]["expired"],
                    row[10]["mark"],
                    row[10]["material"] == "o"
                ])
            })
            .collect();
        // The upstream observer is an ordinary callback; compare it separately from the
        // instrumented request snapshots and the two mutation callback sequences.
        let actual: Vec<_> = trace
            .lock()
            .unwrap()
            .iter()
            .filter(|row| row[0] != "observer")
            .cloned()
            .collect();
        assert_eq!(
            actual, expected,
            "{trial} request/callback state and ordering"
        );
        assert_paid_once(&game);
    }
}

mod fifo;
