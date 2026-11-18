use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, ChoiceKind, Command, DamageNature, EventKind, Game, Phase, Program, RuleError,
    TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::{Arc, Mutex};

fn reg(
    owner: u32,
    name: &str,
    timing: &str,
    order: TriggerOrder,
    effect: impl Fn(&mut crate::TriggerContext<'_>, u16, Option<&crate::TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) -> TriggerRegistration {
    TriggerRegistration {
        owner,
        skill: name.into(),
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
fn submit(game: &mut Game, seat: u32, action: Action) {
    game.submit(Command {
        version: game.version,
        seat,
        action,
    })
    .unwrap();
}
#[test]
fn live_damage_increases_stack_once_and_propagate_the_committed_amount() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Rebel, Role::Loyalist], 0);
    game.seat_mut(0).hand.push(card(1, crate::CardKind::Sha));
    game.set_sha_nature(1, Some(DamageNature::Fire)).unwrap();
    game.seat_mut(1).linked = true;
    game.seat_mut(2).linked = true;
    game.seat_mut(1).armor = 1;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    game.register_trigger(reg(
        0,
        "first",
        "damageBegin1",
        TriggerOrder::First,
        move |ctx, _, _| {
            log.lock()
                .unwrap()
                .push(("first", ctx.event().player.unwrap(), ctx.event().amount));
            assert_eq!(ctx.add_damage(u16::MAX), Err(RuleError::InvalidSelection));
            TriggerStep::Done
        },
    ))
    .unwrap();
    for (owner, name) in [(0, "one"), (3, "two")] {
        let log = seen.clone();
        let mut r = reg(
            owner,
            name,
            "damageBegin1",
            TriggerOrder::Normal,
            move |ctx, _, _| {
                let direct = ctx
                    .event()
                    .parent
                    .and_then(|id| ctx.game().event_kind(id))
                    .is_some_and(|kind| matches!(kind, EventKind::ShaEffect { .. }));
                if direct {
                    log.lock().unwrap().push((
                        name,
                        ctx.event().player.unwrap(),
                        ctx.event().amount,
                    ));
                    ctx.add_damage(1).unwrap();
                }
                TriggerStep::Done
            },
        );
        r.filter = Arc::new(move |_, event, _| event.amount == if owner == 0 { 1 } else { 2 });
        game.register_trigger(r).unwrap();
    }
    let log = seen.clone();
    game.register_trigger(reg(
        0,
        "after",
        "damageBegin1",
        TriggerOrder::Last,
        move |ctx, _, _| {
            log.lock()
                .unwrap()
                .push(("after", ctx.event().player.unwrap(), ctx.event().amount));
            TriggerStep::Done
        },
    ))
    .unwrap();
    game.register_trigger(reg(
        0,
        "bad_time",
        "useCard2",
        TriggerOrder::First,
        |ctx, _, _| {
            assert_eq!(ctx.add_damage(1), Err(RuleError::WrongEvent));
            TriggerStep::Done
        },
    ))
    .unwrap();
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: 1,
            targets: vec![1],
        },
    );
    for _ in 0..40 {
        let view = game.view(None).unwrap();
        let Some(seat) = view.waiting_for else { break };
        let Some(choice) = game.view(Some(seat)).unwrap().choice else {
            break;
        };
        let action = match choice.kind {
            ChoiceKind::Shan | ChoiceKind::Save => Action::Choose {
                event: choice.event,
                card: None,
            },
            ChoiceKind::Content => Action::Select {
                event: choice.event,
                branch: 0,
                cards: vec![],
                targets: vec![],
            },
            _ => panic!("unexpected choice"),
        };
        submit(&mut game, seat, action);
    }
    let hits = game.damage_history();
    assert_eq!(hits.len(), 2);
    assert_eq!(
        (
            hits[0].target,
            hits[0].amount,
            hits[0].absorbed,
            hits[0].propagated
        ),
        (1, 3, 1, false)
    );
    assert_eq!(
        (
            hits[1].target,
            hits[1].amount,
            hits[1].absorbed,
            hits[1].propagated
        ),
        (2, 3, 0, true)
    );
    assert_eq!((game.seat(1).unwrap().hp, game.seat(2).unwrap().hp), (2, 1));
    assert_eq!(
        *seen.lock().unwrap(),
        [
            ("first", 1, 1),
            ("one", 1, 1),
            ("two", 1, 2),
            ("after", 1, 3),
            ("first", 2, 3),
            ("after", 2, 3)
        ]
    );
}

#[test]
fn same_group_rechecks_updated_amount_and_nested_damage_keeps_separate_frames() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Rebel, Role::Rebel], 0);
    game.seat_mut(0).hand.push(card(1, crate::CardKind::Sha));
    let mut initial = reg(
        0,
        "earlier",
        "damageBegin1",
        TriggerOrder::First,
        |ctx, _, _| {
            assert_eq!(ctx.event().amount, 1);
            ctx.add_damage(1).unwrap();
            TriggerStep::Done
        },
    );
    initial.filter = Arc::new(|_, e, _| e.player == Some(1));
    game.register_trigger(initial).unwrap();
    let stale = Arc::new(Mutex::new(false));
    for name in ["increase", "stale"] {
        let ran = stale.clone();
        let mut registration = reg(
            0,
            name,
            "damageBegin1",
            TriggerOrder::Normal,
            move |ctx, _, _| {
                if name == "increase" {
                    assert_eq!(ctx.event().amount, 2);
                    ctx.add_damage(1).unwrap();
                } else {
                    *ran.lock().unwrap() = true;
                }
                TriggerStep::Done
            },
        );
        registration.silent = false;
        registration.label = Some(name.into());
        registration.filter = Arc::new(|_, e, _| e.player == Some(1) && e.amount == 2);
        game.register_trigger(registration).unwrap();
    }
    let mut nested = reg(
        0,
        "nested",
        "damageBegin1",
        TriggerOrder::Last,
        |ctx, _, _| {
            assert_eq!(ctx.event().amount, 3);
            ctx.damage(Some(0), 3, 1, None, None, &[]).unwrap();
            TriggerStep::Done
        },
    );
    nested.filter = Arc::new(|_, e, _| e.player == Some(1));
    game.register_trigger(nested).unwrap();
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: 1,
            targets: vec![1],
        },
    );
    let mut chosen = false;
    for _ in 0..40 {
        let view = game.view(None).unwrap();
        let Some(seat) = view.waiting_for else { break };
        let Some(choice) = game.view(Some(seat)).unwrap().choice else {
            break;
        };
        let action = if let Some(b) = choice.branches.iter().find(|b| b.label == "increase") {
            assert!(choice.branches.iter().any(|b| b.label == "stale"));
            chosen = true;
            Action::Select {
                event: choice.event,
                branch: b.id,
                cards: vec![],
                targets: vec![],
            }
        } else {
            match choice.kind {
                ChoiceKind::Shan | ChoiceKind::Save => Action::Choose {
                    event: choice.event,
                    card: None,
                },
                ChoiceKind::Content => Action::Select {
                    event: choice.event,
                    branch: 0,
                    cards: vec![],
                    targets: vec![],
                },
                _ => panic!("unexpected choice"),
            }
        };
        submit(&mut game, seat, action);
    }
    assert!(chosen && !*stale.lock().unwrap());
    let hits = game.damage_history();
    assert_eq!(
        hits.iter()
            .map(|hit| (hit.target, hit.amount))
            .collect::<Vec<_>>(),
        [(3, 1), (1, 3)]
    );
    assert_ne!(hits[0].event, hits[1].event);
}
