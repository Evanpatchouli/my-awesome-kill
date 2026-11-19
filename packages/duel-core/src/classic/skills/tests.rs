use super::super::tests::{card, scene};
use super::{HeroMoment, SkillId};
use crate::classic_data::{roster, Role, Suit};
use crate::{
    Action, CardFace, CardId, CardKind, ChoiceKind, Command, DrawReason, EventKind, Game, Phase,
    Program, RuleError, SeatId,
};

mod card_enabled;
mod extended;

fn assigned(heroes: &[&str], active: SeatId) -> Game {
    let roles = [
        Role::Lord,
        Role::Rebel,
        Role::Loyalist,
        Role::Rebel,
        Role::Renegade,
    ];
    let mut game = scene(&roles[..heroes.len()], active);
    for (&hero, player) in heroes.iter().zip(&mut game.seats) {
        player.hero = roster().iter().find(|general| general.id == hero);
        assert!(player.hero.is_some(), "missing test general {hero}");
    }
    game
}

fn set_face(game: &mut Game, id: CardId, suit: Suit, rank: u8) {
    game.classic
        .as_mut()
        .unwrap()
        .faces
        .push(CardFace { id, suit, rank });
}

fn start(game: &mut Game, program: Program) {
    game.scheduler.start(program);
    game.run(None);
}

fn play_phase(game: &mut Game, seat: SeatId) {
    start(
        game,
        Program::event(EventKind::Phase {
            seat,
            phase: Phase::Play,
        }),
    );
}

fn command(game: &mut Game, seat: SeatId, action: Action) {
    game.submit(Command {
        seat,
        version: game.version,
        action,
    })
    .unwrap();
}

fn select(game: &mut Game, seat: SeatId, branch: u32, cards: Vec<CardId>, targets: Vec<SeatId>) {
    let event = game
        .view(Some(seat))
        .unwrap()
        .choice
        .expect("the seat has a choice")
        .event;
    command(
        game,
        seat,
        Action::Select {
            event,
            branch,
            cards,
            targets,
        },
    );
}

fn skill(game: &mut Game, seat: SeatId, skill: SkillId, cards: Vec<CardId>, targets: Vec<SeatId>) {
    command(
        game,
        seat,
        Action::UseSkill {
            skill,
            cards,
            targets,
        },
    );
}

fn pass(game: &mut Game, seat: SeatId) {
    let choice = game
        .view(Some(seat))
        .unwrap()
        .choice
        .expect("the seat has a choice");
    if choice.kind == ChoiceKind::Content {
        assert_eq!(choice.branches[0].id, 0);
        select(game, seat, 0, vec![], vec![]);
    } else {
        command(
            game,
            seat,
            Action::Choose {
                event: choice.event,
                card: None,
            },
        );
    }
}

#[test]
fn roster_skill_ids_match_the_wire_format() {
    let ids: std::collections::HashSet<_> = roster()
        .iter()
        .flat_map(|hero| &hero.skills)
        .map(|skill| &skill.id)
        .collect();
    assert_eq!(ids.len(), 40);
    for id in ids {
        let parsed: SkillId =
            serde_json::from_value(serde_json::Value::String(id.clone())).unwrap();
        assert_eq!(parsed.as_str(), id);
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            serde_json::Value::String(id.clone())
        );
    }
}

