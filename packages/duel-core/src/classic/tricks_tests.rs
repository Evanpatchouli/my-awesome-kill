use super::tests::{pass_counters, pass_counters_until};
use crate::{Action, Card, CardKind, Command, CounterContext, Game};

fn card(id: u32, kind: CardKind) -> Card {
    Card { id, kind }
}

#[test]
fn declarative_trick_stops_effects_when_the_deck_exhausts() {
    use crate::content::{ContentPack, Effect};
    let mut pack =
        ContentPack::parse(include_str!("../../data/example-content-pack.json")).unwrap();
    pack.cards[0].effects = vec![Effect::Draw { count: 8 }, Effect::Draw { count: 1 }];
    let mut game = scene();
    game.content = pack.compile().unwrap();
    let trick = card(104, CardKind::Content(pack.cards[0].id));
    let remaining = card(105, CardKind::Shan);
    game.classic.as_mut().unwrap().deck = vec![remaining];
    start_trick(&mut game, trick, vec![0]);
    pass_counters(&mut game);
    let view = game.view(Some(0)).unwrap();
    assert!(view.drawn);
    assert!(!game.seat(0).unwrap().hand.contains(&remaining));
    assert_eq!(view.deck_count, 1);
}

#[test]
fn counterspell_suppresses_all_declarative_card_effects() {
    use crate::content::{ContentPack, Effect};
    let mut pack =
        ContentPack::parse(include_str!("../../data/example-content-pack.json")).unwrap();
    pack.cards[0].effects = vec![Effect::Draw { count: 3 }, Effect::Recover { count: 1 }];
    let mut game = scene();
    game.content = pack.compile().unwrap();
    let trick = card(104, CardKind::Content(pack.cards[0].id));
    let counter = card(105, CardKind::Wuxie);
    game.seat_mut(0).hp = 2;
    game.seat_mut(1).hand.push(counter);
    start_trick(&mut game, trick, vec![0]);
    pass_counters_until(&mut game, |choice| choice.seat == 1);
    let view = game.view(Some(1)).unwrap();
    let choice = view.choice.unwrap();
    let branch = choice
        .branches
        .iter()
        .find(|branch| branch.cards.contains(&counter.id))
        .unwrap();
    game.submit(Command {
        version: view.version,
        seat: 1,
        action: Action::Select {
            event: choice.event,
            branch: branch.id,
            cards: vec![counter.id],
            targets: vec![],
        },
    })
    .unwrap();
    pass_counters(&mut game);
    assert_eq!(game.seat(0).unwrap().hp, 2);
    assert!(game.seat(0).unwrap().hand.is_empty());
    assert!(game.discard.contains(&trick));
}

#[test]
fn target_eligibility_obeys_distance_empty_zones_and_delayed_duplicates() {
    let mut game = Game::classic(3, 0, "zhangfei", 4).unwrap();
    for seat in &mut game.seats {
        seat.hand.clear();
        seat.equipment.clear_test();
        seat.judges.clear();
    }
    game.seat_mut(1).hand.push(card(1000, CardKind::Sha));
    game.seat_mut(2).hand.push(card(1001, CardKind::Sha));
    game.seat_mut(1).judges.push(card(1002, CardKind::Lebu));
    let actor = game.seat(0).unwrap();
    let targets = |kind| {
        game.trick_uses(actor, card(1003, kind))
            .unwrap()
            .into_iter()
            .map(|use_| use_.targets)
            .collect::<Vec<_>>()
    };
    assert_eq!(targets(CardKind::Shunshou), vec![vec![1]]);
    assert_eq!(targets(CardKind::Guohe), vec![vec![1], vec![2]]);
    assert_eq!(targets(CardKind::Lebu), vec![vec![2], vec![3]]);
    assert_eq!(targets(CardKind::Nanman), vec![vec![1, 2, 3]]);
    assert_eq!(targets(CardKind::Wugu), vec![vec![0, 1, 2, 3]]);
    assert!(targets(CardKind::Wuxie).is_empty());
}

