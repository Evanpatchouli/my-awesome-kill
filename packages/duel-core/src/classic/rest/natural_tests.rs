use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, Card, CardKind, ChoiceBranch, ChoiceKind, Command, EventStatus, Phase, TriggerContext,
    TriggerEvent,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Mutex;

const A: SeatId = 0;
const B: SeatId = 1;
const C: SeatId = 2;
const D: SeatId = 3;
const E: SeatId = 4;
const F: SeatId = 5;
const G: SeatId = 6;
const CLOCK_SEATS: [SeatId; 5] = [B, D, E, F, G];
const OBSERVER: &str = "reference_rest_observer";
const REAL_HAND: Card = Card {
    id: 701,
    kind: CardKind::Sha,
};
const REAL_EQUIP: Card = Card {
    id: 702,
    kind: CardKind::Zhuge,
};
const LIFECYCLE: &[&str] = &[
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
    "revive",
    "reviveBefore",
    "reviveBegin",
    "reviveEnd",
    "reviveAfter",
    "reviveCancelled",
    "dieBefore",
    "dieBegin",
    "die",
    "dieEnd",
    "dieAfter",
    "dieCancelled",
];

#[derive(Default)]
struct Trace {
    trial: Option<&'static str>,
    events: Vec<Value>,
    turns: Vec<Value>,
    stages: Vec<Value>,
    clock_triggers: HashSet<crate::TriggerId>,
    real_triggers: Vec<crate::TriggerId>,
}

fn label(seat: SeatId) -> char {
    char::from(b'A' + seat as u8)
}

fn clock(game: &Game, seat: SeatId) -> Value {
    game.rest_clock(seat)
        .map_or(Value::Null, |clock| json!([clock.unit, clock.count]))
}

fn turn_name(turn: &crate::TurnInfo) -> String {
    turn.reason
        .clone()
        .unwrap_or_else(|| format!("{}-natural", label(turn.seat)))
}

fn record_turn(trace: &mut Trace, game: &Game, event: &TriggerEvent) {
    if trace.trial.is_none() {
        return;
    }
    let id = if event.timing == "phaseZhunbeiBefore" {
        event.parent.expect("the phase has a turn")
    } else {
        event.subject
    };
    let turn = game.turn_info(id).expect("the timing belongs to a turn");
    if event.timing != "roundStart" && !CLOCK_SEATS.contains(&turn.seat) && turn.seat != C {
        return;
    }
    trace.turns.push(json!({
        "timing": event.timing,
        "turn": turn_name(&turn),
        "phase": game.turn_number(),
        "round": game.round_number(),
        "in": game.is_in(turn.seat),
        "clocks": CLOCK_SEATS.map(|seat| clock(game, seat)),
        "counted": turn.rest_counted,
    }));
}

fn record_event(trace: &mut Trace, game: &Game, event: &TriggerEvent, kind: &str) {
    let (Some(trial), Some(seat)) = (trace.trial, event.player) else {
        return;
    };
    if !CLOCK_SEATS.contains(&seat) || !LIFECYCLE.contains(&event.timing.as_str()) {
        return;
    }
    if let Some(id) = game.seat(seat).unwrap().rest_trigger {
        trace.clock_triggers.insert(id);
    }
    if trial == "mbmowang" && matches!(event.timing.as_str(), "die" | "dieEnd" | "dieAfter") {
        assert!(!game.living_seats().contains(&seat));
        assert!(game.dead_seats().contains(&seat));
        assert_eq!(game.seat_order_from(D).unwrap()[..3], [D, E, F]);
        assert_eq!(game.living_seat_ring(D).unwrap()[..2], [D, F]);
    }
    let parent = event.parent.and_then(|id| game.event_kind(id));
    let parent_name = match parent {
        Some(EventKind::RestEnd { .. }) => "restEnd",
        Some(EventKind::RegisteredTrigger { registration, .. }) => game
            .trigger_registrations
            .get(&registration)
            .map(|trigger| trigger.skill.as_str())
            .or_else(|| {
                trace
                    .clock_triggers
                    .contains(&registration)
                    .then_some("_rest_return")
            })
            .expect("lifecycle parent trigger has a recorded name"),
        other => panic!("unexpected lifecycle parent {other:?}"),
    };
    let turn = game
        .scheduler
        .active_events()
        .find_map(|(ancestor, _)| game.turn_info(ancestor.id));
    let revive_hp = match game.event_kind(event.subject) {
        Some(EventKind::Revive { hp, .. }) => Some(hp),
        _ => None,
    };
    let cancelled = game
        .rest_cancelled(event.subject)
        .or_else(|| game.revival_cancelled(event.subject))
        .unwrap_or_else(|| {
            matches!(
                game.scheduler.active_frame(event.subject),
                Some((_, Program::Death(flow))) if matches!(flow.stage, crate::DeathStage::FinishCancelled)
            )
        });
    trace.events.push(json!([
        kind, trial, event.timing, label(seat), turn.as_ref().map(turn_name), parent_name, {
            "hp": game.seat(seat).unwrap().hp,
            "out": game.is_out(seat),
            "alive": game.is_alive(seat),
            "clock": clock(game, seat),
            "reviveHp": revive_hp,
            "reserveOut": game.death_reserved(event.subject),
            "cancelled": cancelled,
        }
    ]));
}

