use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, CardKind, ChoiceKind, Command, EventStatus, GameSetup, SeatSetup, TriggerContext,
    TriggerEvent, TriggerSelection,
};
use serde_json::{json, Value};
use std::sync::Mutex;

fn clock(unit: RestUnit, count: i32) -> RestClock {
    RestClock { unit, count }
}

fn trigger(
    skill: &str,
    timing: &str,
    order: TriggerOrder,
    effect: impl Fn(&mut TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) -> TriggerRegistration {
    TriggerRegistration {
        owner: 0,
        skill: skill.into(),
        timing: timing.into(),
        role: TriggerRole::Global,
        order,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(effect),
    }
}

fn reference() -> Value {
    serde_json::from_str(include_str!("../../../../../tests/reference/rest.json")).unwrap()
}

fn clock_row(game: &Game, seat: SeatId) -> Value {
    match game.rest_clock(seat) {
        Some(clock) => json!([clock.unit, clock.count]),
        None => Value::Null,
    }
}

fn lifecycle_row(game: &Game, event: &TriggerEvent, trial: &str, observation: &str) -> Value {
    let player = game.seat(1).unwrap();
    let parent = event.parent.and_then(|id| game.event_kind(id));
    let revive_hp = match game.event_kind(event.subject) {
        Some(EventKind::Revive { hp, .. }) => Some(hp),
        _ => None,
    };
    let cancelled = game
        .rest_cancelled(event.subject)
        .or_else(|| game.revival_cancelled(event.subject))
        .unwrap();
    json!([
        observation,
        trial,
        event.timing,
        matches!(parent, Some(EventKind::RestEnd { .. })),
        player.hp,
        game.is_out(1),
        game.is_alive(1),
        clock_row(game, 1),
        revive_hp,
        cancelled
    ])
}

fn assert_direct_stage(game: &Game, reference: &Value, name: &str) {
    let expected = reference["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stage| stage["name"] == name)
        .unwrap();
    let player = game.seat(1).unwrap();
    let displayed = game.view(None).unwrap();
    let view = displayed.seats.iter().find(|seat| seat.id == 1).unwrap();
    let ring = game.living_seat_ring(1).unwrap();
    let label = |seat: SeatId| char::from(b'A' + seat as u8).to_string();
    let previous = *ring.last().unwrap();
    let next_participant = game
        .in_order_from(previous)
        .into_iter()
        .find(|&seat| seat != previous);
    for (field, actual) in [
        ("hp", json!(player.hp)),
        ("maxHp", json!(player.max_hp)),
        ("alive", json!(game.is_alive(1))),
        ("out", json!(game.is_out(1))),
        ("in", json!(game.is_in(1))),
        ("clock", clock_row(game, 1)),
        ("mark", json!(view.rest.is_some())),
        ("undist", json!(game.has_skill(1, "undist"))),
        ("hand", json!(player.hand.len())),
        ("equip", json!(player.equipment.len())),
        ("linked", json!(player.linked)),
        ("turnedOver", json!(player.turned_over)),
        ("inPlayers", json!(game.living_seats().contains(&1))),
        ("inDead", json!(game.dead_seats().contains(&1))),
        ("counted", json!(game.in_order_from(0).contains(&1))),
        ("previous", json!(label(*ring.last().unwrap()))),
        ("next", json!(label(ring[1]))),
        ("previousEligibleNext", json!(next_participant.map(label))),
        ("identityShown", json!(player.role_shown)),
    ] {
        let index = reference["stateColumns"]
            .as_array()
            .unwrap()
            .iter()
            .position(|column| column == field)
            .unwrap();
        assert_eq!(actual, expected["state"][index], "{name} {field}");
    }
    assert_eq!(view.out, game.is_out(1));
}

#[test]
fn manual_rest_and_end_match_the_reference_requests_and_callbacks() {
    let mut game = Game::new(GameSetup {
        active_seat: 0,
        seats: (0..8)
            .map(|id| SeatSetup {
                id,
                hp: if id == 1 { 2 } else { 4 },
                hand: Vec::new(),
            })
            .collect(),
    })
    .unwrap();
    let hand = card(1, CardKind::Sha);
    let equipment = card(2, CardKind::Zhuge);
    let player = game.seat_mut(1);
    player.max_hp = 4;
    player.hand.push(hand);
    player.equipment.install_test(equipment);
    player.linked = true;
    player.turned_over = true;

    let trials = [
        ("end-without-rest", Flow::end(1, 1), None),
        (
            "cancel-rest",
            Flow::rest(1, RestClock::default()),
            Some("rest"),
        ),
        (
            "repeat-rest",
            Flow::rest(1, clock(RestUnit::Round, 8)),
            None,
        ),
        ("cancel-revive", Flow::end(1, 3), Some("reviveBefore")),
        ("zero-clock", Flow::rest(1, clock(RestUnit::Phase, 0)), None),
        ("manual-zero-hp", Flow::end(1, 0), None),
        (
            "negative-clock",
            Flow::rest(1, clock(RestUnit::Phase, -2)),
            None,
        ),
        ("cancel-rest-end", Flow::end(1, 4), Some("restEnd")),
        ("repeat-end", Flow::end(1, 2), None),
    ];
    let reference = reference();
    assert_direct_stage(&game, &reference, "initial");

    let trial = Arc::new(Mutex::new(("", None)));
    let rows = Arc::new(Mutex::new(Vec::new()));
    let (observed_trial, requests) = (trial.clone(), rows.clone());
    let timings = [
        "rest",
        "restBefore",
        "restBegin",
        "restEnd",
        "restAfter",
        "restCancelled",
        "restEndBefore",
        "restEndBegin",
        "restEndEnd",
        "restEndAfter",
        "restEndCancelled",
        "reviveBefore",
        "reviveBegin",
        "reviveEnd",
        "reviveAfter",
        "reviveCancelled",
    ];
    game.timing_observer = Some(Arc::new(move |game, event| {
        if timings.contains(&event.timing.as_str()) {
            let name = observed_trial.lock().unwrap().0;
            requests
                .lock()
                .unwrap()
                .push(lifecycle_row(game, event, name, "request"));
        }
    }));
    for timing in timings {
        let (stage, observed) = (trial.clone(), rows.clone());
        game.register_trigger(trigger(
            "probe",
            timing,
            TriggerOrder::First,
            move |ctx, _, _| {
                let (name, cancel) = *stage.lock().unwrap();
                observed.lock().unwrap().push(lifecycle_row(
                    ctx.game(),
                    ctx.event(),
                    name,
                    "callback",
                ));
                if cancel == Some(ctx.event().timing.as_str()) {
                    TriggerStep::CancelSubject
                } else {
                    TriggerStep::Done
                }
            },
        ))
        .unwrap();
    }

    for (name, flow, cancel) in trials.clone() {
        *trial.lock().unwrap() = (name, cancel);
        let kind = flow.kind();
        let old_events = game.events.len();
        game.scheduler.start(Program::Rest(flow));
        game.run(None);
        assert_direct_stage(&game, &reference, name);
        let finished = game.events[old_events..]
            .iter()
            .find(|event| event.kind == kind && event.status != EventStatus::Started)
            .unwrap();
        assert_eq!(
            finished.status,
            if cancel.is_some_and(|timing| timing != "reviveBefore") {
                EventStatus::Cancelled
            } else {
                EventStatus::Finished
            },
            "{name}"
        );
        let player = game.seat(1).unwrap();
        assert_eq!(player.hand, [hand]);
        assert_eq!(player.equipment, [equipment]);
    }
    let expected: Vec<Value> = reference["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| trials.iter().any(|(name, _, _)| row[1] == *name))
        .map(|row| {
            let state = &row[6];
            json!([
                row[0],
                row[1],
                row[2],
                row[5] == "restEnd",
                state["hp"],
                state["out"],
                state["alive"],
                state["clock"],
                state["reviveHp"],
                state["cancelled"]
            ])
        })
        .collect();
    assert_eq!(*rows.lock().unwrap(), expected);
}

#[test]
fn rest_reinserts_a_dead_seat_in_physical_order_without_restoring_hp() {
    let mut game = Game::new(GameSetup {
        active_seat: 4,
        seats: [(4, 4), (9, 0), (2, 0), (7, 4)]
            .into_iter()
            .map(|(id, hp)| SeatSetup {
                id,
                hp,
                hand: Vec::new(),
            })
            .collect(),
    })
    .unwrap();
    game.revive_now(2, Some(1)).unwrap();
    assert_eq!(game.living_seats(), &[4, 7, 2]);
    assert_eq!(game.dead_seats(), &[9]);

    game.scheduler
        .start(Program::Rest(Flow::rest(9, RestClock::default())));
    game.run(None);
    assert!(game.is_alive(9) && game.is_resting(9));
    assert_eq!(game.seat(9).unwrap().hp, 0);
    assert_eq!(game.living_seats(), &[4, 9, 2, 7]);
    assert_eq!(game.living_seat_ring(9).unwrap(), [9, 2, 7, 4]);
    assert_eq!(game.in_order_from(9), [2, 7, 4]);
    assert!(game.dead_seats().is_empty());
}

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
    let classic = game.classic.as_mut().unwrap();
    classic.turn = None;
    classic.deck = (100..300).map(|id| card(id, CardKind::Tao)).collect();
    game
}

fn end_play_or_discard(game: &mut Game) {
    let view = game.view(None).unwrap();
    let seat = view.waiting_for.unwrap();
    let action = match game.view(Some(seat)).unwrap().choice {
        None => Action::EndPlay,
        Some(choice) if choice.kind == ChoiceKind::Discard => Action::Discard {
            event: choice.event,
            cards: choice.cards[..choice.min].to_vec(),
        },
        Some(choice) => panic!("unexpected decision {:?}", choice.kind),
    };
    game.submit(Command {
        seat,
        version: view.version,
        action,
    })
    .unwrap();
}

#[test]
fn countdown_uses_the_first_eligible_seat_in_the_phase_before_dispatch() {
    let mut game = table();
    game.register_trigger(trigger(
        "put_to_rest",
        "phaseUseBegin",
        TriggerOrder::First,
        |ctx, _, _| {
            if ctx.game().turn_number() == 1 {
                ctx.rest(1, clock(RestUnit::Round, 1)).unwrap();
                ctx.rest(3, clock(RestUnit::Phase, 3)).unwrap();
                ctx.rest(5, clock(RestUnit::Phase, 0)).unwrap();
                ctx.rest(6, clock(RestUnit::Round, -2)).unwrap();
                ctx.insert_extra_turn(1, "B-extra").unwrap();
                ctx.insert_extra_turn(2, "C-extra").unwrap();
            }
            TriggerStep::Done
        },
    ))
    .unwrap();

    let turns = Arc::new(Mutex::new(Vec::new()));
    let captured = turns.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.timing == "restEnd" {
            assert!(matches!(
                event.parent.and_then(|id| game.event_kind(id)),
                Some(EventKind::RegisteredTrigger { seat, .. }) if event.player == Some(seat)
            ));
        }
        if matches!(
            event.timing.as_str(),
            "phaseBefore" | "phaseBeforeStart" | "phaseBegin" | "phaseAfter"
        ) {
            let turn = game.turn_info(event.subject).unwrap();
            captured.lock().unwrap().push((
                turn.number,
                turn.seat,
                event.timing.clone(),
                game.is_in(turn.seat),
                game.rest_clock(1).map(|clock| clock.count),
                game.rest_clock(3).map(|clock| clock.count),
                turn.rest_counted,
            ));
        }
    }));

    let ordered = Arc::new(Mutex::new(Vec::new()));
    for order in [TriggerOrder::First, TriggerOrder::Last] {
        let capture = ordered.clone();
        game.register_trigger(trigger(
            if order == TriggerOrder::First {
                "before_clock"
            } else {
                "after_clock"
            },
            "phaseBefore",
            order,
            move |ctx, _, _| {
                let turn = ctx.game().turn_info(ctx.event().subject).unwrap();
                if turn.number == 4 {
                    capture.lock().unwrap().push((
                        order,
                        ctx.game().rest_clock(1),
                        ctx.game().rest_clock(3),
                        turn.rest_counted,
                    ));
                }
                TriggerStep::Done
            },
        ))
        .unwrap();
    }

    game.run(None);
    while game.turn_number() < 5 {
        end_play_or_discard(&mut game);
    }
    let records = turns.lock().unwrap();
    let lookup = |number, timing: &str| {
        records
            .iter()
            .find(|row| row.0 == number && row.2 == timing)
            .map(|row| (row.1, row.3, row.4, row.5, row.6))
            .unwrap()
    };
    assert_eq!(
        lookup(2, "phaseBeforeStart"),
        (1, false, Some(1), Some(2), true)
    );
    assert_eq!(
        lookup(3, "phaseBeforeStart"),
        (2, true, Some(1), Some(1), true)
    );
    assert_eq!(lookup(2, "phaseBegin"), (1, false, Some(1), Some(2), true));
    assert_eq!(lookup(2, "phaseAfter"), (1, false, Some(1), Some(2), true));
    assert_eq!(
        lookup(4, "phaseBefore"),
        (1, false, Some(1), Some(1), false)
    );
    assert_eq!(
        lookup(4, "phaseBeforeStart"),
        (1, true, None, Some(1), true)
    );
    assert_eq!(lookup(5, "phaseBeforeStart"), (2, true, None, None, true));
    assert_eq!(
        *ordered.lock().unwrap(),
        [
            (
                TriggerOrder::First,
                Some(clock(RestUnit::Round, 1)),
                Some(clock(RestUnit::Phase, 1)),
                false
            ),
            (
                TriggerOrder::Last,
                None,
                Some(clock(RestUnit::Phase, 1)),
                true
            ),
        ]
    );
    assert_eq!(game.rest_clock(5), Some(clock(RestUnit::Phase, 0)));
    assert_eq!(game.rest_clock(6), Some(clock(RestUnit::Round, -2)));
    assert!(game.is_out(5) && game.is_out(6));
    let out_turn = &game.turn_history()[1];
    assert_eq!(out_turn.reason.as_deref(), Some("B-extra"));
    assert!(out_turn.personal_started && out_turn.rest_counted);
    assert_eq!(out_turn.next, 0);
    assert!(!out_turn.skipped && !out_turn.cancelled);
    assert!(game.seat(1).unwrap().rest_trigger.is_none());
    assert!(game.seat(3).unwrap().rest_trigger.is_none());
}

