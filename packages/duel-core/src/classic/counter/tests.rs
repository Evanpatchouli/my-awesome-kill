use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, CardKind, Command, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::{Arc, Mutex};

#[test]
fn one_counter_window_contains_all_responses_and_after_precedes_payment() {
    let mut game = scene(
        &[Role::Lord, Role::Rebel, Role::Loyalist, Role::Renegade],
        0,
    );
    game.classic = None;
    let first = card(1, CardKind::Wuxie);
    let second = card(2, CardKind::Wuxie);
    game.seat_mut(1).hand.push(first);
    game.seat_mut(2).hand.push(second);
    let log = Arc::new(Mutex::new(Vec::new()));
    for timing in ["chooseToUseBegin", "chooseToUseAfter"] {
        let output = log.clone();
        game.register_trigger(TriggerRegistration {
            owner: 0,
            skill: format!("observe_{timing}"),
            timing: timing.into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(move |ctx, _, _| {
                let event = ctx.event();
                assert!(matches!(
                    event.parent.and_then(|id| ctx.game().event_kind(id)),
                    Some(EventKind::CounterWindow {
                        seat: 3,
                        kind: CardKind::Juedou
                    })
                ));
                assert!(matches!(
                    ctx.game().event_kind(event.subject),
                    Some(EventKind::ChooseToUse { .. })
                ));
                assert_eq!(event.source, Some(3));
                let result = ctx.game().response_result(event.subject);
                if result == Some(true) {
                    let seat = event.player.unwrap();
                    assert_eq!(
                        ctx.game().seat(seat).unwrap().hand.len(),
                        1,
                        "After runs before payment"
                    );
                }
                output.lock().unwrap().push((
                    event.timing.clone(),
                    event.player.unwrap(),
                    result,
                    event.parent.unwrap(),
                ));
                TriggerStep::Done
            }),
        })
        .unwrap();
    }
    let paid = Arc::new(Mutex::new(Vec::new()));
    let paid2 = paid.clone();
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "observe_payment".into(),
        timing: "useCard2".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |ctx, _, _| {
            assert!(matches!(
                ctx.event().parent.and_then(|id| ctx.game().event_kind(id)),
                Some(EventKind::CounterWindow { .. })
            ));
            paid2.lock().unwrap().push(ctx.event().parent.unwrap());
            TriggerStep::Done
        }),
    })
    .unwrap();
    let context = CounterContext {
        card_kind: CardKind::Juedou,
        source: Some(0),
        target: 3,
        negated: false,
    };
    game.scheduler.start(game.counter_window(context));
    game.run(None);
    let mut plays = [false; 4];
    while let Some(crate::Decision::Cards(choice)) = game.decision.as_ref() {
        let seat = choice.seat;
        let use_card = (seat == 1 || seat == 2) && !plays[seat as usize];
        if use_card {
            plays[seat as usize] = true;
        }
        let event = choice.event;
        game.submit(Command {
            version: game.version,
            seat,
            action: Action::Select {
                event,
                branch: u32::from(use_card),
                cards: if use_card { vec![seat] } else { vec![] },
                targets: vec![],
            },
        })
        .unwrap();
    }
    let log = log.lock().unwrap();
    assert_eq!(
        log.iter()
            .filter(|(timing, _, _, _)| timing == "chooseToUseAfter")
            .count(),
        2
    );
    assert_eq!(
        log.iter()
            .filter(|(_, _, result, _)| *result == Some(true))
            .map(|(_, seat, _, _)| *seat)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        game.discard
            .iter()
            .filter(|&&card| card == first || card == second)
            .count(),
        2
    );
    let parent = log[0].3;
    assert!(log.iter().all(|entry| entry.3 == parent));
    assert_eq!(*paid.lock().unwrap(), vec![parent, parent]);
}

#[test]
fn sha_shan_after_follows_payment_but_wanjian_shan_is_pure_response() {
    for is_sha in [true, false] {
        let mut game = scene(&[Role::Lord, Role::Rebel, Role::Renegade], 0);
        if !is_sha {
            game.classic = None;
        }
        let material = card(
            1,
            if is_sha {
                CardKind::Sha
            } else {
                CardKind::Wanjian
            },
        );
        let shan = card(2, CardKind::Shan);
        game.seat_mut(0).hand.push(material);
        game.seat_mut(1).hand.push(shan);
        let after = Arc::new(Mutex::new(0));
        let result = after.clone();
        game.register_trigger(TriggerRegistration {
            owner: 0,
            skill: "shan_after".into(),
            timing: "chooseToUseAfter".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, event, _| event.player == Some(1)),
            effect: Arc::new(move |ctx, _, _| {
                assert_eq!(ctx.game().response_result(ctx.event().subject), Some(true));
                assert!(
                    ctx.game().discard.contains(&shan),
                    "Sha's Shan is paid before After"
                );
                *result.lock().unwrap() += 1;
                TriggerStep::Done
            }),
        })
        .unwrap();
        let program = if is_sha {
            game.attack_program(0, vec![material], vec![1])
        } else {
            game.trick_uses(game.seat(0).unwrap(), material)
                .unwrap()
                .remove(0)
                .program
        };
        game.scheduler.start(program);
        game.run(None);
        for _ in 0..30 {
            if is_sha && *after.lock().unwrap() == 1 {
                break;
            }
            let Some(crate::Decision::Cards(choice)) = game.decision.as_ref() else {
                break;
            };
            let seat = choice.seat;
            let event = choice.event;
            let action = if choice.kind == crate::ChoiceKind::Shan {
                Action::Choose {
                    event,
                    card: (seat == 1).then_some(shan.id),
                }
            } else {
                Action::Select {
                    event,
                    branch: 0,
                    cards: vec![],
                    targets: vec![],
                }
            };
            game.submit(Command {
                version: game.version,
                seat,
                action,
            })
            .unwrap();
        }
        assert_eq!(*after.lock().unwrap(), usize::from(is_sha));
        assert!(game.discard.contains(&shan));
    }
}

