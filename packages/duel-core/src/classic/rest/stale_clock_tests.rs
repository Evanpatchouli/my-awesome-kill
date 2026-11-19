use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::{self, Role, Suit};
use crate::{Action, CardKind, ChoiceBranch, ChoiceKind, Command, Phase, TriggerContext};
use serde_json::{json, Value};
use std::sync::Mutex;

const A: SeatId = 0;
const B: SeatId = 1;
const C: SeatId = 2;
const NEW_CLOCK: RestClock = RestClock {
    unit: RestUnit::Phase,
    count: 9,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct State {
    clock: Option<RestClock>,
    visible_clock: Option<RestClock>,
    hp: i32,
    alive: bool,
    out: bool,
    in_play: bool,
    candidate: bool,
    mark: bool,
    undist: bool,
    next_from_a: Option<SeatId>,
    next_from_b: Option<SeatId>,
}

impl State {
    fn of(game: &Game) -> Self {
        Self {
            clock: game.rest_clock(B),
            visible_clock: game.view(None).unwrap().seats[B as usize].rest,
            hp: game.seat(B).unwrap().hp,
            alive: game.is_alive(B),
            out: game.is_out(B),
            in_play: game.is_in(B),
            candidate: game.in_order_from(A).contains(&B),
            mark: game.has_rest_mark(B),
            undist: game.has_skill(B, "undist"),
            next_from_a: game.next_participant(A).unwrap(),
            next_from_b: game.next_participant(B).unwrap(),
        }
    }

    fn reference_state(self) -> Value {
        json!({
            "hp": self.hp,
            "alive": self.alive,
            "out": self.out,
            "in": self.in_play,
            "clock": self.clock.map(|clock| json!({"type": clock.unit, "count": clock.count})),
            "mark": self.mark,
            "undist": self.undist,
            "candidate": self.candidate,
            "previousNext": self.next_from_a.map(label),
        })
    }
}

fn label(seat: SeatId) -> char {
    char::from(b'A' + seat as u8)
}

#[derive(Default)]
struct Observations {
    dead: Option<State>,
    revived: Option<State>,
    phase_before: Vec<(SeatId, Option<String>, State)>,
    phase_before_start: Vec<(SeatId, Option<String>, State)>,
    same_seat_filter: Vec<(Option<String>, State)>,
    same_seat_effect: Vec<(Option<String>, State)>,
    retry: Option<State>,
}

fn observer(
    timing: &str,
    effect: impl Fn(&mut TriggerContext<'_>, u16) -> TriggerStep + Send + Sync + 'static,
) -> TriggerRegistration {
    TriggerRegistration {
        owner: A,
        skill: format!("stale_clock_probe_{timing}"),
        timing: timing.into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |ctx, step, _| effect(ctx, step)),
    }
}

fn stop_after_retry() -> TriggerStep {
    TriggerStep::ChooseFor {
        seat: A,
        branches: vec![ChoiceBranch {
            id: 1,
            label: "stale clock retry complete".into(),
            cards: vec![],
            min_cards: 0,
            max_cards: 0,
            targets: vec![],
            min_targets: 0,
            max_targets: 0,
        }],
        then: 2,
    }
}

fn trick_targets(game: &Game, seat: SeatId, kind: CardKind) -> Vec<SeatId> {
    game.trick_uses(game.seat(seat).unwrap(), card(1200, kind))
        .unwrap()
        .into_iter()
        .flat_map(|planned| planned.targets)
        .collect()
}

fn run_trial(clock: RestClock, keep_mark: bool) -> Observations {
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
        A,
    );
    let table = game.classic.as_mut().unwrap();
    table.turn = None;
    table.deck = (100..200).map(|id| card(id, CardKind::Tao)).collect();
    let neighbor = game.seat_mut(C);
    neighbor.hand.push(card(700, CardKind::Sha));
    neighbor.hero = classic_data::roster()
        .iter()
        .find(|general| general.id == "huangyueying");
    assert_eq!(game.next_participant(A).unwrap(), Some(B));
    assert_eq!(game.next_participant(B).unwrap(), Some(C));
    assert_eq!(game.distance(A, C).unwrap(), 2);
    assert!(game.in_sha_range(A, B) && game.in_sha_range(B, A));
    if keep_mark {
        game.register_trigger(observer("dieBefore", |ctx, _| {
            ctx.reserve_death(ctx.event().subject, &["_rest_return"])
                .unwrap();
            TriggerStep::Done
        }))
        .unwrap();
    }
    let trace = Arc::new(Mutex::new(Observations::default()));
    let operations = trace.clone();
    game.register_trigger(observer("phaseUseBegin", move |ctx, step| {
        if ctx.game().turn_number() != 1 {
            return TriggerStep::Done;
        }
        match step {
            0 => {
                ctx.rest(B, clock).unwrap();
                TriggerStep::Continue(1)
            }
            1 => {
                ctx.die_including_out(B, None).unwrap();
                TriggerStep::Continue(2)
            }
            2 => {
                operations.lock().unwrap().dead = Some(State::of(ctx.game()));
                ctx.revive(B, Some(2)).unwrap();
                let game = ctx.game();
                operations.lock().unwrap().revived = Some(State::of(game));
                assert_eq!(game.distance(A, C).unwrap(), 1);
                assert!(game.distance(A, B).is_ok());
                assert!(!game.in_sha_range(A, B) && !game.in_sha_range(B, A));
                assert!(game.in_sha_range(A, C));
                ctx.gain_created_card(B, CardKind::Sha, Suit::Spade, 5, None)
                    .unwrap();
                TriggerStep::Continue(3)
            }
            _ => {
                let game = ctx.game();
                assert!(trick_targets(game, A, CardKind::Guohe).contains(&B));
                let snatch = trick_targets(game, A, CardKind::Shunshou);
                assert!(!snatch.contains(&B) && snatch.contains(&C));
                assert!(trick_targets(game, B, CardKind::Shunshou).is_empty());
                assert!(trick_targets(game, C, CardKind::Shunshou).contains(&B));
                ctx.insert_extra_turn(A, "reference_extra").unwrap();
                ctx.insert_extra_turn(B, "reference_extra").unwrap();
                TriggerStep::Done
            }
        }
    }))
    .unwrap();

    let before = trace.clone();
    game.register_trigger(observer("phaseBefore", move |ctx, _| {
        let game = ctx.game();
        if game.turn_number() > 0 {
            let turn = game.turn_info(ctx.event().subject).unwrap();
            before
                .lock()
                .unwrap()
                .phase_before
                .push((turn.seat, turn.reason, State::of(game)));
        }
        TriggerStep::Done
    }))
    .unwrap();

    let (filter_trace, effect_trace) = (trace.clone(), trace.clone());
    game.register_trigger(TriggerRegistration {
        owner: B,
        skill: "same_seat_clock_probe".into(),
        timing: "phaseBefore".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 10,
        label: None,
        silent: true,
        filter: Arc::new(move |game, event, _| {
            let turn = game.turn_info(event.subject).unwrap();
            if game.turn_number() > 0 {
                filter_trace
                    .lock()
                    .unwrap()
                    .same_seat_filter
                    .push((turn.reason, State::of(game)));
            }
            true
        }),
        effect: Arc::new(move |ctx, _, _| {
            let game = ctx.game();
            let turn = game.turn_info(ctx.event().subject).unwrap();
            if game.turn_number() > 0 {
                effect_trace
                    .lock()
                    .unwrap()
                    .same_seat_effect
                    .push((turn.reason, State::of(game)));
            }
            TriggerStep::Done
        }),
    })
    .unwrap();

    let boundaries = trace.clone();
    game.register_trigger(observer("phaseBeforeStart", move |ctx, step| {
        if ctx.game().turn_number() == 1 {
            return TriggerStep::Done;
        }
        let turn = ctx.game().turn_info(ctx.event().subject).unwrap();
        if step == 0 {
            assert!(!turn.rest_counted);
            boundaries.lock().unwrap().phase_before_start.push((
                turn.seat,
                turn.reason.clone(),
                State::of(ctx.game()),
            ));
            if turn.reason.is_none() {
                assert_eq!(turn.seat, B);
                ctx.rest(B, NEW_CLOCK).unwrap();
                return TriggerStep::Continue(1);
            }
            for phase in [
                Phase::Judgment,
                Phase::Draw,
                Phase::Play,
                Phase::Discard,
                Phase::End,
            ] {
                ctx.skip_next_phase(turn.seat, phase).unwrap();
            }
            TriggerStep::Done
        } else {
            boundaries.lock().unwrap().retry = Some(State::of(ctx.game()));
            stop_after_retry()
        }
    }))
    .unwrap();

    game.run(None);
    let public = game.view(None).unwrap();
    assert_eq!(public.waiting_for, Some(A));
    assert!(public.choice.is_none());
    let old_trigger = game.seat(B).unwrap().rest_trigger;
    assert_eq!(old_trigger.is_some(), clock.count > 0);
    game.submit(Command {
        seat: A,
        version: public.version,
        action: Action::EndPlay,
    })
    .unwrap();
    let public = game.view(Some(A)).unwrap();
    let choice = public.choice.unwrap();
    assert_eq!(choice.kind, ChoiceKind::Content);
    assert_eq!(choice.branches[0].label, "stale clock retry complete");
    if let Some(registration) = old_trigger {
        assert!(!game.events.iter().any(|event| {
            event.kind
                == EventKind::RegisteredTrigger {
                    seat: B,
                    registration,
                }
        }));
    }
    drop(game);
    Arc::try_unwrap(trace).ok().unwrap().into_inner().unwrap()
}

