use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep};
use std::sync::{Arc, Mutex};

fn table() -> Game {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist], 0);
    game.classic = None;
    game
}

fn observe(
    game: &mut Game,
    action: impl Fn(&mut crate::TriggerContext<'_>) -> TriggerStep + Send + Sync + 'static,
) {
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "use_probe".into(),
        timing: "useCard2".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |ctx, _, _| action(ctx)),
    })
    .unwrap();
}

fn run(game: &mut Game, program: Program) {
    game.scheduler.start(program);
    game.run(None);
}

#[test]
fn physical_and_virtual_tao_pay_before_trigger_and_heal_added_target() {
    for virtual_card in [false, true] {
        let mut game = table();
        let material = card(
            1,
            if virtual_card {
                CardKind::Sha
            } else {
                CardKind::Tao
            },
        );
        game.seat_mut(0).hand.push(material);
        game.seat_mut(0).hp = 2;
        game.seat_mut(1).hp = 2;
        let seen = Arc::new(Mutex::new(0));
        let seen2 = seen.clone();
        observe(&mut game, move |ctx| {
            *seen2.lock().unwrap() += 1;
            assert!(!ctx.game().seat(0).unwrap().hand.contains(&material));
            assert!(ctx.game().resolving.contains(&material));
            ctx.add_use_target(1).unwrap();
            TriggerStep::Done
        });
        run(
            &mut game,
            Program::BasicUse(Flow::tao(0, 0, material, virtual_card)),
        );
        assert_eq!(*seen.lock().unwrap(), 1);
        assert_eq!((game.seat(0).unwrap().hp, game.seat(1).unwrap().hp), (3, 3));
        assert_eq!(
            game.discard
                .iter()
                .filter(|&&card| card == material)
                .count(),
            1
        );
        assert!(!game.resolving.contains(&material));
    }
}

#[test]
fn shan_use_triggers_after_payment_but_respond_does_not() {
    for use_card in [true, false] {
        let mut game = table();
        let shan = card(1, CardKind::Shan);
        game.seat_mut(0).hand.push(shan);
        let seen = Arc::new(Mutex::new(0));
        let seen2 = seen.clone();
        observe(&mut game, move |ctx| {
            assert!(ctx.game().resolving.contains(&shan));
            *seen2.lock().unwrap() += 1;
            TriggerStep::Done
        });
        run(&mut game, Program::BasicUse(Flow::shan(0, shan, use_card)));
        assert_eq!(*seen.lock().unwrap(), usize::from(use_card));
        assert_eq!(game.discard.iter().filter(|&&card| card == shan).count(), 1);
    }
}

#[test]
fn cancelled_equipment_use_discards_paid_card_without_installing() {
    let mut game = table();
    let armor = card(1, CardKind::Bagua);
    game.seat_mut(0).hand.push(armor);
    observe(&mut game, move |ctx| {
        assert!(ctx.game().resolving.contains(&armor));
        TriggerStep::CancelSubject
    });
    let program = game
        .equipment_uses(game.seat(0).unwrap(), armor)
        .unwrap()
        .remove(0)
        .program;
    run(&mut game, program);
    assert!(!game.seat(0).unwrap().equipment.contains(&armor));
    assert_eq!(
        game.discard.iter().filter(|&&card| card == armor).count(),
        1
    );
}

#[test]
fn cancelled_wuxie_pays_once_and_does_not_respond() {
    let mut game = table();
    let wuxie = card(1, CardKind::Wuxie);
    let duel = card(2, CardKind::Juedou);
    game.seat_mut(0).hand.extend([wuxie, duel]);
    observe(&mut game, move |ctx| {
        if ctx.event().card_kind == Some(CardKind::Wuxie) {
            assert!(ctx.game().resolving.contains(&wuxie));
            TriggerStep::CancelSubject
        } else {
            TriggerStep::Done
        }
    });
    let program = game
        .trick_uses(game.seat(0).unwrap(), duel)
        .unwrap()
        .into_iter()
        .find(|item| item.targets == vec![1])
        .unwrap()
        .program;
    run(&mut game, program);
    let mut attempted = false;
    let mut cancelled = false;
    while let Some(crate::Decision::Cards(choice)) = game.decision.as_ref() {
        let seat = choice.seat;
        let play = !attempted && seat == 0 && choice.counter.is_some();
        if play {
            attempted = true;
        }
        let event = choice.event;
        game.submit(crate::Command {
            version: game.version,
            seat,
            action: crate::Action::Select {
                event,
                branch: u32::from(play),
                cards: if play { vec![wuxie.id] } else { vec![] },
                targets: vec![],
            },
        })
        .unwrap();
        cancelled |= game.events.iter().any(|event| {
            event.kind
                == EventKind::UseResponseCard {
                    seat: 0,
                    card: wuxie,
                }
                && event.status == crate::EventStatus::Cancelled
        });
    }
    assert!(attempted);
    assert_eq!(
        game.seat(1).unwrap().hp,
        3,
        "cancelled Wuxie must not negate the Duel"
    );
    assert_eq!(
        game.discard.iter().filter(|&&card| card == wuxie).count(),
        1
    );
    assert!(!game.resolving.contains(&wuxie));
    assert!(cancelled);
}

#[test]
fn choose_shan_preserves_use_and_respond_context() {
    for uses_card in [true, false] {
        let mut game = table();
        let shan = card(1, CardKind::Shan);
        game.seat_mut(0).hand.push(shan);
        let seen = Arc::new(Mutex::new(0));
        let seen2 = seen.clone();
        observe(&mut game, move |_| {
            *seen2.lock().unwrap() += 1;
            TriggerStep::Done
        });
        let flow = if uses_card {
            crate::classic::equipment::Flow::response(0, CardKind::Shan, false)
        } else {
            crate::classic::equipment::Flow::respond(0, CardKind::Shan)
        };
        run(&mut game, Program::Equipment(flow));
        let view = game.view(Some(0)).unwrap();
        let choice = view.choice.unwrap();
        game.submit(crate::Command {
            version: view.version,
            seat: 0,
            action: crate::Action::Choose {
                event: choice.event,
                card: Some(shan.id),
            },
        })
        .unwrap();
        assert_eq!(*seen.lock().unwrap(), usize::from(uses_card));
        assert_eq!(game.discard.iter().filter(|&&card| card == shan).count(), 1);
    }
}
