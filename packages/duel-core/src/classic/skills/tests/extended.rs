use super::*;
use crate::classic::tests::{pass_counters, pass_counters_until};

fn play_card(game: &mut Game, seat: SeatId, card: CardId, targets: Vec<SeatId>) {
    command(game, seat, Action::PlayCard { card, targets });
}

fn start_turn(game: &mut Game, seat: SeatId) {
    start(
        game,
        Program::event(EventKind::Turn {
            seat,
            number: 1,
            round: 1,
        }),
    );
}

fn deal_damage(game: &mut Game, source: SeatId, target: SeatId) {
    start(
        game,
        Program::event(EventKind::Damage {
            source: Some(source),
            target,
            amount: 1,
        }),
    );
}

#[test]
fn paoxiao_keeps_a_second_sha_available_in_the_same_play_phase() {
    let mut game = assigned(&["zhangfei", "sunquan", "lvbu"], 0);
    game.seat_mut(0)
        .hand
        .extend([card(1, CardKind::Sha), card(2, CardKind::Sha)]);
    play_phase(&mut game, 0);
    for id in [1, 2] {
        play_card(&mut game, 0, id, vec![1]);
        pass(&mut game, 1);
    }
    assert_eq!(game.seat(1).unwrap().hp, 2);
}

#[test]
fn mashu_reaches_an_opposite_seat_and_red_tieji_bypasses_its_shan() {
    let mut game = assigned(&["machao", "sunquan", "zhouyu", "lvbu"], 0);
    let shan = card(2, CardKind::Shan);
    game.seat_mut(0).hand.push(card(1, CardKind::Sha));
    game.seat_mut(2).hand.push(shan);
    set_face(&mut game, 109, Suit::Heart, 9);
    play_phase(&mut game, 0);
    assert_eq!(game.distance(0, 2), Ok(1));
    play_card(&mut game, 0, 1, vec![2]);
    select(&mut game, 0, 1, vec![], vec![]);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(0));
    assert_eq!(game.seat(2).unwrap().hp, 3);
    assert_eq!(game.seat(2).unwrap().hand, vec![shan]);
}

#[test]
fn guose_moves_a_diamond_equipment_card_to_judgment_and_respects_qianxun() {
    let mut game = assigned(&["daqiao", "luxun", "lvbu"], 0);
    let cost = card(1, CardKind::Qilin);
    game.seat_mut(0).equipment.install_test(cost);
    set_face(&mut game, cost.id, Suit::Diamond, 9);
    play_phase(&mut game, 0);
    let view = game.view(Some(0)).unwrap();
    let conversion = view
        .virtual_plays
        .iter()
        .find(|play| play.kind == CardKind::Lebu)
        .unwrap();
    assert_eq!(conversion.branch.targets, vec![2]);
    command(
        &mut game,
        0,
        Action::PlayVirtual {
            kind: CardKind::Lebu,
            cards: vec![cost.id],
            targets: vec![2],
        },
    );
    assert!(game.seat(0).unwrap().equipment.is_empty());
    let target = &game.view(None).unwrap().seats[2];
    assert_eq!(target.judges, vec![cost]);
    assert_eq!(target.judge_assignments[0].kind, CardKind::Lebu);
}

#[test]
fn qixi_turns_a_black_equipped_card_into_guohe() {
    let mut game = assigned(&["ganning", "zhouyu", "lvbu"], 0);
    let cost = card(1, CardKind::Qilin);
    let removed = card(2, CardKind::Tao);
    game.seat_mut(0).equipment.install_test(cost);
    game.seat_mut(1).hand.push(removed);
    set_face(&mut game, cost.id, Suit::Spade, 9);
    play_phase(&mut game, 0);
    command(
        &mut game,
        0,
        Action::PlayVirtual {
            kind: CardKind::Guohe,
            cards: vec![cost.id],
            targets: vec![1],
        },
    );
    pass_counters(&mut game);
    select(&mut game, 0, 0, vec![], vec![]);
    assert!(game.seat(0).unwrap().equipment.is_empty());
    assert!(game.seat(1).unwrap().hand.is_empty());
    assert!(game.discard.contains(&cost) && game.discard.contains(&removed));
}