#[test]
fn borrowed_weapon_requires_valid_holder_and_reachable_second_target() {
    let mut game = Game::classic(3, 0, "zhangfei", 4).unwrap();
    for seat in &mut game.seats {
        seat.equipment.clear_test();
    }
    game.seat_mut(1)
        .equipment
        .install_test(card(1000, CardKind::Zhuge));
    let actor = game.seat(0).unwrap();
    let targets = game
        .trick_uses(actor, card(1001, CardKind::Jiedao))
        .unwrap()
        .into_iter()
        .map(|use_| use_.targets)
        .collect::<Vec<_>>();
    assert_eq!(targets, vec![vec![1, 2], vec![1, 0]]);
}

#[test]
fn borrowed_weapon_added_targets_use_upstream_positional_victims() {
    use crate::{TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep};
    use std::sync::Arc;
    let mut game = scene();
    game.seat_mut(1)
        .equipment
        .install_test(card(50, CardKind::Zhuge));
    game.seat_mut(2)
        .equipment
        .install_test(card(51, CardKind::Qinglong));
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "add_holder".into(),
        timing: "useCard2".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, event, _| event.card_kind == Some(CardKind::Jiedao)),
        effect: Arc::new(|ctx, _, _| {
            ctx.add_use_target(1).unwrap();
            TriggerStep::Done
        }),
    })
    .unwrap();
    start_trick(&mut game, card(20, CardKind::Jiedao), vec![2, 3]);
    let view = game.view(Some(0)).unwrap();
    assert_eq!(view.waiting_for, Some(0));
    let choice = view.choice.unwrap();
    assert_eq!(choice.target, Some(1));
    let branch = &choice.branches[0];
    assert!(branch.targets.contains(&0));
    game.submit(Command {
        version: view.version,
        seat: 0,
        action: Action::Select {
            event: choice.event,
            branch: branch.id,
            cards: vec![],
            targets: vec![0],
        },
    })
    .unwrap();
    pass_counters(&mut game);
    let choice = game.view(Some(1)).unwrap().choice.unwrap();
    assert_eq!(choice.target, Some(3));
    select(&mut game, 0, vec![]);
    pass_counters(&mut game);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(2));
    assert_eq!(game.view(Some(2)).unwrap().choice.unwrap().target, Some(0));
    select(&mut game, 0, vec![]);
    assert!(game
        .seat(0)
        .unwrap()
        .hand
        .contains(&card(50, CardKind::Zhuge)));
    assert!(game
        .seat(0)
        .unwrap()
        .hand
        .contains(&card(51, CardKind::Qinglong)));
}

#[test]
fn added_trick_targets_resolve_in_turn_order_after_payment() {
    use crate::{TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep};
    use std::sync::Arc;
    let mut game = scene();
    game.seat_mut(1).hand.push(card(41, CardKind::Tao));
    game.seat_mut(2).hand.push(card(42, CardKind::Tao));
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "add_guohe".into(),
        timing: "useCard2".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, event, _| event.card_kind == Some(CardKind::Guohe)),
        effect: Arc::new(|ctx, _, _| {
            assert!(ctx.game().resolving.contains(&card(20, CardKind::Guohe)));
            assert_eq!(
                ctx.game().use_targets(ctx.event().subject),
                Some([2].as_slice())
            );
            ctx.add_use_target(1).unwrap();
            TriggerStep::Done
        }),
    })
    .unwrap();
    start_trick(&mut game, card(20, CardKind::Guohe), vec![2]);
    pass_counters(&mut game);
    assert_eq!(game.view(Some(0)).unwrap().choice.unwrap().target, Some(1));
    select(&mut game, 0, vec![]);
    pass_counters(&mut game);
    assert_eq!(game.view(Some(0)).unwrap().choice.unwrap().target, Some(2));
    select(&mut game, 0, vec![]);
    assert!(game.discard.contains(&card(41, CardKind::Tao)));
    assert!(game.discard.contains(&card(42, CardKind::Tao)));
}