#[test]
fn all_reserved_deaths_yield_the_live_loop_without_declaring_a_winner() {
    let mut game = scene(&[Role::Lord, Role::Rebel], 0);
    game.classic.as_mut().unwrap().turn = None;
    let reserve = game
        .register_trigger(trigger(
            "reserve_both",
            "dieBefore",
            TriggerOrder::First,
            |ctx, _, _| {
                ctx.reserve_death(ctx.event().subject, &[]).unwrap();
                TriggerStep::Done
            },
        ))
        .unwrap();
    game.set_trigger_force_die(reserve, true).unwrap();
    game.register_trigger(trigger(
        "both_die",
        "phaseUseBegin",
        TriggerOrder::First,
        |ctx, _, _| {
            ctx.die(1, None).unwrap();
            ctx.die(0, None).unwrap();
            TriggerStep::Done
        },
    ))
    .unwrap();
    game.run(None);
    assert_eq!(game.dead_seats(), [1, 0]);
    assert!(game.living_seats().is_empty());
    assert!(game.round_suspended());
    assert!(game.match_result().is_none());
}

#[test]
fn full_table_rest_yields_one_round_at_a_time_without_a_winner() {
    let mut game = table();
    game.register_trigger(trigger(
        "all_out",
        "phaseUseBegin",
        TriggerOrder::First,
        |ctx, _, _| {
            ctx.rest(0, clock(RestUnit::Phase, i32::MAX)).unwrap();
            for seat in 1..8 {
                ctx.rest(seat, RestClock::default()).unwrap();
            }
            TriggerStep::Done
        },
    ))
    .unwrap();
    game.run(None);
    assert!(game.round_suspended());
    assert_eq!((game.round_number(), game.turn_number()), (1, 8));
    assert_eq!(
        game.rest_clock(0),
        Some(clock(RestUnit::Phase, i32::MAX - 7))
    );
    assert!(game.match_result().is_none());
    assert_eq!(game.view(None).unwrap().waiting_for, None);
    let loop_id = game.scheduler.current_id();
    let command = game.bot_command().unwrap();
    assert_eq!(command.action, Action::AdvanceRound);
    game.submit(command).unwrap();
    assert!(game.round_suspended());
    assert_eq!(game.scheduler.current_id(), loop_id);
    assert_eq!((game.round_number(), game.turn_number()), (2, 16));
    assert_eq!(
        game.rest_clock(0),
        Some(clock(RestUnit::Phase, i32::MAX - 15))
    );
    assert!(game.match_result().is_none());
    assert_eq!(game.view(None).unwrap().waiting_for, None);
}