#[test]
fn qingguo_and_longdan_can_dodge_using_sha_as_physical_material() {
    for hero in ["zhenji", "zhaoyun"] {
        let mut game = assigned(&["zhangfei", hero, "lvbu"], 0);
        let dodge = card(2, CardKind::Sha);
        game.seat_mut(0).hand.push(card(1, CardKind::Sha));
        game.seat_mut(1).hand.push(dodge);
        set_face(&mut game, dodge.id, Suit::Spade, 9);
        play_phase(&mut game, 0);
        play_card(&mut game, 0, 1, vec![1]);
        supply(&mut game, 1, dodge.id);
        assert_eq!(game.seat(1).unwrap().hp, 4);
        assert!(game.discard.contains(&dodge));
    }
}

#[test]
fn kongcheng_blocks_both_sha_and_juedou_until_the_target_has_a_hand_card() {
    let mut game = assigned(&["zhangfei", "zhugeliang", "lvbu"], 0);
    game.seat_mut(0)
        .hand
        .extend([card(1, CardKind::Sha), card(2, CardKind::Juedou)]);
    play_phase(&mut game, 0);
    let targeting = [
        Action::PlayCard {
            card: 1,
            targets: vec![1],
        },
        Action::PlayCard {
            card: 2,
            targets: vec![1],
        },
    ];
    let view = game.view(Some(0)).unwrap();
    assert!(targeting
        .iter()
        .all(|action| !view.actions.contains(action)));
    game.seat_mut(1).hand.push(card(3, CardKind::Shan));
    let view = game.view(Some(0)).unwrap();
    assert!(targeting.iter().all(|action| view.actions.contains(action)));
}

#[test]
fn liuli_moves_the_incoming_sha_to_the_chosen_character() {
    let mut game = assigned(&["zhangfei", "daqiao", "zhouyu"], 0);
    let sha = card(1, CardKind::Sha);
    let cost = card(2, CardKind::Shan);
    game.seat_mut(0).hand.push(sha);
    game.seat_mut(1).hand.push(cost);
    play_phase(&mut game, 0);
    play_card(&mut game, 0, sha.id, vec![1]);
    let choice = game.view(Some(1)).unwrap().choice.unwrap();
    let discard = choice
        .branches
        .iter()
        .find(|choice| choice.cards.contains(&cost.id))
        .unwrap();
    select(&mut game, 1, discard.id, vec![cost.id], vec![2]);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(2));
    pass(&mut game, 2);
    assert_eq!(game.seat(1).unwrap().hp, 4);
    assert_eq!(game.seat(2).unwrap().hp, 3);
    assert!(game.discard.contains(&sha) && game.discard.contains(&cost));
}

#[test]
fn qicai_reaches_a_distant_trick_target_and_jizhi_draws_before_its_effect() {
    let mut game = assigned(&["huangyueying", "luxun", "zhouyu", "lvbu"], 0);
    let shunshou = card(1, CardKind::Shunshou);
    let gained = card(2, CardKind::Tao);
    game.seat_mut(0).hand.push(shunshou);
    game.seat_mut(1).hand.push(card(3, CardKind::Tao));
    game.seat_mut(2).hand.push(gained);
    play_phase(&mut game, 0);
    assert_eq!(game.distance(0, 2), Ok(2));
    assert!(!game
        .view(Some(0))
        .unwrap()
        .actions
        .contains(&Action::PlayCard {
            card: shunshou.id,
            targets: vec![1]
        }));
    play_card(&mut game, 0, shunshou.id, vec![2]);
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    assert!(choice.branches.iter().any(|branch| branch.label == "集智"));
    select(&mut game, 0, 1, vec![], vec![]);
    assert_eq!(game.seat(0).unwrap().hand, vec![card(109, CardKind::Sha)]);
    assert_eq!(game.seat(2).unwrap().hand, vec![gained]);
    pass_counters(&mut game);
    select(&mut game, 0, 0, vec![], vec![]);
    assert!(game.seat(0).unwrap().hand.contains(&gained));
}

#[test]
fn jizhi_can_draw_when_using_wuxie() {
    let mut game = assigned(&["lvbu", "huangyueying", "zhouyu"], 0);
    let wuxie = card(2, CardKind::Wuxie);
    game.seat_mut(0).hand.push(card(1, CardKind::Wuzhong));
    game.seat_mut(1).hand.push(wuxie);
    play_phase(&mut game, 0);
    play_card(&mut game, 0, 1, vec![0]);
    pass_counters_until(&mut game, |choice| choice.seat == 1);
    supply(&mut game, 1, wuxie.id);
    let choice = game.view(Some(1)).unwrap().choice.unwrap();
    assert!(choice.branches.iter().any(|branch| branch.label == "集智"));
    select(&mut game, 1, 1, vec![], vec![]);
    pass_counters(&mut game);
    assert_eq!(game.seat(1).unwrap().hand, vec![card(109, CardKind::Sha)]);
    assert!(game.seat(0).unwrap().hand.is_empty());
}