fn capture(trace: &mut Trace, game: &Game, name: &str, seat: SeatId, detail: Value) {
    let player = game.seat(seat).unwrap();
    let ring = game.living_seat_ring(seat).unwrap();
    let previous = *ring.last().unwrap();
    let eligible_next = if game.is_in(previous) {
        game.in_order_from(previous).into_iter().nth(1).map(label)
    } else {
        None
    };
    let public = game.view(None).unwrap();
    let state = json!([
        player.hp,
        player.max_hp,
        game.is_alive(seat),
        game.is_out(seat),
        game.is_in(seat),
        clock(game, seat),
        public
            .seats
            .iter()
            .find(|visible| visible.id == seat)
            .unwrap()
            .rest
            .is_some(),
        game.has_skill(seat, "undist"),
        player.hand.len(),
        player.equipment.len(),
        player.linked,
        player.turned_over,
        game.living_seats().contains(&seat),
        game.dead_seats().contains(&seat),
        game.in_order_from(A).contains(&seat),
        label(previous),
        label(ring[1]),
        eligible_next,
        player.role_shown,
    ]);
    trace
        .stages
        .push(json!({"name": name, "seat": label(seat), "state": state, "detail": detail}));
}

fn completed_status(game: &Game, kind: impl Fn(&EventKind) -> bool) -> EventStatus {
    game.events
        .iter()
        .rev()
        .find(|event| event.status != EventStatus::Started && kind(&event.kind))
        .expect("the operation reached its terminal record")
        .status
}

fn probe(seat: SeatId, label: &str) -> TriggerStep {
    TriggerStep::ChooseFor {
        seat,
        branches: vec![ChoiceBranch {
            id: 1,
            label: label.to_owned(),
            cards: Vec::new(),
            min_cards: 0,
            max_cards: 0,
            targets: Vec::new(),
            min_targets: 0,
            max_targets: 0,
        }],
        then: 1,
    }
}

fn start_clock(ctx: &mut TriggerContext<'_>, seat: SeatId, unit: RestUnit, count: i32) {
    ctx.rest(seat, RestClock { unit, count }).unwrap();
}

fn first_turn(ctx: &mut TriggerContext<'_>, trace: &mut Trace, step: u16) -> TriggerStep {
    match step {
        0 => {
            trace.trial = Some("persistent-clocks");
            start_clock(ctx, F, RestUnit::Phase, 0);
            start_clock(ctx, G, RestUnit::Round, -2);
            TriggerStep::Continue(1)
        }
        1 => {
            capture(trace, ctx.game(), "persistent-zero-placed", F, Value::Null);
            capture(
                trace,
                ctx.game(),
                "persistent-negative-placed",
                G,
                Value::Null,
            );
            trace.trial = Some("phase-clock");
            start_clock(ctx, B, RestUnit::Phase, 4);
            TriggerStep::Continue(2)
        }
        _ => {
            let status = completed_status(ctx.game(), |kind| {
                matches!(kind, EventKind::Rest { seat: B, .. })
            });
            capture(
                trace,
                ctx.game(),
                "phase-clock",
                B,
                json!({
                    "finished": status == EventStatus::Finished,
                    "cancelled": status == EventStatus::Cancelled,
                }),
            );
            ctx.insert_extra_turn(C, "C-extra").unwrap();
            ctx.insert_extra_turn(B, "B-extra").unwrap();
            TriggerStep::Done
        }
    }
}

