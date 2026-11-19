use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, ActiveSkillRegistration, Card, CardKind, ChoiceBranch, ChoiceKind, Command, Decision,
    EventStatus, Phase, TriggerContext, TriggerOrder, TriggerRegistration, TriggerRole,
    TriggerSelection, TriggerStep,
};
use std::sync::{Arc, Mutex};

fn roles(target: Role) -> [Role; 8] {
    [
        Role::Lord,
        target,
        Role::Loyalist,
        Role::Rebel,
        Role::Loyalist,
        Role::Rebel,
        Role::Loyalist,
        Role::Rebel,
    ]
}

#[test]
fn a_prescribed_table_can_lose_its_maximum_without_an_identity_role() {
    let mut game = Game::new(crate::GameSetup {
        active_seat: 3,
        seats: vec![
            crate::SeatSetup {
                id: 3,
                hp: 4,
                hand: vec![],
            },
            crate::SeatSetup {
                id: 7,
                hp: 1,
                hand: vec![],
            },
        ],
    })
    .unwrap();
    let operation = game.max_hp_program(7, Some(3), MaxHpKind::Lose, 2).unwrap();
    game.scheduler.start(operation);
    game.run(None);
    assert!(!game.is_alive(7));
    assert_eq!(game.health(7), Some((-1, -1)));
    assert!(game.view(None).unwrap().events.iter().any(|event| {
        event.status == EventStatus::Finished
            && matches!(
                event.kind,
                EventKind::Death {
                    target: 7,
                    source: Some(3),
                    role: None
                }
            )
    }));
}

fn register_active(
    game: &mut Game,
    owner: SeatId,
    name: &str,
    effect: impl Fn(&mut TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) -> u64 {
    game.register_active_skill(ActiveSkillRegistration {
        owner,
        skill: name.into(),
        limited: false,
        choice: Arc::new(|_, _| {
            Some(ChoiceBranch {
                id: 0,
                label: "regression fixture".into(),
                cards: vec![],
                min_cards: 0,
                max_cards: 0,
                targets: vec![],
                min_targets: 0,
                max_targets: 0,
            })
        }),
        effect: Arc::new(effect),
    })
    .unwrap()
}

fn trigger(owner: SeatId, name: &str, timing: &str) -> TriggerRegistration {
    TriggerRegistration {
        owner,
        skill: name.into(),
        timing: timing.into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|_, _, _| TriggerStep::Done),
    }
}

fn start_play(game: &mut Game, seat: SeatId) {
    game.scheduler.start(Program::event(EventKind::Phase {
        seat,
        phase: Phase::Play,
    }));
    game.run(None);
    assert!(matches!(&game.decision, Some(Decision::Play(actor)) if *actor == seat));
}

fn submit(game: &mut Game, seat: SeatId, action: Action) {
    game.submit(Command {
        version: game.version,
        seat,
        action,
    })
    .unwrap();
}

fn activate(game: &mut Game, owner: SeatId, registration: u64) {
    submit(
        game,
        owner,
        Action::ActivateSkill {
            registration,
            cards: vec![],
            targets: vec![],
        },
    );
}

fn save_choice(game: &Game, actor: SeatId, target: SeatId) -> crate::ChoiceView {
    let view = game.view(Some(actor)).unwrap();
    assert_eq!(view.waiting_for, Some(actor));
    let choice = view.choice.expect("a rescue choice is pending");
    assert_eq!(choice.kind, ChoiceKind::Save);
    assert_eq!(choice.target, Some(target));
    choice
}

fn choose_save(game: &mut Game, actor: SeatId, target: SeatId, card: Option<Card>) {
    let choice = save_choice(game, actor, target);
    if let Some(card) = card {
        assert!(
            choice.cards.contains(&card.id),
            "the real Tao must be offered"
        );
    }
    submit(
        game,
        actor,
        Action::Choose {
            event: choice.event,
            card: card.map(|card| card.id),
        },
    );
}

