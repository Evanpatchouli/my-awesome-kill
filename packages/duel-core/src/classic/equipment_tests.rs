use super::{
    equipment,
    tests::{card, scene},
};
use crate::classic_data::{self, Role, Suit};
use crate::{
    Action, Card, CardFace, CardId, CardKind, ChoiceKind, Command, EventKind, EventStatus, Game,
    Phase, Program, RuleError, SeatId,
};

fn table() -> Game {
    scene(
        &[Role::Lord, Role::Loyalist, Role::Rebel, Role::Renegade],
        0,
    )
}

fn play_phase(game: &mut Game) {
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
}

fn play(game: &mut Game, id: CardId, targets: &[SeatId]) {
    let view = game.view(Some(0)).unwrap();
    game.submit(Command {
        version: view.version,
        seat: 0,
        action: Action::PlayCard {
            card: id,
            targets: targets.to_vec(),
        },
    })
    .unwrap();
}

fn select(game: &mut Game, seat: SeatId, branch: u32, cards: &[CardId]) {
    let view = game.view(Some(seat)).unwrap();
    let choice = view.choice.unwrap();
    assert_eq!(choice.kind, ChoiceKind::Content);
    game.submit(Command {
        version: view.version,
        seat,
        action: Action::Select {
            event: choice.event,
            branch,
            cards: cards.to_vec(),
            targets: vec![],
        },
    })
    .unwrap();
}

fn defend(game: &mut Game, seat: SeatId, card: Option<CardId>) {
    let view = game.view(Some(seat)).unwrap();
    let choice = view.choice.unwrap();
    assert_eq!(choice.kind, ChoiceKind::Shan);
    game.submit(Command {
        version: view.version,
        seat,
        action: Action::Choose {
            event: choice.event,
            card,
        },
    })
    .unwrap();
}

#[test]
fn all_seventeen_equipment_cards_use_the_normal_action_and_replace_slots() {
    let mut game = table();
    let equipment: Vec<_> = classic_data::standard_deck()
        .into_iter()
        .map(|face| Card {
            id: face.id,
            kind: serde_json::from_value(serde_json::Value::String(face.kind)).unwrap(),
        })
        .filter(|card| equipment::is_equipment(card.kind))
        .collect();
    assert_eq!(equipment.len(), 17);
    game.seat_mut(0).hand = equipment.clone();
    play_phase(&mut game);
    for card in &equipment {
        play(&mut game, card.id, &[0]);
        assert!(game.view(None).unwrap().seats[0].equipment.contains(card));
    }
    let view = game.view(Some(0)).unwrap();
    assert_eq!(view.seats[0].equipment.len(), 4);
    assert!(view.seats[0].hand.as_ref().unwrap().is_empty());
    assert_eq!(view.discard.len(), 13);
    assert!(view.resolving.is_empty());
}

#[test]
fn horses_adjust_distance_and_zhuge_refreshes_current_use_limit() {
    let mut game = table();
    game.seat_mut(0).equipment = vec![card(5, CardKind::Chitu)];
    assert_eq!(game.distance(0, 2), Ok(1));
    game.seat_mut(2).equipment = vec![card(6, CardKind::Dilu)];
    assert_eq!(game.distance(0, 2), Ok(2));
    assert_eq!(game.distance(2, 0), Ok(2));
    game.seat_mut(0).hand = vec![
        card(1, CardKind::Sha),
        card(2, CardKind::Sha),
        card(3, CardKind::Zhuge),
    ];
    play_phase(&mut game);
    play(&mut game, 1, &[1]);
    defend(&mut game, 1, None);
    let restricted = game.view(Some(0)).unwrap();
    let second = Action::PlayCard {
        card: 2,
        targets: vec![1],
    };
    assert!(!restricted.actions.contains(&second));
    play(&mut game, 3, &[0]);
    assert!(game.view(Some(0)).unwrap().actions.contains(&second));
    play(&mut game, 2, &[1]);
    defend(&mut game, 1, None);
    assert_eq!(game.view(None).unwrap().seats[1].hp, 2);
}

#[test]
fn bagua_red_is_virtual_shan_black_falls_back_and_qinggang_bypasses_it() {
    for (suit, qinggang) in [
        (Suit::Heart, false),
        (Suit::Club, false),
        (Suit::Heart, true),
    ] {
        let mut game = table();
        let sha = card(1, CardKind::Sha);
        let hand_shan = card(2, CardKind::Shan);
        let judged = card(7, CardKind::Tao);
        game.seat_mut(0).hand = vec![sha];
        game.seat_mut(1).hand = vec![hand_shan];
        game.seat_mut(1).equipment = vec![card(8, CardKind::Bagua)];
        game.classic.as_mut().unwrap().deck.push(judged);
        game.classic.as_mut().unwrap().faces.push(CardFace {
            id: judged.id,
            suit,
            rank: 8,
        });
        if qinggang {
            game.seat_mut(0).equipment = vec![card(9, CardKind::Qinggang)];
        }
        play_phase(&mut game);
        play(&mut game, sha.id, &[1]);
        if !qinggang {
            select(&mut game, 1, 1, &[]);
        }
        if qinggang {
            defend(&mut game, 1, None);
        }
        if suit == Suit::Club {
            assert_eq!(game.view(None).unwrap().discard, [judged]);
            defend(&mut game, 1, Some(hand_shan.id));
        }
        let done = game.view(Some(1)).unwrap();
        assert_eq!(done.seats[1].hp, if qinggang { 3 } else { 4 });
        assert_eq!(done.discard.contains(&judged), !qinggang);
        assert_eq!(done.discard.contains(&hand_shan), suit == Suit::Club);
        assert!(done.discard.contains(&sha));
    }
}