fn before_start(ctx: &mut TriggerContext<'_>, trace: &mut Trace, step: u16) -> TriggerStep {
    if step != 0 || trace.trial.is_none() {
        return TriggerStep::Done;
    }
    let turn = ctx.game().turn_info(ctx.event().subject).unwrap();
    record_turn(trace, ctx.game(), ctx.event());
    if ctx.game().is_in(turn.seat) {
        // The browser probe replaces its plan with just preparation. The Rust
        // public phase API expresses that setup as skips for the other entries.
        for phase in [
            Phase::Judgment,
            Phase::Draw,
            Phase::Play,
            Phase::Discard,
            Phase::End,
        ] {
            ctx.skip_next_phase(turn.seat, phase).unwrap();
        }
    }
    match (turn.seat, turn.round, turn.reason.as_deref()) {
        (E, 3, None) => {
            capture(
                trace,
                ctx.game(),
                "persistent-zero-unchanged",
                F,
                Value::Null,
            );
            capture(
                trace,
                ctx.game(),
                "persistent-negative-unchanged",
                G,
                Value::Null,
            );
            let game = ctx.game();
            capture(
                trace,
                game,
                "real-returned",
                E,
                json!({
                    "remainingAttendants": game.skill_mark_count(E, "mbdanggu", None),
                    "dead": game.dead_seats().iter().copied().map(label).collect::<Vec<_>>(),
                    "gameOver": game.match_result().is_some(),
                }),
            );
            for id in trace.real_triggers.drain(..) {
                assert!(ctx.unregister_trigger(id));
            }
            trace.trial = Some("competing-clocks");
            for seat in [F, G] {
                ctx.rest_end(seat, Some(2)).unwrap();
                start_clock(ctx, seat, RestUnit::Phase, 1);
            }
            ctx.insert_extra_turn(G, "G-priority-extra").unwrap();
            TriggerStep::Done
        }
        (F, 3, None) => probe(F, "reference complete"),
        _ => TriggerStep::Done,
    }
}

fn phase_begin(ctx: &mut TriggerContext<'_>, trace: &mut Trace, step: u16) -> TriggerStep {
    let turn = ctx.game().turn_info(ctx.event().subject).unwrap();
    match (turn.seat, turn.round, turn.reason.as_deref(), step) {
        (C, 1, None, 0) => {
            capture(trace, ctx.game(), "phase-clock-returned", B, Value::Null);
            trace.trial = Some("round-clock");
            start_clock(ctx, D, RestUnit::Round, 2);
            TriggerStep::Continue(1)
        }
        (C, 1, None, 1) => {
            capture(trace, ctx.game(), "round-clock", D, Value::Null);
            ctx.insert_extra_turn(D, "D-extra").unwrap();
            TriggerStep::Done
        }
        (D, 2, None, 0) => {
            capture(trace, ctx.game(), "round-clock-returned", D, Value::Null);
            trace.trial = Some("mbmowang");
            probe(D, "install the last attendants")
        }
        _ => TriggerStep::Done,
    }
}

fn phase_over(ctx: &mut TriggerContext<'_>, trace: &mut Trace, step: u16) -> TriggerStep {
    if ctx.event().player != Some(E) || ctx.game().round_number() != 2 {
        return TriggerStep::Done;
    }
    if step == 0 {
        let game = ctx.game();
        let view = game.view(None).unwrap();
        capture(
            trace,
            game,
            "real-entered",
            E,
            json!({
                "handInDiscard": view.discard.contains(&REAL_HAND),
                "equipInDiscard": view.discard.contains(&REAL_EQUIP),
                "remainingAttendants": game.skill_mark_count(E, "mbdanggu", None),
                "dead": game.dead_seats().iter().copied().map(label).collect::<Vec<_>>(),
            }),
        );
        trace.trial = Some("mbmowang-repeat-death");
        ctx.die_including_out(E, None).unwrap();
        TriggerStep::Continue(1)
    } else {
        let status = completed_status(ctx.game(), |kind| {
            matches!(kind, EventKind::Death { target: E, .. })
        });
        capture(
            trace,
            ctx.game(),
            "real-repeat-death",
            E,
            json!({
                "cancelled": status == EventStatus::Cancelled,
            }),
        );
        trace.trial = Some("mbmowang");
        TriggerStep::Done
    }
}

fn register_observers(game: &mut Game, trace: &Arc<Mutex<Trace>>) {
    let requests = trace.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        let mut trace = requests.lock().unwrap();
        record_event(&mut trace, game, event, "request");
        if matches!(
            event.timing.as_str(),
            "phaseBefore" | "phaseZhunbeiBefore" | "roundStart"
        ) {
            record_turn(&mut trace, game, event);
        }
    }));
    for timing in LIFECYCLE.iter().copied().chain([
        "phaseUseBegin",
        "phaseBeforeStart",
        "phaseBegin",
        "phaseOver",
    ]) {
        let callbacks = trace.clone();
        game.register_trigger(TriggerRegistration {
            owner: A,
            skill: OBSERVER.to_owned(),
            timing: timing.to_owned(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(move |ctx, step, _| {
                let mut trace = callbacks.lock().unwrap();
                match timing {
                    "phaseUseBegin" if ctx.game().turn_number() == 1 => {
                        first_turn(ctx, &mut trace, step)
                    }
                    "phaseBeforeStart" => before_start(ctx, &mut trace, step),
                    "phaseBegin" => phase_begin(ctx, &mut trace, step),
                    "phaseOver" => phase_over(ctx, &mut trace, step),
                    _ => {
                        record_event(&mut trace, ctx.game(), ctx.event(), "callback");
                        TriggerStep::Done
                    }
                }
            }),
        })
        .unwrap();
    }
}