fn scene() -> Game {
    use super::{Classic, TableRandom};
    use crate::classic_data::Role;
    use crate::{Phase, Seat, SeatSetup, TurnView};
    let roles = [Role::Lord, Role::Loyalist, Role::Rebel, Role::Renegade];
    let seats = roles
        .into_iter()
        .enumerate()
        .map(|(id, role)| {
            let mut player = Seat::from(SeatSetup {
                id: id as u32,
                hp: 4,
                hand: vec![],
            });
            player.role = Some(role);
            player
        })
        .collect();
    let mut game = Game::with_seats(seats, 0);
    game.classic = Some(Classic {
        hero_turn: Default::default(),
        human: 0,
        lord: 0,
        deck: (100..120).map(|id| card(id, CardKind::Sha)).collect(),
        faces: vec![],
        random: TableRandom(1),
        turn: Some(TurnView {
            seat: 0,
            number: 1,
            round: 1,
            phase: Phase::Play,
        }),
        sha_played: false,
        result: None,
        result_announced: false,
    });
    game
}

fn start_trick(game: &mut Game, trick: Card, targets: Vec<u32>) {
    game.seat_mut(0).hand.push(trick);
    let flow = game
        .trick_uses(game.seat(0).unwrap(), trick)
        .unwrap()
        .into_iter()
        .find(|item| item.targets == targets)
        .unwrap()
        .program;
    game.scheduler.start(flow);
    game.run(None);
}

fn select(game: &mut Game, branch: u32, cards: Vec<u32>) {
    use crate::{Action, Command};
    let actor = game.view(None).unwrap().waiting_for.unwrap();
    let view = game.view(Some(actor)).unwrap();
    let choice = view.choice.unwrap();
    game.submit(Command {
        version: view.version,
        seat: actor,
        action: Action::Select {
            event: choice.event,
            branch,
            cards,
            targets: vec![],
        },
    })
    .unwrap();
}

#[test]
fn wugu_public_pool_is_taken_in_seat_order_and_consumed_once() {
    let mut game = scene();
    start_trick(&mut game, card(20, CardKind::Wugu), vec![0, 1, 2, 3]);
    assert_eq!(game.view(None).unwrap().resolving.len(), 5);
    for expected in 0..4 {
        pass_counters(&mut game);
        let public = game.view(None).unwrap();
        assert_eq!(public.waiting_for, Some(expected));
        let private = game.view(Some(expected)).unwrap();
        let choice = private.choice.unwrap();
        assert_eq!(choice.branches[0].cards.len(), 4 - expected as usize);
        let chosen = choice.branches[0].cards[0];
        assert!(public.resolving.iter().any(|c| c.id == chosen));
        select(&mut game, 0, vec![chosen]);
        assert!(game
            .seat(expected)
            .unwrap()
            .hand
            .iter()
            .any(|c| c.id == chosen));
    }
    assert!(game.discard.iter().any(|c| c.id == 20));
}

#[test]
fn guohe_hides_hand_id_from_attacker_and_public_choice() {
    let mut game = scene();
    game.seat_mut(1).hand.push(card(42, CardKind::Tao));
    start_trick(&mut game, card(20, CardKind::Guohe), vec![1]);
    pass_counters(&mut game);
    let public = game.view(None).unwrap();
    let attacker = game.view(Some(0)).unwrap();
    let choice = attacker.choice.unwrap();
    assert_eq!(choice.branches.len(), 1);
    assert!(choice.branches[0].cards.is_empty());
    assert!(public.choice.is_none());
    assert!(public.seats[1].hand.is_none());
    select(&mut game, 0, vec![]);
    assert!(game.discard.iter().any(|c| c.id == 42));
}

