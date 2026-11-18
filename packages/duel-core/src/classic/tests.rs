use super::{Classic, TableRandom};
use crate::classic_data::{Role, Side};
use crate::{
    Action, Card, CardKind, ChoiceKind, Command, DrawReason, EventKind, EventStatus, Game,
    GameView, Phase, Program, RuleError, Seat, SeatId, SeatSetup, TurnView,
};

pub(super) fn card(id: u32, kind: CardKind) -> Card {
    Card { id, kind }
}

pub(super) fn scene(roles: &[Role], active: SeatId) -> Game {
    let seats = roles
        .iter()
        .enumerate()
        .map(|(i, role)| {
            let mut seat = Seat::from(SeatSetup {
                id: i as SeatId,
                hp: 4,
                hand: Vec::new(),
            });
            seat.role = Some(*role);
            seat
        })
        .collect();
    let mut game = Game::with_seats(seats, active);
    game.classic = Some(Classic {
        lord: 0,
        human: active,
        deck: (100..110).map(|id| card(id, CardKind::Sha)).collect(),
        faces: Vec::new(),
        random: TableRandom(0),
        turn: Some(TurnView {
            seat: active,
            number: 1,
            round: 1,
            phase: Phase::Play,
        }),
        sha_played: false,
        hero_turn: super::skills::HeroTurn::default(),
        skip_play: false,
        result: None,
        result_announced: false,
    });
    game
}

pub(super) fn pass_counters(game: &mut Game) {
    pass_counters_until(game, |_| false);
}

pub(super) fn pass_counters_until(game: &mut Game, stop: impl Fn(&crate::ChoiceView) -> bool) {
    while let Some(crate::Decision::Cards(choice)) = game.decision.as_ref() {
        if choice.counter.is_none() || stop(choice) {
            break;
        }
        game.submit(Command {
            version: game.version,
            seat: choice.seat,
            action: Action::Select {
                event: choice.event,
                branch: 0,
                cards: vec![],
                targets: vec![],
            },
        })
        .unwrap();
    }
}

fn damage(game: &mut Game, source: SeatId, target: SeatId, amount: u16) {
    game.scheduler.start(Program::event(EventKind::Damage {
        source: Some(source),
        target,
        amount,
    }));
    game.run(None);
}

fn pass_all_saves(game: &mut Game) -> (Vec<SeatId>, GameView) {
    let mut order = Vec::new();
    loop {
        let public = game.view(None).unwrap();
        let Some(actor) = public.waiting_for else {
            return (order, public);
        };
        let private = game.view(Some(actor)).unwrap();
        let Some(choice) = private.choice else {
            return (order, public);
        };
        assert_eq!(choice.kind, ChoiceKind::Save);
        order.push(actor);
        game.submit(Command {
            version: private.version,
            seat: actor,
            action: Action::Choose {
                event: choice.event,
                card: None,
            },
        })
        .unwrap();
    }
}

#[test]
fn dying_order_starts_with_turn_and_allows_repeated_tao_from_same_actor() {
    let mut game = scene(
        &[Role::Lord, Role::Loyalist, Role::Rebel, Role::Renegade],
        2,
    );
    let tao_a = card(1, CardKind::Tao);
    let tao_b = card(2, CardKind::Tao);
    game.seat_mut(1).hp = 1;
    game.seat_mut(2).hand = vec![tao_a, tao_b];
    damage(&mut game, 3, 1, 2);

    let public = game.view(None).unwrap();
    assert_eq!(public.waiting_for, Some(2));
    assert_eq!(public.seats[1].hp, -1);
    assert!(public.seats[1].alive);
    assert!(public.choice.is_none());
    assert!(public.events.iter().any(|event| event.kind
        == EventKind::Dying {
            source: Some(3),
            target: 1
        }));

    let first = game.view(Some(2)).unwrap();
    let first_choice = first.choice.unwrap();
    assert_eq!(first_choice.target, Some(1));
    assert_eq!(first_choice.cards, [tao_a.id, tao_b.id]);
    game.submit(Command {
        version: first.version,
        seat: 2,
        action: Action::Choose {
            event: first_choice.event,
            card: Some(tao_a.id),
        },
    })
    .unwrap();
    let second = game.view(Some(2)).unwrap();
    let second_choice = second.choice.unwrap();
    assert_eq!(second_choice.event, first_choice.event);
    assert_eq!(second_choice.cards, [tao_b.id]);
    assert_eq!(second.seats[1].hp, 0);
    assert_eq!(second.discard, [tao_a]);

    game.submit(Command {
        version: second.version,
        seat: 2,
        action: Action::Choose {
            event: second_choice.event,
            card: Some(tao_b.id),
        },
    })
    .unwrap();
    let saved = game.view(None).unwrap();
    assert_eq!(saved.seats[1].hp, 1);
    assert!(saved.seats[1].alive);
    assert_eq!(saved.seats[1].role, None);
    assert_eq!(saved.discard, [tao_a, tao_b]);
    assert!(saved.resolving.is_empty());
    assert!(!saved
        .events
        .iter()
        .any(|event| matches!(event.kind, EventKind::Death { .. })));
}