#[test]
fn equipped_red_card_can_pay_wusheng_with_the_same_sha_target_validation() {
    let mut game = assigned(&["guanyu", "zhugeliang", "zhouyu", "huanggai"], 0);
    let red = card(1, CardKind::Qilin);
    game.seat_mut(0).equipment.install_test(red);
    set_face(&mut game, red.id, Suit::Heart, 5);
    play_phase(&mut game, 0);

    let view = game.view(Some(0)).unwrap();
    let conversion = view
        .virtual_plays
        .iter()
        .find(|play| play.kind == CardKind::Sha)
        .unwrap();
    assert_eq!(conversion.branch.cards, vec![red.id]);
    assert!(!conversion.branch.targets.contains(&1));
    assert!(!conversion.branch.targets.contains(&2));
    assert_eq!(
        game.submit(Command {
            seat: 0,
            version: view.version,
            action: Action::PlayVirtual {
                kind: CardKind::Sha,
                cards: vec![red.id],
                targets: vec![1]
            }
        }),
        Err(RuleError::InvalidSelection)
    );
    assert_eq!(game.view(Some(0)).unwrap(), view);
    command(
        &mut game,
        0,
        Action::PlayVirtual {
            kind: CardKind::Sha,
            cards: vec![red.id],
            targets: vec![3],
        },
    );
    pass(&mut game, 3);

    assert!(game.seat(0).unwrap().equipment.is_empty());
    assert_eq!(game.seat(3).unwrap().hp, 3);
    assert!(game.discard.contains(&red));
}

#[test]
fn wusheng_cannot_spend_the_only_zhuge_for_a_second_sha() {
    let mut game = Game::classic(57, 0, "guanyu", 2).unwrap();
    command(
        &mut game,
        0,
        Action::PlayCard {
            card: 62,
            targets: vec![0],
        },
    );
    let can_convert = |view: &crate::GameView| {
        view.virtual_plays
            .iter()
            .any(|play| play.kind == CardKind::Sha && play.branch.cards.contains(&62))
    };
    assert!(can_convert(&game.view(Some(0)).unwrap()));
    command(
        &mut game,
        0,
        Action::PlayCard {
            card: 5,
            targets: vec![1],
        },
    );
    pass(&mut game, 1);

    let before = game.view(Some(0)).unwrap();
    assert_eq!(before.waiting_for, Some(0));
    assert_eq!(before.seats[0].equipment, vec![card(62, CardKind::Zhuge)]);
    let opponent_before = game.view(Some(1)).unwrap();
    assert!(!can_convert(&before));
    let forged = Command {
        seat: 0,
        version: before.version,
        action: Action::PlayVirtual {
            kind: CardKind::Sha,
            cards: vec![62],
            targets: vec![1],
        },
    };
    assert_eq!(game.submit(forged), Err(RuleError::InvalidSelection));
    assert_eq!(game.view(Some(0)).unwrap(), before);
    assert_eq!(game.view(Some(1)).unwrap(), opponent_before);
}

#[test]
fn wusheng_can_spend_zhuge_for_a_second_sha_when_paoxiao_remains() {
    use crate::classic_data::General;
    use std::sync::OnceLock;

    static COMBINED_SKILLS: OnceLock<General> = OnceLock::new();
    let hero = COMBINED_SKILLS.get_or_init(|| {
        let mut hero = roster()
            .iter()
            .find(|hero| hero.id == "guanyu")
            .unwrap()
            .clone();
        let zhangfei = roster().iter().find(|hero| hero.id == "zhangfei").unwrap();
        hero.skills.extend(zhangfei.skills.clone());
        hero
    });
    let mut game = assigned(&["guanyu", "lvbu"], 0);
    game.seat_mut(0).hero = Some(hero);
    let zhuge = card(1, CardKind::Zhuge);
    game.seat_mut(0).equipment.install_test(zhuge);
    game.seat_mut(0).hand.push(card(2, CardKind::Sha));
    set_face(&mut game, zhuge.id, Suit::Diamond, 1);
    play_phase(&mut game, 0);
    command(
        &mut game,
        0,
        Action::PlayCard {
            card: 2,
            targets: vec![1],
        },
    );
    pass(&mut game, 1);

    let view = game.view(Some(0)).unwrap();
    assert!(view
        .virtual_plays
        .iter()
        .any(|play| play.kind == CardKind::Sha && play.branch.cards.contains(&zhuge.id)));
    command(
        &mut game,
        0,
        Action::PlayVirtual {
            kind: CardKind::Sha,
            cards: vec![zhuge.id],
            targets: vec![1],
        },
    );
    pass(&mut game, 1);
    assert_eq!(game.seat(1).unwrap().hp, 2);
    assert!(game.seat(0).unwrap().equipment.is_empty());
    assert!(game.discard.contains(&zhuge));
}