#[test]
fn tuxi_takes_cards_from_two_targets_in_place_of_the_draw() {
    let mut game = assigned(&["zhangliao", "zhouyu", "lvbu"], 0);
    let gained = [card(1, CardKind::Tao), card(2, CardKind::Shan)];
    game.seat_mut(1).hand.push(gained[0]);
    game.seat_mut(2).hand.push(gained[1]);
    start_turn(&mut game, 0);
    select(&mut game, 0, 1, vec![], vec![1, 2]);
    assert_eq!(game.seat(0).unwrap().hand, gained);
    assert!(game.seat(1).unwrap().hand.is_empty() && game.seat(2).unwrap().hand.is_empty());
    assert_eq!(game.view(None).unwrap().deck_count, 10);
}

#[test]
fn yingzi_draws_an_extra_card_and_biyue_draws_at_the_end_of_the_turn() {
    let mut game = assigned(&["zhouyu", "diaochan", "lvbu"], 0);
    start_turn(&mut game, 0);
    select(&mut game, 0, 1, vec![], vec![]);
    assert_eq!(game.seat(0).unwrap().hand.len(), 3);
    command(&mut game, 0, Action::EndPlay);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    command(&mut game, 1, Action::EndPlay);
    let choice = game.view(Some(1)).unwrap().choice.unwrap();
    assert!(choice.branches.iter().any(|branch| branch.label == "闭月"));
    select(&mut game, 1, 1, vec![], vec![]);
    assert_eq!(game.seat(1).unwrap().hand.len(), 3);
}

#[test]
fn yingzi_reports_one_activation_only_when_the_choice_is_accepted() {
    use crate::EventStatus;

    for (branch, amount) in [(0, 2), (1, 3)] {
        let mut game = Game::classic(0, 0, "zhouyu", 2).unwrap();
        let before = game.view(Some(0)).unwrap();
        assert!(before
            .choice
            .unwrap()
            .branches
            .iter()
            .any(|branch| branch.label == "英姿"));
        let activations = |events: &[crate::GameEvent]| {
            events
                .iter()
                .filter(|event| {
                    event.status == EventStatus::Started
                        && event.kind
                            == EventKind::Skill {
                                seat: 0,
                                skill: SkillId::Yingzi,
                            }
                })
                .count()
        };
        assert_eq!(activations(&before.events), 0);

        select(&mut game, 0, branch, vec![], vec![]);
        let after = game.view(Some(0)).unwrap();
        assert_eq!(activations(&after.events), branch as usize);
        assert!(after.events.iter().any(|event| event.kind
            == EventKind::Draw {
                seat: 0,
                amount,
                reason: DrawReason::Phase
            }));
    }
}

#[test]
fn luoshen_keeps_the_black_card_and_ends_after_a_red_judgment() {
    let mut game = assigned(&["zhenji", "zhangfei", "lvbu"], 0);
    set_face(&mut game, 109, Suit::Club, 9);
    set_face(&mut game, 108, Suit::Diamond, 9);
    start_turn(&mut game, 0);
    select(&mut game, 0, 1, vec![], vec![]);
    assert_eq!(game.seat(0).unwrap().hand, vec![card(109, CardKind::Sha)]);
    select(&mut game, 0, 1, vec![], vec![]);
    assert!(game.view(Some(0)).unwrap().choice.is_none());
    assert_eq!(game.seat(0).unwrap().hand.len(), 3);
    assert!(game.discard.contains(&card(108, CardKind::Sha)));
}