#[test]
fn lord_penalty_cleans_hand_and_equipment_after_death_and_skips_dead_seat() {
    let mut game = scene(
        &[Role::Lord, Role::Loyalist, Role::Rebel, Role::Renegade],
        0,
    );
    let lord_hand = card(1, CardKind::Sha);
    let lord_equipment = card(2, CardKind::Bagua);
    let dead_hand = card(3, CardKind::Shan);
    let dead_equipment = card(4, CardKind::Zhuge);
    let dead_judgment = card(5, CardKind::Lebu);
    game.seat_mut(0).hand = vec![lord_hand];
    game.seat_mut(0).equipment = vec![lord_equipment];
    game.seat_mut(1).hand = vec![dead_hand];
    game.seat_mut(1).equipment = vec![dead_equipment];
    game.seat_mut(1).judges = vec![dead_judgment];
    game.seat_mut(1).hp = 1;
    assert_eq!(game.distance(0, 2), Ok(2));
    damage(&mut game, 0, 1, 1);

    let (order, settled) = pass_all_saves(&mut game);
    assert_eq!(order, [0, 1, 2, 3]);
    assert_eq!(
        settled.discard,
        [
            dead_hand,
            dead_equipment,
            dead_judgment,
            lord_hand,
            lord_equipment
        ]
    );
    assert_eq!(settled.seats[0].hand_count, 0);
    assert!(settled.seats[0].equipment.is_empty());
    assert_eq!(settled.seats[1].role, Some(Role::Loyalist));
    assert!(!settled.seats[1].alive);
    assert_eq!(settled.active_seat, 2);
    assert_eq!(game.distance(0, 2), Ok(1));
    assert_eq!(
        game.distance(0, 1),
        Err(RuleError::InactiveSeat { seat: 1 })
    );
    assert!(settled
        .events
        .iter()
        .any(|event| event.status == EventStatus::Started
            && event.kind == EventKind::Penalty { lord: 0 }));
}

