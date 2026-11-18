use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, CardKind, ChoiceKind, Command, TriggerContext, TriggerOrder, TriggerRegistration,
    TriggerRole, TriggerSelection, TriggerStep,
};
use std::sync::{Arc, Mutex};

#[path = "turns_regressions.rs"]
mod regressions;
#[path = "turns_scenarios.rs"]
mod scenarios;

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
fn advance(game: &mut Game) {
    let view = game.view(None).unwrap();
    let seat = view.waiting_for.unwrap();
    let action = match game.view(Some(seat)).unwrap().choice {
        None => Action::EndPlay,
        Some(choice) => match choice.kind {
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
        },
    };
    game.submit(Command {
        seat,
        version: view.version,
        action,
    })
    .unwrap();
}
#[test]
fn loop_preserves_parent_fifo_natural_cursor_flip_dead_and_round_boundary() {
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
    game.classic.as_mut().unwrap().turn = None;
    game.classic.as_mut().unwrap().deck = (100..300).map(|id| card(id, CardKind::Tao)).collect();
    game.seat_mut(3).turned_over = true;
    game.seat_mut(5).alive = false;
    let rows = Arc::new(Mutex::new(Vec::new()));
    let out = rows.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if [
            "phaseBefore",
            "phaseAfter",
            "phaseOver",
            "roundStart",
            "roundEnd",
            "phaseCancelled",
            "phaseBeforeStart",
            "phaseBegin",
            "phaseChange",
            "phaseEnd",
        ]
        .contains(&event.timing.as_str())
        {
            let info = game.turn_info(event.subject);
            let parent = event.parent;
            if info.is_some() {
                assert!(matches!(
                    parent.and_then(|id| game.event_kind(id)),
                    Some(EventKind::PhaseLoop)
                ));
            }
            if ["phaseBefore", "phaseOver", "roundStart"].contains(&event.timing.as_str()) {
                assert_eq!(game.current_phase_player(), None);
            }
            if event.timing == "phaseAfter" {
                assert_eq!(game.current_phase_player(), event.player);
            }
            out.lock().unwrap().push((
                event.timing.clone(),
                event.player.unwrap(),
                game.turn_number(),
                game.round_number(),
                info.and_then(|i| i.reason),
                game.seat(event.player.unwrap()).unwrap().turned_over,
                event.subject,
            ));
        }
    }));
    let mut queue = reg("grant", "phaseAfter", |ctx, _, _| {
        let info = ctx.game().turn_info(ctx.event().subject).unwrap();
        if info.seat == 1 && info.reason.is_none() {
            ctx.insert_extra_turn(0, "first").unwrap();
            ctx.insert_extra_turn(3, "flipped").unwrap();
        }
        if info.seat == 0 && info.reason.as_deref() == Some("first") {
            ctx.insert_extra_turn(5, "dead").unwrap();
        }
        TriggerStep::Done
    });
    queue.order = TriggerOrder::Normal;
    game.register_trigger(queue).unwrap();
    let out = rows.clone();
    game.register_trigger(reg(
        "flipped_callback",
        "phaseCancelled",
        move |ctx, _, _| {
            assert!(
                !ctx.game()
                    .seat(ctx.event().player.unwrap())
                    .unwrap()
                    .turned_over
            );
            out.lock().unwrap().push((
                "phaseCancelledCallback".into(),
                ctx.event().player.unwrap(),
                ctx.game().turn_number(),
                ctx.game().round_number(),
                None,
                false,
                ctx.event().subject,
            ));
            TriggerStep::Done
        },
    ))
    .unwrap();
    game.run(None);
    for _ in 0..100 {
        if game.round_number() == 2 {
            break;
        }
        advance(&mut game);
    }
    let history = game.turn_history();
    let seats: Vec<_> = history
        .iter()
        .map(|h| (h.seat, h.reason.as_deref(), h.skipped))
        .collect();
    assert_eq!(
        seats,
        [
            (0, None, false),
            (1, None, false),
            (0, Some("first"), false),
            (3, Some("flipped"), true),
            (5, Some("dead"), false),
            (2, None, false),
            (3, None, false),
            (4, None, false),
            (6, None, false),
            (7, None, false),
            (0, None, false)
        ]
    );
    assert_eq!(game.round_number(), 2);
    assert_eq!(game.turn_number(), 11);
    let rows = rows.lock().unwrap();
    let loop_ids: Vec<_> = rows
        .iter()
        .filter(|r| r.0 == "phaseOver")
        .map(|r| r.6)
        .collect();
    assert!(loop_ids.iter().all(|id| *id == loop_ids[0]));
    let first_before = rows.iter().find(|r| r.0 == "phaseBefore").unwrap();
    assert_eq!((first_before.1, first_before.2, first_before.3), (0, 0, 0));
    let extra_before = rows
        .iter()
        .find(|r| r.0 == "phaseBefore" && r.4.as_deref() == Some("first"))
        .unwrap();
    assert_eq!((extra_before.2, extra_before.3), (2, 1));
    let dead = history
        .iter()
        .find(|h| h.reason.as_deref() == Some("dead"))
        .unwrap();
    let dead_hooks: Vec<_> = rows
        .iter()
        .filter(|r| r.6 == dead.event)
        .map(|r| r.0.as_str())
        .collect();
    assert!(
        dead_hooks.contains(&"phaseBeforeStart")
            && dead_hooks.contains(&"phaseBegin")
            && dead_hooks.contains(&"phaseEnd")
            && dead_hooks.contains(&"phaseAfter")
    );
    assert!(!dead_hooks.contains(&"phaseChange") && !dead_hooks.contains(&"phaseCancelled"));
    assert_eq!(game.personal_turn_number(5), 1);
    assert_eq!(game.personal_turn_number(3), 1);
    let b_after = rows
        .iter()
        .position(|r| r.0 == "phaseAfter" && r.1 == 1)
        .unwrap();
    let b_over = rows
        .iter()
        .position(|r| r.0 == "phaseOver" && r.1 == 1)
        .unwrap();
    let extra_a = rows
        .iter()
        .position(|r| r.0 == "phaseBefore" && r.4.as_deref() == Some("first"))
        .unwrap();
    assert!(b_after < extra_a && extra_a < b_over);
    let flip_request = rows
        .iter()
        .position(|r| r.0 == "phaseCancelled" && r.1 == 3)
        .unwrap();
    let flip_callback = rows
        .iter()
        .position(|r| r.0 == "phaseCancelledCallback" && r.1 == 3)
        .unwrap();
    assert!(rows[flip_request].5 && !rows[flip_callback].5 && flip_request < flip_callback);
    let natural_dead = rows
        .iter()
        .position(|r| r.0 == "phaseOver" && r.1 == 5)
        .unwrap();
    assert_eq!(rows[natural_dead].2, 8);
    let round_end = rows.iter().position(|r| r.0 == "roundEnd").unwrap();
    let round_start = rows.iter().rposition(|r| r.0 == "roundStart").unwrap();
    assert!(round_end < round_start);
}
#[test]
fn inserted_play_phase_is_next_and_live_plan_reports_reason() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist, Role::Rebel], 0);
    game.classic.as_mut().unwrap().turn = None;
    game.classic.as_mut().unwrap().deck = (100..200).map(|id| card(id, CardKind::Tao)).collect();
    let changes = Arc::new(Mutex::new(Vec::new()));
    game.register_trigger(reg("insert", "phaseBegin", |ctx, _, _| {
        ctx.insert_next_phase(Phase::Play, "extra-play").unwrap();
        TriggerStep::Done
    }))
    .unwrap();
    let output = changes.clone();
    game.register_trigger(reg("read", "phaseChange", move |ctx, _, _| {
        let info = ctx.game().turn_info(ctx.event().subject).unwrap();
        let stage = &info.plan[info.next];
        output
            .lock()
            .unwrap()
            .push((stage.phase, stage.reason.clone()));
        TriggerStep::Done
    }))
    .unwrap();
    game.run(None);
    for _ in 0..20 {
        if changes.lock().unwrap().len() >= 7 {
            break;
        }
        advance(&mut game);
    }
    assert_eq!(
        &changes.lock().unwrap()[..7],
        [
            (Phase::Play, Some("extra-play".to_owned())),
            (Phase::Preparation, None),
            (Phase::Judgment, None),
            (Phase::Draw, None),
            (Phase::Play, None),
            (Phase::Discard, None),
            (Phase::End, None)
        ]
    );
}
#[test]
fn phase_change_inserts_at_pending_front_and_wrong_timing_is_rejected() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist, Role::Rebel], 0);
    game.classic.as_mut().unwrap().turn = None;
    game.classic.as_mut().unwrap().deck = (100..200).map(|id| card(id, CardKind::Tao)).collect();
    let inserted = Arc::new(Mutex::new(false));
    let done = inserted.clone();
    game.register_trigger(reg("during_change", "phaseChange", move |ctx, _, _| {
        let mut done = done.lock().unwrap();
        if !*done {
            let info = ctx.game().turn_info(ctx.event().subject).unwrap();
            assert_eq!(info.plan[info.next].phase, Phase::Preparation);
            ctx.insert_next_phase(Phase::Play, "from-change").unwrap();
            let info = ctx.game().turn_info(ctx.event().subject).unwrap();
            assert_eq!(
                (
                    info.plan[info.next].phase,
                    info.plan[info.next].reason.as_deref()
                ),
                (Phase::Play, Some("from-change"))
            );
            *done = true;
        }
        TriggerStep::Done
    }))
    .unwrap();
    game.register_trigger(reg("invalid_insert", "phaseAfter", |ctx, _, _| {
        assert_eq!(
            ctx.insert_next_phase(Phase::Play, "late"),
            Err(RuleError::WrongEvent)
        );
        TriggerStep::Done
    }))
    .unwrap();
    let stages = Arc::new(Mutex::new(Vec::new()));
    let out = stages.clone();
    game.timing_observer = Some(Arc::new(move |_, e| {
        if ["phaseUseBefore", "phaseZhunbeiBefore"].contains(&e.timing.as_str()) {
            out.lock().unwrap().push(e.timing.clone());
        }
    }));
    game.run(None);
    for _ in 0..20 {
        if stages.lock().unwrap().len() >= 2 {
            break;
        }
        advance(&mut game);
    }
    assert_eq!(
        &stages.lock().unwrap()[..2],
        ["phaseUseBefore", "phaseZhunbeiBefore"]
    );
    assert_eq!(
        game.turn_history()[0].plan[0].reason.as_deref(),
        Some("from-change")
    );
}