#[test]
fn rescue_continues_after_the_saver_dies_and_stops_if_the_patient_dies() {
    for victim in [1, 2] {
        let mut game = scene(&roles(Role::Loyalist), 1);
        let first_tao = card(1, CardKind::Tao);
        let second_tao = card(2, CardKind::Tao);
        game.seat_mut(1).hand.push(first_tao);
        game.seat_mut(3).hand.push(second_tao);
        let observed = Arc::new(Mutex::new(Vec::new()));
        let log = observed.clone();
        let mut die_during_tao = trigger(0, "paid_tao_death", "useCard2");
        die_during_tao.filter = Arc::new(move |_, event, _| {
            event.player == Some(1)
                && event.card_kind == Some(CardKind::Tao)
                && event.cards == [first_tao]
        });
        die_during_tao.effect = Arc::new(move |ctx, _, _| {
            let game = ctx.game();
            log.lock().unwrap().push((
                game.seat(2).unwrap().hp,
                !game.seat(1).unwrap().hand.contains(&first_tao),
                game.resolving.contains(&first_tao),
            ));
            ctx.lose_max_hp(victim, 4, None).unwrap();
            TriggerStep::Done
        });
        game.register_trigger(die_during_tao).unwrap();
        let harm = register_active(&mut game, 1, "begin_rescue", |ctx, _, _| {
            ctx.lose_hp(2, 5).unwrap();
            TriggerStep::Done
        });
        start_play(&mut game, 1);
        activate(&mut game, 1, harm);
        assert_eq!(game.health(2), Some((-1, 4)));
        assert!(game.is_alive(2));
        choose_save(&mut game, 1, 2, Some(first_tao));

        assert_eq!(*observed.lock().unwrap(), [(-1, true, true)]);
        assert!(game.discard.contains(&first_tao));
        assert!(!game.is_alive(victim));
        assert!(game.match_result().is_none());
        let deaths: Vec<_> = game
            .events
            .iter()
            .filter_map(|event| {
                if event.status == EventStatus::Started {
                    if let EventKind::Death { target, .. } = event.kind {
                        return Some(target);
                    }
                }
                None
            })
            .collect();
        assert_eq!(
            deaths,
            [victim],
            "exactly one death starts during the paid Tao"
        );

        if victim == 1 {
            assert!(game.is_alive(2));
            assert_eq!(game.health(2), Some((0, 4)));
            // The dying seat is next after the now-dead rescuer; passing it
            // advances to seat 3, which can pay a second physical Tao.
            choose_save(&mut game, 2, 2, None);
            choose_save(&mut game, 3, 2, Some(second_tao));
            assert!(game.is_alive(2));
            assert_eq!(game.health(2), Some((1, 4)));
            assert!(game.discard.contains(&second_tao));
        } else {
            assert!(game.is_alive(1));
            assert_eq!(game.health(2), Some((0, 0)));
            assert!(game.view(None).unwrap().choice.is_none());
            assert!(!game
                .events
                .iter()
                .any(|event| event.status == EventStatus::Started
                    && matches!(event.kind, EventKind::ChooseSave { .. })));
            assert!(game.seat(3).unwrap().hand.contains(&second_tao));
        }
    }
}

#[test]
fn invalidated_queued_target_does_not_discard_later_operations_or_continuation() {
    let mut game = scene(&roles(Role::Loyalist), 0);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let log = observed.clone();
    let active = register_active(
        &mut game,
        0,
        "queued_max_hp",
        move |ctx, stage, _| match stage {
            0 => {
                ctx.lose_max_hp(1, 4, None).unwrap();
                ctx.gain_max_hp(1, 1, None).unwrap();
                ctx.gain_max_hp(2, 1, None).unwrap();
                TriggerStep::Continue(1)
            }
            1 => {
                log.lock().unwrap().push((
                    ctx.game().is_alive(1),
                    ctx.game().health(1),
                    ctx.game().health(2),
                ));
                TriggerStep::Done
            }
            _ => unreachable!(),
        },
    );
    start_play(&mut game, 0);
    activate(&mut game, 0, active);
    assert_eq!(
        *observed.lock().unwrap(),
        [(false, Some((0, 0)), Some((4, 5)))]
    );
    assert!(game.is_alive(2));
    assert!(game.match_result().is_none());
    let started: Vec<_> = game
        .events
        .iter()
        .filter_map(|event| {
            if event.status != EventStatus::Started {
                return None;
            }
            match event.kind {
                EventKind::LoseMaxHp { seat, .. } => Some(("lose", seat)),
                EventKind::GainMaxHp { seat, .. } => Some(("gain", seat)),
                EventKind::Death { target, .. } => Some(("death", target)),
                _ => None,
            }
        })
        .collect();
    assert_eq!(started, [("lose", 1), ("death", 1), ("gain", 2)]);
}