#[test]
fn rende_recovers_only_when_the_second_card_is_given() {
    let mut game = assigned(&["liubei", "zhouyu", "lvbu"], 0);
    game.seat_mut(0).hand = vec![
        card(1, CardKind::Sha),
        card(2, CardKind::Shan),
        card(3, CardKind::Tao),
    ];
    game.seat_mut(0).hp = 2;
    play_phase(&mut game, 0);
    skill(&mut game, 0, SkillId::Rende, vec![1], vec![1]);
    assert_eq!(game.seat(0).unwrap().hp, 2);
    skill(&mut game, 0, SkillId::Rende, vec![2], vec![2]);
    assert_eq!(game.seat(0).unwrap().hp, 3);
    skill(&mut game, 0, SkillId::Rende, vec![3], vec![2]);
    assert_eq!(game.seat(0).unwrap().hp, 3);
    assert_eq!(
        game.seat(1)
            .unwrap()
            .hand
            .iter()
            .map(|card| card.id)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(
        game.seat(2)
            .unwrap()
            .hand
            .iter()
            .map(|card| card.id)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
}

#[test]
fn fanjian_compares_the_chosen_suit_and_keeps_the_obtained_hand_card_private() {
    let mut game = assigned(&["zhouyu", "zhangfei", "lvbu"], 0);
    let obtained = card(1, CardKind::Sha);
    game.seat_mut(0).hand.push(obtained);
    set_face(&mut game, obtained.id, Suit::Club, 7);
    play_phase(&mut game, 0);
    skill(&mut game, 0, SkillId::Fanjian, vec![], vec![1]);
    select(&mut game, 1, 2, vec![], vec![]);

    assert_eq!(game.seat(1).unwrap().hp, 3);
    assert_eq!(game.seat(1).unwrap().hand, vec![obtained]);
    assert!(game
        .view(Some(2))
        .unwrap()
        .card_faces
        .iter()
        .all(|face| face.id != obtained.id));
    assert!(game
        .view(Some(0))
        .unwrap()
        .skill_plays
        .iter()
        .all(|play| play.skill != SkillId::Fanjian));
}

#[test]
fn guanxing_hides_the_preview_and_keeps_the_selected_deck_order() {
    let mut game = assigned(&["zhugeliang", "lvbu", "zhouyu", "xuzhu"], 0);
    for id in 106..110 {
        set_face(&mut game, id, Suit::Spade, 2);
    }
    let preview = game.hero_triggers(HeroMoment::Preparation { seat: 0 });
    start(&mut game, preview);
    select(&mut game, 0, 1, vec![], vec![]);
    let private = game.view(Some(0)).unwrap();
    assert_eq!(
        private
            .private_choice_cards
            .iter()
            .map(|card| card.id)
            .collect::<Vec<_>>(),
        vec![109, 108, 107, 106]
    );
    assert_eq!(private.card_faces.len(), 4);
    let public = game.view(Some(1)).unwrap();
    assert!(
        public.private_choice_cards.is_empty()
            && public.card_faces.is_empty()
            && public.resolving.is_empty()
    );
    let event = private.choice.unwrap().event;
    command(
        &mut game,
        0,
        Action::MoveCards {
            event,
            moves: Some(vec![
                crate::MoveStep {
                    card: 106,
                    row: 1,
                    index: 0,
                },
                crate::MoveStep {
                    card: 108,
                    row: 1,
                    index: 1,
                },
                crate::MoveStep {
                    card: 107,
                    row: 0,
                    index: 0,
                },
            ]),
        },
    );

    // The next seat has already drawn the two final top cards.
    assert_eq!(
        game.seat(1)
            .unwrap()
            .hand
            .iter()
            .map(|card| card.id)
            .collect::<Vec<_>>(),
        vec![107, 109]
    );
    let bottom: Vec<_> = game
        .classic
        .as_ref()
        .unwrap()
        .deck
        .iter()
        .take(2)
        .map(|card| card.id)
        .collect();
    assert_eq!(bottom, vec![108, 106]);
    assert_eq!(
        game.view(Some(0)).unwrap().seats[1].known_hand,
        game.seat(1).unwrap().hand
    );
    assert!(game.view(None).unwrap().seats[1].known_hand.is_empty());
}

#[test]
fn guicai_replaces_a_judgment_before_tiandu_can_keep_the_card() {
    let mut game = assigned(&["guojia", "simayi", "zhangfei"], 0);
    let replacement = card(1, CardKind::Sha);
    game.seat_mut(1).hand.push(replacement);
    set_face(&mut game, replacement.id, Suit::Heart, 2);
    set_face(&mut game, 109, Suit::Spade, 7);
    let judge = game.judge_program(0, false);
    start(&mut game, judge);
    select(&mut game, 1, 1, vec![replacement.id], vec![]);
    assert!(game.discard.iter().any(|card| card.id == 109));
    select(&mut game, 0, 1, vec![], vec![]);

    assert!(game.seat(0).unwrap().hand.contains(&replacement));
    assert!(!game.discard.contains(&replacement));
    assert!(!game.resolving.contains(&replacement));
}

#[test]
fn damage_triggers_return_jianxiong_material_without_discarding_it() {
    let mut game = assigned(&["machao", "caocao", "zhouyu"], 0);
    let sha = card(1, CardKind::Sha);
    game.seat_mut(0).hand.push(sha);
    play_phase(&mut game, 0);
    command(
        &mut game,
        0,
        Action::PlayCard {
            card: sha.id,
            targets: vec![1],
        },
    );
    let tieji = game.view(Some(0)).unwrap().choice.unwrap();
    assert!(tieji.branches.iter().any(|branch| branch.label == "铁骑"));
    select(&mut game, 0, 0, vec![], vec![]);
    pass(&mut game, 1);
    select(&mut game, 1, 1, vec![], vec![]);

    assert!(game.seat(1).unwrap().hand.contains(&sha));
    assert!(!game.resolving.contains(&sha));
    assert!(!game.discard.contains(&sha));
    assert_eq!(game.seat(1).unwrap().hp, 3);
}

#[test]
fn liuli_discarding_the_weapon_cannot_move_sha_beyond_the_remaining_range() {
    let mut game = assigned(&["zhouyu", "daqiao", "zhangfei", "zhangliao"], 0);
    let held = card(1, CardKind::Shan);
    let weapon = card(2, CardKind::Qilin);
    game.seat_mut(1).hand.push(held);
    game.seat_mut(1).equipment.install_test(weapon);
    let redirect = game.hero_triggers(HeroMoment::ShaRedirect {
        seat: 0,
        target: 1,
        excluded: vec![],
    });
    start(&mut game, redirect);
    let choices = game.view(Some(1)).unwrap().choice.unwrap().branches;
    let for_card = |id| {
        choices
            .iter()
            .find(|branch| branch.cards.contains(&id))
            .unwrap()
    };
    assert_eq!(for_card(held.id).targets, vec![2, 3]);
    assert_eq!(for_card(weapon.id).targets, vec![2]);
}

#[test]
fn kurou_enters_dying_without_starting_damage() {
    let mut game = assigned(&["huanggai", "zhouyu", "zhangfei"], 0);
    game.seat_mut(0).hp = 1;
    play_phase(&mut game, 0);
    skill(&mut game, 0, SkillId::Kurou, vec![], vec![]);

    let view = game.view(None).unwrap();
    assert!(view.events.iter().any(|event| event.kind
        == EventKind::Dying {
            source: None,
            target: 0
        }));
    assert!(!view
        .events
        .iter()
        .any(|event| matches!(event.kind, EventKind::Damage { .. })));
    assert_eq!(view.seats[0].hp, 0);
    while let Some(seat) = game.view(None).unwrap().waiting_for {
        assert_eq!(
            game.view(Some(seat)).unwrap().choice.unwrap().kind,
            ChoiceKind::Save
        );
        pass(&mut game, seat);
    }
    assert!(!game.seat(0).unwrap().alive);
}

#[test]
fn damaging_card_only_enables_the_matching_luoyi_turn_bonus() {
    let mut game = assigned(&["xuzhu", "zhouyu", "zhangfei"], 0);
    game.seat_mut(0).hand.push(card(1, CardKind::Sha));
    start(
        &mut game,
        Program::event(EventKind::Turn {
            seat: 0,
            number: 1,
            round: 1,
        }),
    );
    select(&mut game, 0, 1, vec![], vec![]);
    assert!(game.events.iter().any(|event| event.kind
        == EventKind::Draw {
            seat: 0,
            amount: 1,
            reason: DrawReason::Phase
        }));
    assert!(game.has_skill(0, "luoyi2"));
    assert!(!game.has_skill(1, "luoyi2"));
    command(
        &mut game,
        0,
        Action::PlayCard {
            card: 1,
            targets: vec![1],
        },
    );
    pass(&mut game, 1);
    assert_eq!(game.seat(1).unwrap().hp, 2);
}

fn supply(game: &mut Game, seat: SeatId, card: CardId) {
    let choice = game.view(Some(seat)).unwrap().choice.unwrap();
    if choice.kind == ChoiceKind::Content {
        let branch = choice
            .branches
            .iter()
            .find(|choice| choice.cards.contains(&card))
            .unwrap();
        select(game, seat, branch.id, vec![card], vec![]);
    } else {
        command(
            game,
            seat,
            Action::Choose {
                event: choice.event,
                card: Some(card),
            },
        );
    }
}

#[test]
fn wushuang_requires_two_defenses_and_consumes_both_cards() {
    let mut game = assigned(&["lvbu", "zhouyu", "zhangfei"], 0);
    let shan = [card(2, CardKind::Shan), card(3, CardKind::Shan)];
    game.seat_mut(0).hand.push(card(1, CardKind::Sha));
    game.seat_mut(1).hand.extend(shan);
    play_phase(&mut game, 0);
    command(
        &mut game,
        0,
        Action::PlayCard {
            card: 1,
            targets: vec![1],
        },
    );
    supply(&mut game, 1, 2);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    supply(&mut game, 1, 3);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(0));
    assert_eq!(game.seat(1).unwrap().hp, 4);
    assert!(shan.iter().all(|card| game.discard.contains(card)));
}

#[test]
fn lianying_triggers_when_the_final_hand_card_moves_to_resolution() {
    let mut game = assigned(&["luxun", "zhangfei", "lvbu"], 0);
    game.seat_mut(0).hand.push(card(1, CardKind::Sha));
    play_phase(&mut game, 0);
    command(
        &mut game,
        0,
        Action::PlayCard {
            card: 1,
            targets: vec![1],
        },
    );
    assert_eq!(game.view(None).unwrap().waiting_for, Some(0));
    assert_eq!(game.seat(1).unwrap().hp, 4);
    assert!(game.seat(0).unwrap().hand.is_empty());
    select(&mut game, 0, 1, vec![], vec![]);
    pass(&mut game, 1);
    assert_eq!(game.seat(0).unwrap().hand, vec![card(109, CardKind::Sha)]);
    assert_eq!(game.seat(1).unwrap().hp, 3);
}

#[test]
fn xiaoji_triggers_once_for_a_replaced_equipment_card() {
    let mut game = assigned(&["sunshangxiang", "zhouyu", "lvbu"], 0);
    let old = card(1, CardKind::Qilin);
    let replacement = card(2, CardKind::Zhuge);
    game.seat_mut(0).equipment.install_test(old);
    game.seat_mut(0).hand.push(replacement);
    play_phase(&mut game, 0);
    command(
        &mut game,
        0,
        Action::PlayCard {
            card: replacement.id,
            targets: vec![0],
        },
    );
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    assert!(choice.branches.iter().any(|branch| branch.label == "枭姬"));
    select(&mut game, 0, 1, vec![], vec![]);
    assert!(game.view(Some(0)).unwrap().choice.is_none());
    assert_eq!(game.seat(0).unwrap().hand.len(), 2);
    assert_eq!(game.seat(0).unwrap().equipment, vec![replacement]);
    assert!(game.discard.contains(&old));
}

#[test]
fn failed_jijiang_becomes_available_after_playing_another_skill() {
    let mut game = assigned(&["liubei", "zhaoyun", "caocao"], 0);
    game.seat_mut(0).hand.push(card(1, CardKind::Tao));
    game.seat_mut(1).hand.push(card(2, CardKind::Sha));
    play_phase(&mut game, 0);
    skill(&mut game, 0, SkillId::Jijiang, vec![], vec![2]);
    select(&mut game, 1, 0, vec![], vec![]);
    let available = |game: &Game| {
        game.view(Some(0))
            .unwrap()
            .skill_plays
            .iter()
            .any(|play| play.skill == SkillId::Jijiang)
    };
    assert!(!available(&game));
    skill(&mut game, 0, SkillId::Rende, vec![1], vec![2]);
    assert!(available(&game));
}

#[test]
fn hujia_accepts_a_shan_from_another_living_wei_general() {
    let mut game = assigned(&["caocao", "zhangfei", "simayi"], 1);
    let shan = card(2, CardKind::Shan);
    game.seat_mut(1).hand.push(card(1, CardKind::Sha));
    game.seat_mut(2).hand.push(shan);
    play_phase(&mut game, 1);
    command(
        &mut game,
        1,
        Action::PlayCard {
            card: 1,
            targets: vec![0],
        },
    );
    let defense = game.view(Some(0)).unwrap().choice.unwrap();
    let request = defense
        .branches
        .iter()
        .find(|choice| choice.label.contains("护驾"))
        .unwrap();
    select(&mut game, 0, request.id, vec![], vec![]);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(2));
    supply(&mut game, 2, shan.id);
    assert_eq!(game.seat(0).unwrap().hp, 4);
    assert!(game.discard.contains(&shan));
}