#[test]
fn wuxie_cancels_only_the_current_aoe_target() {
    let mut game = scene();
    game.seat_mut(2).hand.push(card(42, CardKind::Wuxie));
    start_trick(&mut game, card(20, CardKind::Nanman), vec![1, 2, 3]);
    pass_counters_until(&mut game, |choice| choice.seat == 2);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(2));
    assert!(game.view(Some(2)).unwrap().choice.unwrap().prompt.is_none());
    select(&mut game, 1, vec![42]);
    pass_counters(&mut game);
    assert_eq!(game.seat(1).unwrap().hp, 4, "first target was negated");
    assert_eq!(game.view(None).unwrap().waiting_for, Some(2));
    assert_eq!(
        game.view(Some(2)).unwrap().choice.unwrap().prompt.as_deref(),
        Some("应对【南蛮入侵】，请打出一张【杀】。")
    );
    select(&mut game, 0, vec![]);
    pass_counters(&mut game);
    assert_eq!(game.seat(2).unwrap().hp, 3);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(3));
    select(&mut game, 0, vec![]);
    assert_eq!(game.seat(3).unwrap().hp, 3);
}

#[test]
fn wanjian_prompts_the_target_without_requiring_a_conversion_skill() {
    let mut game = scene();
    game.seat_mut(1).hand.push(card(42, CardKind::Shan));
    start_trick(&mut game, card(20, CardKind::Wanjian), vec![1, 2, 3]);
    pass_counters(&mut game);
    let choice = game.view(Some(1)).unwrap().choice.unwrap();
    assert_eq!(choice.kind, crate::ChoiceKind::Shan);
    assert_eq!(
        choice.prompt.as_deref(),
        Some("应对【万箭齐发】，请打出一张【闪】。")
    );
    assert!(game.view(Some(0)).unwrap().choice.is_none());
}

#[test]
fn successive_wuxie_restore_effect_for_one_target() {
    let mut game = scene();
    game.seat_mut(1).hand.push(card(41, CardKind::Wuxie));
    game.seat_mut(2).hand.push(card(42, CardKind::Wuxie));
    start_trick(&mut game, card(20, CardKind::Nanman), vec![1, 2, 3]);
    pass_counters_until(&mut game, |choice| choice.seat == 1);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    assert!(
        !game
            .view(Some(1))
            .unwrap()
            .choice
            .unwrap()
            .counter
            .unwrap()
            .negated
    );
    select(&mut game, 1, vec![41]);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(2));
    assert!(
        game.view(Some(2))
            .unwrap()
            .choice
            .unwrap()
            .counter
            .unwrap()
            .negated
    );
    select(&mut game, 1, vec![42]);
    assert!(
        !game
            .view(Some(3))
            .unwrap()
            .choice
            .unwrap()
            .counter
            .unwrap()
            .negated
    );
    pass_counters(&mut game);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    select(&mut game, 0, vec![]);
    assert_eq!(game.seat(1).unwrap().hp, 3);
    pass_counters(&mut game);
    select(&mut game, 0, vec![]);
    pass_counters(&mut game);
    select(&mut game, 0, vec![]);
    assert_eq!(game.seat(2).unwrap().hp, 3);
    assert_eq!(game.seat(3).unwrap().hp, 3);
}

#[test]
fn lightning_non_hit_moves_to_next_seat_without_duplicate() {
    use crate::{classic_data::Suit, CardFace};
    let mut game = scene();
    game.classic.as_mut().unwrap().turn.as_mut().unwrap().seat = 2;
    let lightning = card(20, CardKind::Shandian);
    game.seat_mut(0).judges.push(lightning);
    game.seat_mut(1).judges.push(card(21, CardKind::Shandian));
    game.classic.as_mut().unwrap().faces.push(CardFace {
        id: 119,
        suit: Suit::Heart,
        rank: 5,
    });
    let flow = crate::Program::event(crate::EventKind::Phase {
        seat: 0,
        phase: crate::Phase::Judgment,
    });
    game.scheduler.start(flow);
    game.run(None);
    pass_counters(&mut game);
    assert!(game.seat(0).unwrap().judges.is_empty());
    assert_eq!(game.seat(2).unwrap().judges, vec![lightning]);
    assert_eq!(game.seat(0).unwrap().hp, 4);
}