#[test]
fn extreme_health_preserves_bot_order_and_death_change_hp_delta() {
    let mut game = scene(&roles(Role::Rebel), 0);
    let sha = card(80, CardKind::Sha);
    game.seat_mut(0).hand.push(sha);
    let stages = Arc::new(Mutex::new(Vec::new()));
    let log = stages.clone();
    let setup = register_active(&mut game, 0, "extreme_health", move |ctx, stage, _| {
        let seat = ctx.game().seat(1).unwrap();
        log.lock()
            .unwrap()
            .push((stage, seat.hp, seat.max_hp, seat.alive));
        match stage {
            0 => {
                ctx.gain_max_hp(1, -4, None).unwrap();
                TriggerStep::Continue(1)
            }
            1 => {
                ctx.gain_max_hp(1, i32::MIN, None).unwrap();
                TriggerStep::Continue(2)
            }
            2 => {
                ctx.gain_max_hp(1, i32::MAX, None).unwrap();
                TriggerStep::Continue(3)
            }
            3 => {
                ctx.gain_max_hp(1, 1, None).unwrap();
                TriggerStep::Continue(4)
            }
            4 => TriggerStep::Done,
            _ => unreachable!(),
        }
    });
    let finish = register_active(&mut game, 0, "zero_loss_death", |ctx, _, _| {
        ctx.lose_max_hp(1, 0, None).unwrap();
        TriggerStep::Done
    });
    let observed = Arc::new(Mutex::new(Vec::new()));
    for timing in ["changeHpBefore", "changeHp"] {
        let mut probe = trigger(2, &format!("wide_{timing}"), timing);
        probe.filter = Arc::new(|_, event, _| event.player == Some(1));
        let log = observed.clone();
        probe.effect = Arc::new(move |ctx, _, _| {
            let player = ctx.game().seat(1).unwrap();
            log.lock().unwrap().push((
                ctx.event().timing.clone(),
                ctx.event().delta,
                ctx.event().subject_number(ctx.game()),
                player.hp,
                player.max_hp,
                player.alive,
            ));
            TriggerStep::Done
        });
        game.register_trigger(probe).unwrap();
    }
    start_play(&mut game, 0);
    activate(&mut game, 0, setup);
    assert_eq!(
        *stages.lock().unwrap(),
        [
            (0, 4, 4, true),
            (1, 0, 0, true),
            (2, i32::MIN, i32::MIN, true),
            (3, i32::MIN, -1, true),
            (4, i32::MIN, 0, true),
        ]
    );
    let priority = game.bot_card_priority(game.seat(0).unwrap(), CardKind::Sha, &[1], 0);
    assert_eq!(priority, (3, 2_147_483_648_i64));
    // Ask the actual bot to choose between two legal adjacent targets with the
    // same attack priority, using a physical Sha and the extreme raw HP.
    assert!(game.play_choices(0).contains(&Action::PlayCard {
        card: sha.id,
        targets: vec![1]
    }));
    assert!(game.play_choices(0).contains(&Action::PlayCard {
        card: sha.id,
        targets: vec![7]
    }));
    game.classic.as_mut().unwrap().human = 7;
    let bot = game.bot_command().expect("non-human play decision");
    assert_eq!(bot.seat, 0);
    assert_eq!(
        bot.action,
        Action::PlayCard {
            card: sha.id,
            targets: vec![1]
        }
    );

    activate(&mut game, 0, finish);
    assert_eq!(
        *observed.lock().unwrap(),
        [
            (
                "changeHpBefore".to_owned(),
                Some(2_147_483_648_i64),
                Some(2_147_483_648_i64),
                i32::MIN,
                0,
                false
            ),
            (
                "changeHp".to_owned(),
                Some(2_147_483_648_i64),
                Some(2_147_483_648_i64),
                0,
                0,
                false
            ),
        ]
    );
    assert_eq!(game.health(1), Some((0, 0)));
    assert!(!game.is_alive(1));
    assert!(game.match_result().is_none());
    assert!(game.events.iter().any(|event| matches!(
        event.kind,
        EventKind::ChangeHp {
            seat: 1,
            before: i32::MIN,
            after: 0
        }
    )));
}

#[test]
fn cancelled_death_hides_the_living_non_lord_role_in_public_events() {
    for role in [Role::Loyalist, Role::Rebel] {
        let mut game = scene(&roles(role), 0);
        let mut cancel = trigger(0, "cancel_death", "dieBefore");
        cancel.filter = Arc::new(|_, event, _| event.player == Some(1));
        cancel.effect = Arc::new(|_, _, _| TriggerStep::CancelSubject);
        game.register_trigger(cancel).unwrap();
        let active = register_active(&mut game, 0, "try_direct_death", |ctx, _, _| {
            ctx.lose_max_hp(1, 4, None).unwrap();
            TriggerStep::Done
        });
        start_play(&mut game, 0);
        activate(&mut game, 0, active);
        assert!(game.is_alive(1));
        assert_eq!(game.health(1), Some((0, 0)));
        assert!(game.match_result().is_none());
        for viewer in [None, Some(0), Some(2), Some(1)] {
            let view = game.view(viewer).unwrap();
            assert_eq!(
                view.seats[1].role,
                if viewer == Some(1) { Some(role) } else { None }
            );
            let deaths: Vec<_> = view
                .events
                .iter()
                .filter(|event| matches!(event.kind, EventKind::Death { target: 1, .. }))
                .collect();
            assert_eq!(
                deaths.iter().map(|event| event.status).collect::<Vec<_>>(),
                [EventStatus::Started, EventStatus::Cancelled]
            );
            for event in deaths {
                assert!(matches!(
                    event.kind,
                    EventKind::Death {
                        target: 1,
                        role: None,
                        ..
                    }
                ));
                assert!(serde_json::to_value(event).unwrap().get("role").is_none());
            }
        }
    }
}