#[test]
fn jijiang_uses_ally_material_as_sha_from_the_lord() {
    let mut game = assigned(&["liubei", "zhaoyun", "caocao"], 0);
    let shan = card(1, CardKind::Shan);
    game.seat_mut(1).hand.push(shan);
    play_phase(&mut game, 0);
    skill(&mut game, 0, SkillId::Jijiang, vec![], vec![2]);
    supply(&mut game, 1, shan.id);
    pass(&mut game, 2);
    assert_eq!(game.seat(2).unwrap().hp, 3);
    assert!(game.seat(1).unwrap().hand.is_empty());
    assert!(game.events.iter().any(|event| event.kind
        == EventKind::Damage {
            source: Some(0),
            target: 2,
            amount: 1
        }));
    let recover_cost = game.view(Some(2)).unwrap().choice.unwrap();
    assert!(recover_cost
        .branches
        .iter()
        .any(|branch| branch.label == "奸雄"));
    pass(&mut game, 2);
    assert!(game.discard.contains(&shan));
}

#[test]
fn lijian_ignores_wuxie_and_applies_wushuang_to_the_other_duelist() {
    let mut game = assigned(&["diaochan", "lvbu", "zhouyu", "xuzhu"], 0);
    let cost = card(1, CardKind::Tao);
    let counter = card(2, CardKind::Wuxie);
    let response = card(3, CardKind::Sha);
    game.seat_mut(0).hand.push(cost);
    game.seat_mut(2).hand.extend([counter, response]);
    play_phase(&mut game, 0);
    skill(&mut game, 0, SkillId::Lijian, vec![cost.id], vec![2]);
    select(&mut game, 0, 1, vec![], vec![1]);
    let resolving_virtuals = game.view(None).unwrap().resolving_virtuals;
    assert_eq!(resolving_virtuals.len(), 1);
    assert_eq!(resolving_virtuals[0].kind, CardKind::Juedou);
    assert!(resolving_virtuals[0].materials.is_empty());
    for seat in 0..4 {
        assert_eq!(
            game.view(Some(seat)).unwrap().resolving_virtuals,
            resolving_virtuals,
        );
    }
    assert!(game.events.iter().all(|event| !matches!(
        event.kind,
        EventKind::Respond {
            kind: CardKind::Wuxie,
            ..
        }
    )));
    supply(&mut game, 2, response.id);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(2));
    assert_eq!(game.view(None).unwrap().resolving_virtuals, resolving_virtuals);
    pass(&mut game, 2);
    assert!(game.view(None).unwrap().resolving_virtuals.is_empty());
    assert_eq!(game.seat(2).unwrap().hp, 3);
    assert!(game.seat(2).unwrap().hand.contains(&counter));
    assert!(game.discard.contains(&cost));
    assert!(game.discard.contains(&response));
}