#[test]
fn after_callback_can_remove_selected_wuxie_without_double_payment() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Renegade], 0);
    game.classic = None;
    let wuxie = card(1, CardKind::Wuxie);
    game.seat_mut(1).hand.push(wuxie);
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "intercept".into(),
        timing: "chooseToUseAfter".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, event, _| event.player == Some(1)),
        effect: Arc::new(move |ctx, _, _| {
            if ctx.game().response_result(ctx.event().subject) == Some(true) {
                ctx.discard_hand(1, &[wuxie.id]).unwrap();
            }
            TriggerStep::Done
        }),
    })
    .unwrap();
    let context = CounterContext {
        card_kind: CardKind::Juedou,
        source: Some(0),
        target: 2,
        negated: false,
    };
    game.scheduler.start(game.counter_window(context));
    game.run(None);
    while let Some(crate::Decision::Cards(choice)) = game.decision.as_ref() {
        let seat = choice.seat;
        let play = seat == 1 && game.seat(1).unwrap().hand.contains(&wuxie);
        let event = choice.event;
        game.submit(Command {
            version: game.version,
            seat,
            action: Action::Select {
                event,
                branch: u32::from(play),
                cards: if play { vec![wuxie.id] } else { vec![] },
                targets: vec![],
            },
        })
        .unwrap();
    }
    assert_eq!(
        game.discard.iter().filter(|&&card| card == wuxie).count(),
        1
    );
    assert!(!game.resolving.contains(&wuxie));
}

#[test]
fn cancelled_or_intercepted_lose_does_not_discard_an_unowned_material() {
    for cancel in [true, false] {
        let mut game = scene(&[Role::Lord, Role::Rebel, Role::Renegade], 0);
        game.classic = None;
        let wuxie = card(1, CardKind::Wuxie);
        game.seat_mut(1).hand.push(wuxie);
        game.register_trigger(TriggerRegistration {
            owner: 1,
            skill: "prevent_loss".into(),
            timing: "loseBefore".into(),
            role: TriggerRole::Player,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(move |_, event, _| event.cards.contains(&wuxie)),
            effect: Arc::new(move |ctx, _, _| {
                if cancel {
                    TriggerStep::CancelSubject
                } else {
                    ctx.discard_hand(1, &[wuxie.id]).unwrap();
                    TriggerStep::Done
                }
            }),
        })
        .unwrap();
        let uses = Arc::new(Mutex::new(0));
        let uses2 = uses.clone();
        game.register_trigger(TriggerRegistration {
            owner: 0,
            skill: "used".into(),
            timing: "useCard2".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, event, _| event.card_kind == Some(CardKind::Wuxie)),
            effect: Arc::new(move |_, _, _| {
                *uses2.lock().unwrap() += 1;
                TriggerStep::Done
            }),
        })
        .unwrap();
        let context = CounterContext {
            card_kind: CardKind::Juedou,
            source: Some(0),
            target: 2,
            negated: false,
        };
        game.scheduler.start(game.counter_window(context));
        game.run(None);
        let mut attempted = false;
        while let Some(crate::Decision::Cards(choice)) = game.decision.as_ref() {
            let seat = choice.seat;
            let play = seat == 1 && !attempted;
            if play {
                attempted = true;
            }
            let event = choice.event;
            game.submit(Command {
                version: game.version,
                seat,
                action: Action::Select {
                    event,
                    branch: u32::from(play),
                    cards: if play { vec![wuxie.id] } else { vec![] },
                    targets: vec![],
                },
            })
            .unwrap();
        }
        assert!(attempted);
        assert_eq!(*uses.lock().unwrap(), 1);
        assert_eq!(game.seat(1).unwrap().hand.contains(&wuxie), cancel);
        assert_eq!(
            game.discard.iter().filter(|&&card| card == wuxie).count(),
            usize::from(!cancel)
        );
        assert!(!game.resolving.contains(&wuxie));
    }
}