#[test]
fn a_revived_seat_clears_only_a_positive_clock_at_its_eligible_boundary() {
    let reference: Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/rest-interruptions.json"
    ))
    .unwrap();
    let mut phase_events = Vec::new();
    for expected in reference["clockTrials"].as_array().unwrap() {
        let clock = RestClock {
            unit: serde_json::from_value(expected["type"].clone()).unwrap(),
            count: i32::try_from(expected["count"].as_i64().unwrap()).unwrap(),
        };
        let name = format!("{}:{}", expected["type"].as_str().unwrap(), clock.count);
        let result = run_trial(clock, false);
        let revived = result.revived.unwrap();
        assert_eq!(
            result.dead,
            Some(State {
                hp: 0,
                alive: false,
                in_play: false,
                candidate: false,
                ..revived
            })
        );
        assert_eq!(revived.next_from_b, None);
        assert_eq!(revived.visible_clock, None);
        assert_eq!(revived.reference_state(), expected["afterRevive"], "{name}");
        assert_eq!(result.phase_before.len(), 3);
        assert_eq!(result.phase_before_start.len(), 3);
        assert_eq!(result.same_seat_filter.len(), 3);
        assert_eq!(result.same_seat_effect.len(), 3);
        for (index, stage) in ["afterOtherExtra", "afterOwnExtra", "afterOwnNatural"]
            .into_iter()
            .enumerate()
        {
            let (seat, reason, before) = &result.phase_before[index];
            let (started_seat, started_reason, after) = &result.phase_before_start[index];
            assert_eq!((seat, reason), (started_seat, started_reason));
            assert_eq!(result.same_seat_filter[index], (reason.clone(), *before));
            assert_eq!(result.same_seat_effect[index], (reason.clone(), *after));
            assert_eq!(after.reference_state(), expected[stage], "{name} {stage}");
            phase_events.extend([
                json!([
                    name,
                    "phaseBefore",
                    label(*seat),
                    reason,
                    before.reference_state()
                ]),
                json!([
                    name,
                    "phaseBeforeStart",
                    label(*seat),
                    reason,
                    after.reference_state()
                ]),
            ]);
        }
        let retry = result.retry.unwrap();
        assert_eq!(
            retry.reference_state(),
            expected["afterRestAgain"],
            "{name}"
        );
        assert_eq!(retry.visible_clock, (clock.count > 0).then_some(NEW_CLOCK));
    }
    let expected_phase_events = reference["phaseEvents"].as_array().unwrap();
    assert_eq!(phase_events.len(), expected_phase_events.len());
    for (index, (actual, expected)) in phase_events.iter().zip(expected_phase_events).enumerate() {
        assert_eq!(actual, expected, "phase event {index}");
    }
}

#[test]
fn discarding_the_clock_keeps_an_explicitly_reserved_mark_visible_as_infinite() {
    let clock = RestClock {
        unit: RestUnit::Phase,
        count: 3,
    };
    let result = run_trial(clock, true);
    assert_eq!(result.revived.unwrap().visible_clock, Some(clock));
    let after_cleanup = result.phase_before_start[0].2;
    assert_eq!(after_cleanup.clock, None);
    assert_eq!(after_cleanup.visible_clock, Some(RestClock::default()));
    assert!(after_cleanup.mark && after_cleanup.undist && after_cleanup.in_play);
    assert_eq!(result.retry.unwrap().clock, Some(NEW_CLOCK));
}