#[test]
fn jijiu_can_save_another_player_with_a_red_equipped_card() {
    let mut game = assigned(&["zhangfei", "huatuo", "lvbu"], 0);
    let remedy = card(1, CardKind::Qilin);
    set_face(&mut game, remedy.id, Suit::Heart, 5);
    game.seat_mut(0).hp = 1;
    game.seat_mut(1).equipment.install_test(remedy);
    start(
        &mut game,
        Program::event(EventKind::Damage {
            source: Some(2),
            target: 0,
            amount: 1,
        }),
    );
    pass(&mut game, 0);
    supply(&mut game, 1, remedy.id);

    assert_eq!(game.seat(0).unwrap().hp, 1);
    assert!(game.seat(1).unwrap().equipment.is_empty());
    assert!(game.discard.contains(&remedy));
}

#[test]
fn jiuyuan_adds_one_to_wu_peach_recovery_for_the_lord() {
    let mut game = assigned(&["sunquan", "zhouyu", "lvbu"], 0);
    let peach = card(1, CardKind::Tao);
    game.seat_mut(0).hp = 1;
    game.seat_mut(1).hand.push(peach);
    start(
        &mut game,
        Program::event(EventKind::Damage {
            source: Some(2),
            target: 0,
            amount: 2,
        }),
    );
    pass(&mut game, 0);
    supply(&mut game, 1, peach.id);

    assert_eq!(game.seat(0).unwrap().hp, 1);
    assert!(game.events.iter().any(|event| event.kind
        == EventKind::Recover {
            seat: 1,
            target: 0,
            amount: 2
        }));
    assert!(game.discard.contains(&peach));
}

#[test]
fn yiji_distributes_deck_cards_without_exposing_them_to_other_players() {
    let mut game = assigned(&["guojia", "zhangfei", "lvbu"], 0);
    set_face(&mut game, 109, Suit::Club, 8);
    set_face(&mut game, 108, Suit::Heart, 5);
    start(
        &mut game,
        Program::event(EventKind::Damage {
            source: Some(1),
            target: 0,
            amount: 1,
        }),
    );
    select(&mut game, 0, 1, vec![], vec![]);
    assert_eq!(game.view(Some(0)).unwrap().private_choice_cards.len(), 2);
    assert!(game.view(Some(1)).unwrap().card_faces.is_empty());
    select(&mut game, 0, 1, vec![109], vec![2]);
    select(&mut game, 0, 1, vec![108], vec![0]);

    assert_eq!(game.seat(2).unwrap().hand, vec![card(109, CardKind::Sha)]);
    assert_eq!(game.seat(0).unwrap().hand, vec![card(108, CardKind::Sha)]);
    assert!(game.view(None).unwrap().resolving.is_empty());
}