#[test]
fn lethal_lightning_discards_once_before_death_cleanup() {
    use crate::{classic_data::Suit, Action, CardFace, ChoiceKind, Command};
    let mut game = scene();
    game.seat_mut(0).hp = 2;
    let lightning = card(20, CardKind::Shandian);
    game.seat_mut(0).judges.push(lightning);
    game.classic.as_mut().unwrap().faces.push(CardFace {
        id: 119,
        suit: Suit::Spade,
        rank: 5,
    });
    game.scheduler
        .start(crate::Program::event(crate::EventKind::Phase {
            seat: 0,
            phase: crate::Phase::Judgment,
        }));
    game.run(None);
    pass_counters(&mut game);
    while let Some(actor) = game.view(None).unwrap().waiting_for {
        let view = game.view(Some(actor)).unwrap();
        let choice = view.choice.unwrap();
        assert_eq!(choice.kind, ChoiceKind::Save);
        game.submit(Command {
            version: view.version,
            seat: actor,
            action: Action::Choose {
                event: choice.event,
                card: None,
            },
        })
        .unwrap();
    }
    assert_eq!(
        game.discard.iter().filter(|c| c.id == lightning.id).count(),
        1
    );
    assert!(!game.seat(0).unwrap().alive);
}

#[test]
fn borrowed_weapon_transfers_when_holder_declines_to_attack() {
    let mut game = scene();
    let weapon = card(50, CardKind::Zhuge);
    game.seat_mut(1).equipment.install_test(weapon);
    start_trick(&mut game, card(20, CardKind::Jiedao), vec![1, 2]);
    pass_counters(&mut game);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    select(&mut game, 0, vec![]);
    assert!(game.seat(1).unwrap().equipment.is_empty());
    assert!(game.seat(0).unwrap().hand.contains(&weapon));
    assert!(game.discard.iter().any(|c| c.id == 20));
}

#[test]
fn lebu_non_heart_skips_the_affected_turn_play_decision() {
    use crate::{classic_data::Suit, CardFace, EventKind};
    let mut game = scene();
    let delayed = card(20, CardKind::Lebu);
    game.seat_mut(0).judges.push(delayed);
    game.classic.as_mut().unwrap().faces.push(CardFace {
        id: 119,
        suit: Suit::Club,
        rank: 5,
    });
    game.scheduler.start(crate::Program::event(EventKind::Turn {
        seat: 0,
        number: 1,
        round: 1,
    }));
    game.run(None);
    pass_counters(&mut game);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    assert!(game.seat(0).unwrap().judges.is_empty());
    assert!(game.discard.iter().any(|c| c.id == delayed.id));
}

#[test]
fn borrowed_weapon_attack_targets_designated_other_seat() {
    use crate::{Action, ChoiceKind, Command};
    let mut game = scene();
    game.seat_mut(1)
        .equipment
        .install_test(card(50, CardKind::Zhuge));
    game.seat_mut(1).hand.push(card(51, CardKind::Sha));
    start_trick(&mut game, card(20, CardKind::Jiedao), vec![1, 2]);
    pass_counters(&mut game);
    select(&mut game, 1, vec![51]);
    let view = game.view(Some(2)).unwrap();
    let choice = view.choice.unwrap();
    assert_eq!(choice.kind, ChoiceKind::Shan);
    game.submit(Command {
        version: view.version,
        seat: 2,
        action: Action::Choose {
            event: choice.event,
            card: None,
        },
    })
    .unwrap();
    assert_eq!(game.seat(2).unwrap().hp, 3);
    assert_eq!(game.seat(1).unwrap().equipment[0].id, 50);
    assert!(game.discard.iter().any(|c| c.id == 51));
}

