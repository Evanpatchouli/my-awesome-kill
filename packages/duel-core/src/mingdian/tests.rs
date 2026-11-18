use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{Action, Command, EventKind, EventStatus, Phase, Program};

fn table() -> Game {
    scene(
        &[
            Role::Lord,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Renegade,
        ],
        0,
    )
}

fn submit(game: &mut Game, action: Action) {
    game.submit(Command {
        version: game.version,
        seat: 0,
        action,
    })
    .unwrap();
}

#[test]
fn damage_opens_a_private_choice_of_two_ordinary_basics() {
    let mut game = table();
    let sha = card(1, CardKind::Sha);
    let shan = card(2, CardKind::Shan);
    let tiesuo = card(3, CardKind::Tiesuo);
    game.seat_mut(0).hand.extend([sha, shan, tiesuo]);
    install(&mut game, 0).unwrap();
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "mingdian_test_damage".into(),
        timing: "phaseUseBegin".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|ctx, _, _| {
            ctx.damage(Some(1), 0, 1, None, None, &[]).unwrap();
            TriggerStep::Done
        }),
    })
    .unwrap();
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    assert_eq!(game.seat(0).unwrap().hp, 3);
    assert!(game.view(Some(1)).unwrap().choice.is_none());
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    let branch = choice
        .branches
        .iter()
        .find(|branch| branch.label == "明典")
        .unwrap();
    assert_eq!(branch.cards, [sha.id, shan.id]);
    assert_eq!((branch.min_cards, branch.max_cards), (1, 2));
    submit(
        &mut game,
        Action::Select {
            event: choice.event,
            branch: branch.id,
            cards: vec![shan.id, sha.id],
            targets: vec![],
        },
    );
    assert_eq!(game.discard, [shan, sha]);
    assert_eq!(
        game.seat(0).unwrap().hand,
        [card(109, CardKind::Sha), card(108, CardKind::Sha), tiesuo]
    );
    let recast = game
        .events
        .iter()
        .find(|event| {
            matches!(event.kind, EventKind::Recast { seat: 0 })
                && event.status == EventStatus::Finished
        })
        .unwrap();
    assert!(game.events.iter().any(|event| {
        event.parent == Some(recast.id)
            && event.status == EventStatus::Finished
            && matches!(
                event.kind,
                EventKind::Draw {
                    seat: 0,
                    amount: 2,
                    ..
                }
            )
    }));
    assert!(game.view(Some(1)).unwrap().seats[0].known_hand.is_empty());
    assert_eq!(game.view(Some(0)).unwrap().waiting_for, Some(0));
    assert!(game.active_plays(0).is_empty());
}

#[test]
fn a_previous_basic_use_earns_a_public_peach_and_names_reset_next_round() {
    let mut game = table();
    let attack = card(1, CardKind::Sha);
    let sha = card(2, CardKind::Sha);
    let shan = card(3, CardKind::Shan);
    let peach = card(4, CardKind::Tao);
    game.seat_mut(0).hand.extend([attack, sha, shan]);
    game.discard.push(peach);
    install(&mut game, 0).unwrap();
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    submit(
        &mut game,
        Action::PlayCard {
            card: attack.id,
            targets: vec![1],
        },
    );
    let response = game.view(Some(1)).unwrap().choice.unwrap();
    assert_eq!(response.kind, crate::ChoiceKind::Shan);
    game.submit(Command {
        version: game.version,
        seat: 1,
        action: Action::Choose {
            event: response.event,
            card: None,
        },
    })
    .unwrap();
    assert_eq!(game.seat(1).unwrap().hp, 3);
    let active = &game.active_plays(0)[0];
    let registration = active.registration;
    assert_eq!(active.branch.cards, [sha.id, shan.id]);
    submit(
        &mut game,
        Action::ActivateSkill {
            registration,
            cards: vec![sha.id, shan.id],
            targets: vec![],
        },
    );
    assert_eq!(game.discard, [attack, sha, shan]);
    assert_eq!(
        game.seat(0).unwrap().hand,
        [peach, card(109, CardKind::Sha), card(108, CardKind::Sha)]
    );
    assert_eq!(game.view(Some(1)).unwrap().seats[0].known_hand, [peach]);
    assert_eq!(game.active_plays(0)[0].branch.cards, [peach.id]);
    game.classic.as_mut().unwrap().turn.as_mut().unwrap().round += 1;
    assert_eq!(game.active_plays(0)[0].branch.cards, [peach.id, 109, 108]);
}
