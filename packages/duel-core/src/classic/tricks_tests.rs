use crate::{Card, CardKind, Game};

fn card(id: u32, kind: CardKind) -> Card {
    Card { id, kind }
}

#[test]
fn target_eligibility_obeys_distance_empty_zones_and_delayed_duplicates() {
    let mut game = Game::classic(3, 0, "zhangfei", 4).unwrap();
    for seat in &mut game.seats {
        seat.hand.clear();
        seat.equipment.clear();
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
        seat.equipment.clear();
    }
    game.seat_mut(1).equipment.push(card(1000, CardKind::Zhuge));
    let actor = game.seat(0).unwrap();
    let targets = game
        .trick_uses(actor, card(1001, CardKind::Jiedao))
        .unwrap()
        .into_iter()
        .map(|use_| use_.targets)
        .collect::<Vec<_>>();
    assert_eq!(targets, vec![vec![1, 2], vec![1, 0]]);
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
        skip_play: false,
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
    for expected in 0..4 {
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
    assert_eq!(game.view(None).unwrap().waiting_for, Some(2));
    select(&mut game, 1, vec![42]);
    assert_eq!(game.seat(1).unwrap().hp, 4, "first target was negated");
    assert_eq!(game.view(None).unwrap().waiting_for, Some(2));
    select(&mut game, 0, vec![]);
    assert_eq!(game.seat(2).unwrap().hp, 3);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(3));
    select(&mut game, 0, vec![]);
    assert_eq!(game.seat(3).unwrap().hp, 3);
}

#[test]
fn successive_wuxie_restore_effect_for_one_target() {
    let mut game = scene();
    game.seat_mut(1).hand.push(card(41, CardKind::Wuxie));
    game.seat_mut(2).hand.push(card(42, CardKind::Wuxie));
    start_trick(&mut game, card(20, CardKind::Nanman), vec![1, 2, 3]);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    select(&mut game, 1, vec![41]);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(2));
    select(&mut game, 1, vec![42]);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    select(&mut game, 0, vec![]);
    assert_eq!(game.seat(1).unwrap().hp, 3);
    select(&mut game, 0, vec![]);
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
    let flow = game.trick_judgment(0, lightning);
    game.scheduler.start(flow);
    game.run(None);
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
    game.scheduler.start(game.trick_judgment(0, lightning));
    game.run(None);
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
    game.seat_mut(1).equipment.push(weapon);
    start_trick(&mut game, card(20, CardKind::Jiedao), vec![1, 2]);
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
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    assert!(game.seat(0).unwrap().judges.is_empty());
    assert!(game.discard.iter().any(|c| c.id == delayed.id));
}

#[test]
fn borrowed_weapon_attack_targets_designated_other_seat() {
    use crate::{Action, ChoiceKind, Command};
    let mut game = scene();
    game.seat_mut(1).equipment.push(card(50, CardKind::Zhuge));
    game.seat_mut(1).hand.push(card(51, CardKind::Sha));
    start_trick(&mut game, card(20, CardKind::Jiedao), vec![1, 2]);
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
    game.scheduler.start(game.trick_judgment(0, lightning));
    game.run(None);
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