#[test]
fn virtual_lebu_keeps_physical_card_and_blocks_duplicates() {
    let mut game = scene();
    game.classic.as_mut().unwrap().turn.as_mut().unwrap().seat = 2;
    let material = card(40, CardKind::Tao);
    game.seat_mut(0).hand.push(material);
    let program = game
        .virtual_trick_program(0, CardKind::Lebu, vec![material], vec![1])
        .unwrap();
    game.scheduler.start(program);
    game.run(None);
    assert_eq!(game.seat(1).unwrap().judges, vec![material]);
    assert_eq!(game.judgment_kind(material), CardKind::Lebu);
    let other = card(41, CardKind::Tao);
    game.seat_mut(0).hand.push(other);
    assert!(game
        .virtual_trick_program(0, CardKind::Lebu, vec![other], vec![1])
        .is_err());
    let public = game.view(None).unwrap();
    assert_eq!(public.seats[1].judges, vec![material]);
    assert_eq!(public.seats[1].judge_assignments[0].kind, CardKind::Lebu);
}

#[test]
fn virtual_duel_skips_wuxie_and_has_no_fabricated_card() {
    let mut game = scene();
    game.seat_mut(3).hand.push(card(42, CardKind::Wuxie));
    game.scheduler
        .start(game.virtual_duel_without_counter(0, 1));
    game.run(None);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    select(&mut game, 0, vec![]);
    assert_eq!(game.seat(1).unwrap().hp, 3);
    assert!(game.seat(3).unwrap().hand.iter().any(|c| c.id == 42));
    assert!(!game.discard.iter().any(|c| c.id == 0));
}

#[test]
fn lightning_material_is_available_to_jianxiong_until_damage_finishes() {
    use crate::classic_data::{roster, Suit};
    use crate::CardFace;
    let mut game = scene();
    game.seat_mut(0).hero = roster().iter().find(|g| g.id == "caocao");
    let lightning = card(20, CardKind::Shandian);
    game.seat_mut(0).judges.push(lightning);
    game.classic.as_mut().unwrap().faces.push(CardFace {
        id: 119,
        suit: Suit::Spade,
        rank: 5,
    });
    game.scheduler
        .start(crate::Program::event(crate::EventKind::Phase {
            seat: 0,
            phase: crate::Phase::Judgment,
        }));
    game.run(None);
    pass_counters(&mut game);
    assert_eq!(game.seat(0).unwrap().hp, 1);
    assert!(game.resolving.contains(&lightning));
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    let accept = choice
        .branches
        .iter()
        .find(|b| b.label.contains("奸雄") && b.id != 0)
        .unwrap()
        .id;
    select(&mut game, accept, vec![]);
    assert!(game.seat(0).unwrap().hand.contains(&lightning));
    assert!(!game.resolving.contains(&lightning));
    assert!(!game.discard.contains(&lightning));
}

#[test]
fn seed_21_wugu_has_equal_public_windows_with_or_without_a_hidden_counter() {
    let mut held = Game::classic(21, 0, "liubei", 4).unwrap();
    let mut empty = Game::classic(21, 0, "liubei", 4).unwrap();
    let hand = &mut held.seat_mut(1).hand;
    if hand.iter().all(|card| card.kind != CardKind::Wuxie) {
        hand[0].kind = CardKind::Wuxie;
    }
    for card in &mut empty.seat_mut(1).hand {
        if card.kind == CardKind::Wuxie {
            card.kind = CardKind::Sha;
        }
    }
    let opening = held.view(Some(0)).unwrap();
    let action = opening
        .actions
        .into_iter()
        .find(|action| matches!(action, Action::PlayCard { card: 70, .. }))
        .expect("seed 21 opens with the observed wugu play");
    let command = Command {
        version: opening.version,
        seat: 0,
        action,
    };
    held.submit(command.clone()).unwrap();
    empty.submit(command).unwrap();

    for actor in 0..4 {
        assert_eq!(held.view(None).unwrap(), empty.view(None).unwrap());
        assert_eq!(held.view(Some(0)).unwrap(), empty.view(Some(0)).unwrap());
        let public = held.view(None).unwrap();
        assert_eq!(public.waiting_for, Some(actor));
        assert!(public.counter_window);
        assert_eq!(public.resolving.len(), 5);
        assert!(public.choice.is_none());
        let left = held.view(Some(actor)).unwrap().choice.unwrap();
        let right = empty.view(Some(actor)).unwrap().choice.unwrap();
        assert_eq!(
            left.counter,
            Some(CounterContext {
                card_kind: CardKind::Wugu,
                source: Some(0),
                target: 0,
                negated: false,
            })
        );
        assert_eq!(left.counter, right.counter);
        assert_eq!(left.branches.len() != right.branches.len(), actor == 1);
        select(&mut held, 0, vec![]);
        select(&mut empty, 0, vec![]);
    }
    assert_eq!(held.view(None).unwrap(), empty.view(None).unwrap());
    assert!(!held.view(None).unwrap().counter_window);
    assert!(held
        .view(Some(0))
        .unwrap()
        .choice
        .unwrap()
        .counter
        .is_none());
}