#[test]
fn zhangba_validates_two_distinct_costs_and_fangtian_orders_three_targets() {
    let mut game = table();
    let cost = [card(1, CardKind::Tao), card(2, CardKind::Shan)];
    game.seat_mut(0).hand = cost.to_vec();
    game.seat_mut(0).equipment = vec![card(91, CardKind::Zhangba)];
    play_phase(&mut game);
    let opening = game.view(Some(0)).unwrap();
    assert_eq!(opening.virtual_plays[0].branch.min_cards, 2);
    let virtual_play = |cards| Command {
        version: opening.version,
        seat: 0,
        action: Action::PlayVirtual {
            kind: CardKind::Sha,
            cards,
            targets: vec![2],
        },
    };
    assert_eq!(
        game.submit(virtual_play(vec![1, 1])),
        Err(RuleError::InvalidSelection)
    );
    assert_eq!(game.view(Some(0)).unwrap(), opening);
    game.submit(virtual_play(vec![1, 2])).unwrap();
    assert_eq!(game.view(None).unwrap().resolving, cost);
    defend(&mut game, 2, None);
    assert_eq!(game.view(None).unwrap().discard, cost);

    let mut game = table();
    game.seat_mut(0).hand = vec![card(3, CardKind::Sha)];
    game.seat_mut(0).equipment = vec![card(92, CardKind::Fangtian)];
    play_phase(&mut game);
    play(&mut game, 3, &[3, 1, 2]);
    for seat in [1, 2, 3] {
        assert_eq!(game.view(None).unwrap().waiting_for, Some(seat));
        defend(&mut game, seat, None);
    }
    assert_eq!(
        game.view(None)
            .unwrap()
            .seats
            .iter()
            .map(|seat| seat.hp)
            .collect::<Vec<_>>(),
        [4, 3, 3, 3]
    );
}

#[test]
fn qinglong_shares_attack_resolution_and_axe_costs_force_the_original_hit() {
    for weapon in [CardKind::Qinglong, CardKind::Guanshi] {
        let mut game = table();
        let shan = card(8, CardKind::Shan);
        let weapon_card = card(9, weapon);
        game.seat_mut(0).equipment = vec![weapon_card, card(4, CardKind::Chitu)];
        game.seat_mut(0).hand = vec![card(1, CardKind::Sha), card(2, CardKind::Sha)];
        game.seat_mut(1).hand = vec![shan];
        play_phase(&mut game);
        play(&mut game, 1, &[1]);
        defend(&mut game, 1, Some(shan.id));
        if weapon == CardKind::Qinglong {
            select(&mut game, 0, 1, &[2]);
            defend(&mut game, 1, None);
        } else {
            let view = game.view(Some(0)).unwrap();
            assert!(!view.choice.unwrap().branches[1]
                .cards
                .contains(&weapon_card.id));
            select(&mut game, 0, 1, &[2, 4]);
        }
        let settled = game.view(None).unwrap();
        assert_eq!(settled.seats[1].hp, 3);
        assert!(settled.seats[0].equipment.contains(&weapon_card));
        assert!(settled.discard.contains(&card(2, CardKind::Sha)));
    }
}

#[test]
fn qilin_chooses_a_mount_between_damage_and_health_change() {
    let mut game = table();
    let horse = card(5, CardKind::Dilu);
    game.seat_mut(0).equipment = vec![card(8, CardKind::Qilin)];
    game.seat_mut(0).hand = vec![card(1, CardKind::Sha)];
    game.seat_mut(1).equipment = vec![horse];
    play_phase(&mut game);
    play(&mut game, 1, &[1]);
    defend(&mut game, 1, None);

    let waiting = game.view(None).unwrap();
    let damage = waiting
        .events
        .iter()
        .find(|event| {
            event.status == EventStatus::Started && matches!(event.kind, EventKind::Damage { .. })
        })
        .unwrap();
    assert_eq!(waiting.seats[1].hp, 4);
    assert!(waiting.seats[1].equipment.contains(&horse));
    assert!(!waiting
        .events
        .iter()
        .any(|event| matches!(event.kind, EventKind::ChangeHp { .. })));

    select(&mut game, 0, 1, &[horse.id]);
    let finished = game.view(None).unwrap();
    assert_eq!(finished.seats[1].hp, 3);
    assert!(!finished.seats[1].equipment.contains(&horse));
    let change = finished
        .events
        .iter()
        .position(|event| {
            event.parent == Some(damage.id)
                && event.kind
                    == EventKind::ChangeHp {
                        seat: 1,
                        before: 4,
                        after: 3,
                    }
        })
        .unwrap();
    let discard = finished
        .events
        .iter()
        .position(|event| event.kind == EventKind::Discard { card: horse })
        .unwrap();
    assert!(discard < change);
}

#[test]
fn cixiong_runs_before_the_defender_is_asked_for_shan() {
    let mut game = table();
    let find = |id| {
        classic_data::roster()
            .iter()
            .find(|hero| hero.id == id)
            .unwrap()
    };
    game.seat_mut(0).hero = Some(find("liubei"));
    game.seat_mut(1).hero = Some(find("diaochan"));
    game.seat_mut(0).equipment = vec![card(8, CardKind::Cixiong)];
    game.seat_mut(0).hand = vec![card(1, CardKind::Sha)];
    game.seat_mut(1).hand = vec![card(2, CardKind::Tao), card(3, CardKind::Shan)];
    play_phase(&mut game);
    play(&mut game, 1, &[1]);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(0));
    select(&mut game, 0, 1, &[]);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    select(&mut game, 1, 1, &[2]);
    assert_eq!(game.view(None).unwrap().discard, [card(2, CardKind::Tao)]);
    defend(&mut game, 1, Some(3));
    assert_eq!(game.view(None).unwrap().seats[1].hp, 4);
}