#[test]
fn keji_skips_discard_only_if_no_sha_was_played() {
    for played_sha in [false, true] {
        let mut game = assigned(&["lvmeng", "zhangfei", "lvbu"], 0);
        game.seat_mut(0).hp = 1;
        game.seat_mut(0).hand.push(card(1, CardKind::Sha));
        start_turn(&mut game, 0);
        if played_sha {
            play_card(&mut game, 0, 1, vec![1]);
            pass(&mut game, 1);
        }
        command(&mut game, 0, Action::EndPlay);
        let choice = game.view(Some(0)).unwrap().choice.unwrap();
        if played_sha {
            assert_eq!(choice.kind, ChoiceKind::Discard);
        } else {
            assert!(choice.branches.iter().any(|branch| branch.label == "克己"));
            select(&mut game, 0, 1, vec![], vec![]);
            assert_eq!(game.seat(0).unwrap().hand.len(), 3);
        }
    }
}

#[test]
fn fankui_takes_a_selected_equipment_card_from_the_damage_source() {
    let mut game = assigned(&["simayi", "zhouyu", "lvbu"], 0);
    let gained = card(1, CardKind::Qilin);
    game.seat_mut(2).equipment.install_test(gained);
    deal_damage(&mut game, 2, 0);
    select(&mut game, 0, 2, vec![gained.id], vec![]);
    assert!(game.seat(2).unwrap().equipment.is_empty());
    assert_eq!(game.seat(0).unwrap().hand, vec![gained]);
}

#[test]
fn ganglie_lets_the_damage_source_discard_two_cards_to_avoid_retaliation() {
    let mut game = assigned(&["xiahoudun", "zhouyu", "lvbu"], 0);
    let discarded = [card(1, CardKind::Tao), card(2, CardKind::Shan)];
    game.seat_mut(2).hand.extend(discarded);
    set_face(&mut game, 109, Suit::Diamond, 9);
    deal_damage(&mut game, 2, 0);
    select(&mut game, 0, 1, vec![], vec![]);
    select(&mut game, 2, 1, vec![1, 2], vec![]);
    assert_eq!(game.seat(2).unwrap().hp, 4);
    assert!(game.seat(2).unwrap().hand.is_empty());
    assert!(discarded.iter().all(|card| game.discard.contains(card)));
}

#[test]
fn zhiheng_accepts_equipment_and_hand_costs_once_each_play_phase() {
    let mut game = assigned(&["sunquan", "zhouyu", "lvbu"], 0);
    let costs = [card(1, CardKind::Qilin), card(2, CardKind::Shan)];
    game.seat_mut(0).equipment.install_test(costs[0]);
    game.seat_mut(0).hand.push(costs[1]);
    play_phase(&mut game, 0);
    skill(&mut game, 0, SkillId::Zhiheng, vec![1, 2], vec![]);
    assert_eq!(
        game.seat(0).unwrap().hand,
        vec![card(109, CardKind::Sha), card(108, CardKind::Sha)]
    );
    assert!(costs.iter().all(|card| game.discard.contains(card)));
    assert!(!game
        .view(Some(0))
        .unwrap()
        .skill_plays
        .iter()
        .any(|play| play.skill == SkillId::Zhiheng));
}

#[test]
fn jieyin_recovers_the_actor_and_a_wounded_male_target() {
    let mut game = assigned(&["sunshangxiang", "zhouyu", "daqiao"], 0);
    game.seat_mut(0)
        .hand
        .extend([card(1, CardKind::Sha), card(2, CardKind::Shan)]);
    for seat in 0..3 {
        game.seat_mut(seat).hp = 2;
    }
    play_phase(&mut game, 0);
    let plays = game.view(Some(0)).unwrap().skill_plays;
    let play = plays
        .iter()
        .find(|play| play.skill == SkillId::Jieyin)
        .unwrap();
    assert_eq!(play.branch.targets, vec![1]);
    skill(&mut game, 0, SkillId::Jieyin, vec![1, 2], vec![1]);
    assert_eq!(game.seat(0).unwrap().hp, 3);
    assert_eq!(game.seat(1).unwrap().hp, 3);
    assert_eq!(game.seat(2).unwrap().hp, 2);
    assert!(game.seat(0).unwrap().hand.is_empty());
}

#[test]
fn qingnang_recovers_a_wounded_target_for_a_discarded_hand_card() {
    let mut game = assigned(&["huatuo", "zhouyu", "lvbu"], 0);
    let cost = card(1, CardKind::Shan);
    game.seat_mut(0).hand.push(cost);
    game.seat_mut(1).hp = 2;
    play_phase(&mut game, 0);
    skill(&mut game, 0, SkillId::Qingnang, vec![cost.id], vec![1]);
    assert_eq!(game.seat(1).unwrap().hp, 3);
    assert!(game.discard.contains(&cost));
}