fn fixture() -> Game {
    let mut game = scene(
        &[
            Role::Lord,
            Role::Loyalist,
            Role::Loyalist,
            Role::Rebel,
            Role::Rebel,
            Role::Rebel,
            Role::Rebel,
            Role::Renegade,
        ],
        A,
    );
    let table = game.classic.as_mut().unwrap();
    table.turn = None;
    table.deck = (100..200).map(|id| card(id, CardKind::Tao)).collect();
    for seat in A..=7 {
        let player = game.seat_mut(seat);
        player.hand = (0..4)
            .map(|number| card(500 + seat * 4 + number, CardKind::Sha))
            .collect();
    }
    // B starts at the end of the already tested manual probe. E's materials
    // come from test setup. Its later death, clean-up and draw use normal rules.
    let player = game.seat_mut(B);
    player.hand.truncate(1);
    player.equipment.push(card(700, CardKind::Zhuge));
    player.linked = true;
    let player = game.seat_mut(E);
    player.hand = vec![REAL_HAND];
    player.equipment.push(REAL_EQUIP);
    player.linked = true;
    for seat in [D, E, F, G] {
        game.seat_mut(seat).hp = 2;
    }
    game
}

fn compare_rows(actual: &[Value], expected: &[Value], section: &str) {
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual, expected, "{section} row {index}");
    }
    assert_eq!(actual.len(), expected.len(), "{section} row count");
}

#[test]
fn natural_rest_clocks_and_last_attendants_match_the_reference() {
    let mut game = fixture();
    let trace = Arc::new(Mutex::new(Trace::default()));
    register_observers(&mut game, &trace);
    game.run(None);

    // Drive the first natural turn with ordinary submissions, then install the
    // production skill precisely where the browser probe adds it.
    for _ in 0..3 {
        let public = game.view(None).unwrap();
        let seat = public
            .waiting_for
            .expect("first turn or installation probe");
        let private = game.view(Some(seat)).unwrap();
        let action = match (seat, private.choice) {
            (A, None) => Action::EndPlay,
            (A, Some(choice)) if choice.kind == ChoiceKind::Discard => Action::Discard {
                event: choice.event,
                cards: choice.cards[..choice.min].to_vec(),
            },
            (D, Some(choice)) if choice.kind == ChoiceKind::Content => {
                assert_eq!(choice.branches[0].label, "install the last attendants");
                let ids = crate::rest_content::install_last_pair(&mut game, E).unwrap();
                let mut trace = trace.lock().unwrap();
                trace.real_triggers = ids;
                capture(&mut trace, &game, "real-ready", E, Value::Null);
                Action::Select {
                    event: choice.event,
                    branch: 1,
                    cards: Vec::new(),
                    targets: Vec::new(),
                }
            }
            (other, choice) => panic!("unexpected seat {other} and choice {choice:?}"),
        };
        game.submit(Command {
            seat,
            version: public.version,
            action,
        })
        .unwrap();
        if seat == D {
            break;
        }
    }
    let final_view = game.view(Some(F)).unwrap();
    assert_eq!(final_view.waiting_for, Some(F));
    assert_eq!(
        final_view.choice.unwrap().branches[0].label,
        "reference complete"
    );

    let reference: Value =
        serde_json::from_str(include_str!("../../../../../tests/reference/rest.json")).unwrap();
    let expected_events: Vec<_> = reference["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| {
            matches!(
                event[1].as_str(),
                Some(
                    "persistent-clocks"
                        | "phase-clock"
                        | "round-clock"
                        | "mbmowang"
                        | "mbmowang-repeat-death"
                        | "competing-clocks"
                )
            )
        })
        .map(|event| {
            // The JS finished flag flips before its generic End/After trigger.
            // Rust records completion after awaited callbacks; terminal status
            // is checked separately at the two checkpoints that need it.
            let mut event = event.clone();
            event[6].as_object_mut().unwrap().remove("finished");
            event
        })
        .collect();
    let expected_stages = &reference["stages"].as_array().unwrap()[10..];
    let trace = trace.lock().unwrap();
    compare_rows(&trace.turns, reference["turns"].as_array().unwrap(), "turn");
    compare_rows(&trace.events, &expected_events, "lifecycle");
    compare_rows(&trace.stages, expected_stages, "stage");
}