#[test]
fn rebel_reward_draws_three_after_death_on_an_ongoing_table() {
    let mut game = scene(
        &[Role::Lord, Role::Loyalist, Role::Rebel, Role::Renegade],
        0,
    );
    game.seat_mut(2).hp = 1;
    let dead_hand = card(3, CardKind::Shan);
    game.seat_mut(2).hand = vec![dead_hand];
    damage(&mut game, 0, 2, 1);

    let (_, settled) = pass_all_saves(&mut game);
    assert_eq!(settled.seats[0].hand_count, 3);
    assert_eq!(settled.discard, [dead_hand]);
    assert_eq!(settled.seats[2].role, Some(Role::Rebel));
    assert!(!settled.seats[2].alive);
    let movement = settled
        .events
        .iter()
        .filter_map(|event| match (event.status, event.kind) {
            (EventStatus::Started, EventKind::Discard { .. }) => Some("discard"),
            (
                EventStatus::Started,
                EventKind::Draw {
                    amount: 3,
                    reason: DrawReason::RebelReward,
                    ..
                },
            ) => Some("reward"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(movement, ["discard", "reward"]);
}

#[test]
fn killer_dying_during_the_victims_discard_cannot_receive_a_reward() {
    use crate::{TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep};
    use std::sync::Arc;
    let mut game = scene(&[Role::Lord, Role::Loyalist, Role::Rebel, Role::Rebel], 0);
    game.seat_mut(1).hp = 1;
    game.seat_mut(2).hp = 1;
    let material = card(900, CardKind::Sha);
    game.seat_mut(2).hand.push(material);
    game.register_trigger(TriggerRegistration {
        owner: 0, skill: "nested_death".into(), timing: "loseAfter".into(), role: TriggerRole::Global,
        order: TriggerOrder::First, priority: 0, label: None, silent: true,
        filter: Arc::new(move |game, event, _| !game.is_alive(2) && event.player == Some(2) && event.cards.contains(&material)),
        effect: Arc::new(|ctx, _, _| { ctx.lose_hp(1, 1).unwrap(); TriggerStep::Done }),
    }).unwrap();
    damage(&mut game, 1, 2, 1);
    pass_all_saves(&mut game);
    assert!(!game.is_alive(1) && !game.is_alive(2));
    assert!(game.seat(1).unwrap().hand.is_empty());
    assert_eq!(game.discard.iter().filter(|&&card| card == material).count(), 1);
}

#[test]
fn terminal_death_freezes_after_cleanup_and_before_a_kill_reward() {
    let mut game = scene(&[Role::Lord, Role::Rebel], 0);
    game.seat_mut(1).hp = 1;
    let dead_hand = card(7, CardKind::Shan);
    game.seat_mut(1).hand = vec![dead_hand];
    damage(&mut game, 0, 1, 1);

    let (_, settled) = pass_all_saves(&mut game);
    assert_eq!(settled.winner, Some(Side::Court));
    assert_eq!(settled.discard, [dead_hand]);
    assert_eq!(settled.seats[0].hand_count, 0);
    assert!(!settled.events.iter().any(|event| matches!(
        event.kind,
        EventKind::Draw {
            reason: DrawReason::RebelReward,
            ..
        }
    )));
    assert_eq!(
        settled.events.last().unwrap().kind,
        EventKind::GameOver {
            winner: Some(Side::Court)
        }
    );
    assert_eq!(
        game.submit(Command {
            version: settled.version,
            seat: 0,
            action: Action::EndPlay,
        }),
        Err(RuleError::GameOver)
    );
    assert_eq!(game.view(None).unwrap(), settled);
}

#[test]
fn refill_reuses_only_discarded_cards_and_exhaustion_draws_the_game() {
    let mut game = scene(&[Role::Lord, Role::Rebel], 0);
    game.classic.as_mut().unwrap().deck.clear();
    game.discard = (1..=4).map(|id| card(id, CardKind::Shan)).collect();
    game.scheduler.start(Program::event(EventKind::Draw {
        seat: 0,
        amount: 2,
        reason: DrawReason::Phase,
    }));
    game.run(None);

    let ready = game.view(Some(1)).unwrap();
    assert_eq!(ready.waiting_for, Some(1));
    assert_eq!(ready.deck_count, 0);
    assert!(ready.discard.is_empty());
    let mut held: Vec<_> = game
        .seats
        .iter()
        .flat_map(|seat| seat.hand.iter().map(|card| card.id))
        .collect();
    held.sort_unstable();
    assert_eq!(held, [1, 2, 3, 4]);

    game.submit(Command {
        version: ready.version,
        seat: 1,
        action: Action::EndPlay,
    })
    .unwrap();
    let exhausted = game.view(None).unwrap();
    assert!(exhausted.drawn);
    assert!(exhausted.winner.is_none());
    assert!(exhausted.events.iter().any(|event| matches!(
        event.kind,
        EventKind::Draw { seat: 0, .. }
    ) && event.status == EventStatus::Cancelled));
    assert_eq!(
        exhausted.events.last().unwrap().kind,
        EventKind::GameOver { winner: None }
    );
}

#[test]
#[ignore = "run in release mode to record the 100-game benchmark"]
fn benchmark_seeded_eight_player_games() {
    use std::collections::HashSet;
    use std::time::Instant;

    let mut rules_times = Vec::new();
    let mut frame_times = Vec::new();
    let mut counts = Vec::new();
    let mut max_events = 0;
    let mut max_bytes = 0;
    let mut timeout_seeds = Vec::new();
    let roster = crate::classic_data::roster();
    for seed in 1..=100 {
        let hero = &roster[seed as usize % roster.len()].id;
        let mut game = Game::classic(seed, 0, hero, 8).unwrap();
        game.classic.as_mut().unwrap().human = SeatId::MAX;
        let mut steps = 0;
        while game.match_result().is_none() && steps < 3_000 {
            let started = Instant::now();
            let command = game
                .bot_command()
                .unwrap_or_else(|| panic!("seed {seed} stalled at version {}", game.version));
            let result = game.submit(command.clone());
            assert_eq!(result, Ok(()), "seed {seed}, command {command:?}");
            rules_times.push(started.elapsed().as_nanos());

            let started = Instant::now();
            let view = game.view(Some(0)).unwrap();
            let bytes = serde_json::to_vec(&view).unwrap();
            frame_times.push(started.elapsed().as_nanos());
            max_events = max_events.max(view.events.len());
            max_bytes = max_bytes.max(bytes.len());
            steps += 1;
        }
        counts.push(steps);
        if game.match_result().is_none() {
            timeout_seeds.push(seed);
            continue;
        }
        assert!(
            game.decision.is_none(),
            "seed {seed} requested input after termination"
        );
        assert!(
            game.resolving.is_empty(),
            "seed {seed} left cards resolving"
        );
        let zones = game
            .seats
            .iter()
            .flat_map(|seat| {
                seat.hand
                    .iter()
                    .chain(&seat.equipment)
                    .chain(&seat.judges)
                    .chain(&seat.expansions)
            })
            .chain(&game.discard)
            .chain(&game.classic.as_ref().unwrap().deck);
        let mut physical_ids = HashSet::new();
        for card in zones {
            assert!(
                physical_ids.insert(card.id),
                "seed {seed} duplicated card {}",
                card.id
            );
        }
        assert_eq!(physical_ids.len(), 104, "seed {seed} lost physical cards");
    }
    rules_times.sort_unstable();
    frame_times.sort_unstable();
    counts.sort_unstable();
    let p95 = (95 * rules_times.len()).div_ceil(100) - 1;
    println!(
        "games=100 nonterminal={timeout_seeds:?} decisions={} min={} median={} max={} rules_p95_ns={} frame_and_json_p95_ns={} max_events={max_events} max_bytes={max_bytes}",
        rules_times.len(), counts[0], counts[49], counts[99], rules_times[p95], frame_times[p95],
    );
    assert!(timeout_seeds.is_empty());
}