#[test]
fn seed_1_bot_keeps_its_wuxie_when_its_wuzhong_is_already_effective() {
    let mut game = Game::classic(1, 0, "liubei", 4).unwrap();
    let use_wuzhong = game.bot_command().unwrap();
    assert_eq!(use_wuzhong.seat, 1);
    assert_eq!(
        use_wuzhong.action,
        Action::PlayCard {
            card: 83,
            targets: vec![1]
        }
    );
    game.submit(use_wuzhong).unwrap();
    let paused = game.view(Some(1)).unwrap();
    let before = paused.seats[1].hand_count;
    let choice = paused.choice.unwrap();
    assert_eq!(
        choice.counter,
        Some(CounterContext {
            card_kind: CardKind::Wuzhong,
            source: Some(1),
            target: 1,
            negated: false,
        })
    );
    assert_eq!(choice.branches[1].cards, [97]);
    let responds = game.bot_command().unwrap();
    assert_eq!(responds.seat, 1);
    assert_eq!(
        responds.action,
        Action::Select {
            event: choice.event,
            branch: 0,
            cards: vec![],
            targets: vec![]
        }
    );
    game.submit(responds).unwrap();
    pass_counters(&mut game);
    let finished = game.view(Some(1)).unwrap();
    assert_eq!(finished.seats[1].hand_count, before + 2);
    assert!(finished.seats[1]
        .hand
        .as_ref()
        .unwrap()
        .iter()
        .any(|card| card.id == 97));
    assert!(!finished.discard.iter().any(|card| card.id == 97));
}

#[test]
fn bot_considers_public_attitudes_and_the_current_counter_parity() {
    let mut game = scene();
    game.classic.as_mut().unwrap().human = 3;
    for (seat, id) in [(0, 40), (1, 41), (2, 42)] {
        game.seat_mut(seat).hand.push(card(id, CardKind::Wuxie));
    }
    start_trick(&mut game, card(20, CardKind::Wuzhong), vec![0]);

    for (seat, expected_branch, negated) in
        [(0, 0, false), (1, 0, false), (2, 1, false), (0, 1, true)]
    {
        pass_counters_until(&mut game, |choice| choice.seat == seat);
        let choice = game.view(Some(seat)).unwrap().choice.unwrap();
        assert_eq!(choice.counter.unwrap().negated, negated);
        let command = game.bot_command().unwrap();
        assert_eq!(command.seat, seat);
        assert!(
            matches!(command.action, Action::Select { branch, .. } if branch == expected_branch)
        );
        game.submit(command).unwrap();
    }
    pass_counters(&mut game);
    assert_eq!(game.seat(0).unwrap().hand.len(), 2);
    assert!(game
        .seat(1)
        .unwrap()
        .hand
        .contains(&card(41, CardKind::Wuxie)));
    assert!(game.discard.contains(&card(40, CardKind::Wuxie)));
    assert!(game.discard.contains(&card(42, CardKind::Wuxie)));
}
