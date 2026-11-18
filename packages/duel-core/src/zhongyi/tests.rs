use super::*;
use crate::classic_data::Suit;
use crate::{
    Action, Card, CardKind, ChoiceKind, Command, EventId, Game, TriggerContext, TriggerEvent,
    TriggerSelection,
};
use serde_json::{json, Value};
use std::sync::Mutex;

fn submit(game: &mut Game, seat: SeatId, action: Action) {
    game.submit(Command {
        version: game.version,
        seat,
        action,
    })
    .unwrap();
}
fn probe(
    timing: &str,
    effect: impl Fn(&mut TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) -> TriggerRegistration {
    TriggerRegistration {
        owner: 0,
        skill: format!("fixture_{timing}"),
        timing: timing.into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(effect),
    }
}
fn name(game: &Game, event: Option<EventId>) -> &'static str {
    match event.and_then(|id| game.event_kind(id)) {
        Some(EventKind::ActivateSkill { .. }) => "useSkill",
        Some(EventKind::ActiveSkillEffect { .. } | EventKind::RegisteredTrigger { .. }) => {
            "zhongyi"
        }
        Some(EventKind::AddToExpansion { .. }) => "addToExpansion",
        Some(EventKind::LoseToDiscardpile { .. }) => "loseToDiscardpile",
        Some(EventKind::Lose { .. }) => "lose",
        Some(EventKind::Damage { .. }) => "damage",
        Some(EventKind::ShaEffect { .. }) => "sha",
        Some(EventKind::Turn { .. }) => "phase",
        Some(EventKind::PhaseLoop) => "phaseLoop",
        Some(EventKind::Phase { .. }) | None => "reference_fixture_use",
        other => panic!("unexpected frame {other:?}"),
    }
}
fn zone(game: &Game, card: Card) -> &'static str {
    if game.seat(0).unwrap().hand.contains(&card) {
        "h"
    } else if game.transferring.contains(&card) {
        "s"
    } else if game.seat(0).unwrap().expansions.contains(&card) {
        "x"
    } else if game.discard.contains(&card) {
        "d"
    } else {
        panic!("lost material")
    }
}
fn row(game: &Game, event: &TriggerEvent, point: &str, card: Card) -> Option<Value> {
    let timing = event.timing.as_str();
    if ![
        "useSkillBegin",
        "useSkillAfter",
        "addToExpansionBefore",
        "addToExpansionAfter",
        "loseAfter",
        "loseToDiscardpileBegin",
        "loseToDiscardpileAfter",
        "damageBegin1",
        "damageBegin2",
        "damageEnd",
        "roundStart",
    ]
    .contains(&timing)
    {
        return None;
    }
    if timing == "loseAfter" && !event.cards.contains(&card) {
        return None;
    }
    let turn = game.classic.as_ref().unwrap().turn.unwrap();
    if timing == "loseAfter" {
        assert_eq!(usize::from(event.amount), event.cards.len());
    }
    let parent = name(game, event.parent);
    let operation = match timing {
        "damageBegin1" | "damageBegin2" | "damageEnd" => match (event.source, event.player, parent)
        {
            (Some(1), Some(2), "sha") => "attack-opponent",
            (Some(2), Some(1), "sha") => "attack-friend",
            (Some(2), Some(1), _) => "attack-friend-other-damage",
            (Some(0), Some(7), "sha") => "attack-expired",
            _ => return None,
        },
        "roundStart" | "loseToDiscardpileBegin" | "loseToDiscardpileAfter" => "between",
        "loseAfter" if turn.round == 2 => "between",
        _ => "activate",
    };
    let player = match event.player {
        Some(0) => Some("owner"),
        Some(1) if operation == "attack-opponent" => Some("opponent"),
        Some(2) => Some("friend"),
        Some(1 | 7) => Some("opponent"),
        _ => None,
    };
    let source = match event.source {
        Some(0) => Some("owner"),
        Some(1) => Some("opponent"),
        Some(2) => Some("friend"),
        _ => None,
    };
    let subject = name(game, Some(event.subject));
    let parent = match (operation, parent) {
        ("between", "reference_fixture_use") => "phaseLoop",
        ("attack-friend-other-damage", _) => "reference_fixture_use",
        (_, p) => p,
    };
    Some(json!([
        turn.round,
        operation,
        point,
        timing,
        subject,
        parent,
        player,
        source,
        event.subject_number(game),
        zone(game, card),
        game.expansion_tags(card.id).iter().any(|s| s == "zhongyi2"),
        game.has_skill(0, "zhongyi2"),
        game.limited_skill_used(0, "zhongyi")
    ]))
}
fn snapshot(game: &Game, card: Card, expected: &Value) {
    let z = zone(game, card);
    assert_eq!(z, expected["zone"]);
    assert_eq!(
        game.seat(0).unwrap().hand.contains(&card),
        expected["inHand"]
    );
    assert_eq!(
        game.seat(0).unwrap().expansions.contains(&card),
        expected["inExpansion"]
    );
    assert_eq!(!game.expansion_tags(card.id).is_empty(), expected["tagged"]);
    assert_eq!(
        game.expansions(0, "zhongyi2").len(),
        expected["markCount"].as_u64().unwrap() as usize
    );
    assert_eq!(game.discard.contains(&card), expected["inDiscard"]);
    assert_eq!(game.has_skill(0, "zhongyi2"), expected["buff"]);
    assert_eq!(game.limited_skill_used(0, "zhongyi"), expected["spent"]);
}
#[test]
fn natural_eight_seat_zhongyi_matches_full_reference_trace() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/zhongyi-expansion.json"
    ))
    .unwrap();
    let mut game = Game::classic(5, 0, "guanyu", 8).unwrap();
    assert!(!game.is_friend_of(0, 2));
    for seat in 0..8 {
        game.set_team(
            seat,
            u32::from(golden["sides"]["teams"][seat as usize].as_bool().unwrap()),
        )
        .unwrap();
        let p = game.seat_mut(seat);
        p.hp = 8;
        p.max_hp = 8;
        p.hand.clear();
        p.hero = None;
    }
    let material = game
        .create_content_card(CardKind::Tao, Suit::Heart, 3, None)
        .unwrap();
    let slash1 = game
        .create_content_card(CardKind::Sha, Suit::Spade, 4, None)
        .unwrap();
    let slash2 = game
        .create_content_card(CardKind::Sha, Suit::Spade, 5, None)
        .unwrap();
    let slash3 = game
        .create_content_card(CardKind::Sha, Suit::Spade, 6, None)
        .unwrap();
    game.seat_mut(0).hand.extend([material, slash3]);
    game.seat_mut(1).hand.push(slash1);
    game.seat_mut(2).hand.push(slash2);
    let active = game.register_active_skill(registration(0)).unwrap();
    let rows = Arc::new(Mutex::new(Vec::new()));
    let request = rows.clone();
    game.timing_observer = Some(Arc::new(move |game, e| {
        if let Some(value) = row(game, e, "request", material) {
            request.lock().unwrap().push(value);
        }
    }));
    for timing in [
        "useSkillBegin",
        "useSkillAfter",
        "addToExpansionBefore",
        "addToExpansionAfter",
        "loseAfter",
        "loseToDiscardpileBegin",
        "loseToDiscardpileAfter",
        "damageBegin1",
        "damageBegin2",
        "damageEnd",
        "roundStart",
    ] {
        let out = rows.clone();
        game.register_trigger(probe(timing, move |ctx, _, _| {
            if let Some(value) = row(ctx.game(), ctx.event(), "effect", material) {
                out.lock().unwrap().push(value);
            }
            TriggerStep::Done
        }))
        .unwrap();
    }
    let mut other_damage = probe("useCardAfter", |ctx, _, _| {
        ctx.damage(Some(2), 1, 1, None, None, &[]).unwrap();
        TriggerStep::Done
    });
    other_damage.filter =
        Arc::new(move |_, event, _| event.player == Some(2) && event.cards.contains(&slash2));
    game.register_trigger(other_damage).unwrap();
    snapshot(&game, material, &golden["snapshots"]["before"]);
    submit(
        &mut game,
        0,
        Action::ActivateSkill {
            registration: active,
            cards: vec![material.id],
            targets: vec![],
        },
    );
    snapshot(&game, material, &golden["snapshots"]["activated"]);
    assert!(game.active_plays(0).is_empty());
    let mut attacked = [false; 3];
    let mut prior_hp = [0; 3];
    for _ in 0..200 {
        let view = game.view(None).unwrap();
        let turn = view.turn.unwrap();
        let seat = view.waiting_for.expect("a decision during natural turns");
        let private = game.view(Some(seat)).unwrap();
        if let Some(choice) = private.choice {
            let action = match choice.kind {
                ChoiceKind::Discard => Action::Discard {
                    event: choice.event,
                    cards: choice.cards[..choice.min].to_vec(),
                },
                ChoiceKind::Shan | ChoiceKind::Save => Action::Choose {
                    event: choice.event,
                    card: None,
                },
                ChoiceKind::Content => {
                    let b = &choice.branches[0];
                    Action::Select {
                        event: choice.event,
                        branch: b.id,
                        cards: b.cards[..b.min_cards].to_vec(),
                        targets: b.targets[..b.min_targets].to_vec(),
                    }
                }
            };
            submit(&mut game, seat, action);
            continue;
        }
        let play = match (turn.round, turn.seat) {
            (1, 1) if !attacked[0] => Some((0, slash1, 2)),
            (1, 2) if !attacked[1] => Some((1, slash2, 1)),
            (2, 0) if !attacked[2] => Some((2, slash3, 7)),
            _ => None,
        };
        if let Some((i, card, target)) = play {
            if i == 2 {
                snapshot(&game, material, &golden["snapshots"]["nextRound"]);
            }
            prior_hp[i] = game.seat(target).unwrap().hp;
            assert!(private.actions.contains(&Action::PlayCard {
                card: card.id,
                targets: vec![target]
            }));
            attacked[i] = true;
            submit(
                &mut game,
                seat,
                Action::PlayCard {
                    card: card.id,
                    targets: vec![target],
                },
            );
            continue;
        }
        if attacked[2] && turn.round == 2 && turn.seat == 0 {
            break;
        }
        submit(&mut game, seat, Action::EndPlay);
    }
    assert_eq!(attacked, [true; 3]);
    assert_eq!(prior_hp, [8, 8, 8]);
    assert_eq!(
        [
            game.seat(2).unwrap().hp,
            game.seat(1).unwrap().hp,
            game.seat(7).unwrap().hp
        ],
        [7, 5, 7]
    );
    let hits = game.damage_history();
    assert_eq!(
        hits.iter().map(|h| h.amount).collect::<Vec<_>>(),
        golden["attacks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["amount"].as_u64().unwrap() as u16)
            .collect::<Vec<_>>()
    );
    snapshot(&game, material, &golden["snapshots"]["final"]);
    let rows = rows.lock().unwrap();
    let expected = golden["events"].as_array().unwrap();
    assert_eq!(rows.len(), expected.len());
    for (index, (actual, reference)) in rows.iter().zip(expected).enumerate() {
        assert_eq!(actual, reference, "reference row {index}");
    }
}

fn finish_decisions(game: &mut Game) {
    for _ in 0..100 {
        let view = game.view(None).unwrap();
        let Some(seat) = view.waiting_for else { break };
        let Some(choice) = game.view(Some(seat)).unwrap().choice else {
            break;
        };
        let action = match choice.kind {
            ChoiceKind::Discard => Action::Discard {
                event: choice.event,
                cards: choice.cards[..choice.min].to_vec(),
            },
            ChoiceKind::Shan | ChoiceKind::Save => Action::Choose {
                event: choice.event,
                card: None,
            },
            ChoiceKind::Content => {
                let b = &choice.branches[0];
                Action::Select {
                    event: choice.event,
                    branch: b.id,
                    cards: b.cards[..b.min_cards].to_vec(),
                    targets: b.targets[..b.min_targets].to_vec(),
                }
            }
        };
        submit(game, seat, action);
    }
}
fn boundary_table() -> Game {
    let mut game = Game::classic(5, 0, "guanyu", 8).unwrap();
    for seat in 0..8 {
        game.set_team(seat, 0).unwrap();
        let player = game.seat_mut(seat);
        player.hero = None;
        player.hand.clear();
        player.hp = 8;
        player.max_hp = 8;
    }
    game
}
#[test]
fn cancelled_expansion_keeps_buff_and_matches_boundary_privacy() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/zhongyi-boundaries.json"
    ))
    .unwrap();
    for (sample, timing) in [
        ("cancel-expansion", "addToExpansionBefore"),
        ("cancel-lose", "loseBefore"),
    ] {
        let expected = golden["trials"]
            .as_array()
            .unwrap()
            .iter()
            .find(|trial| trial["name"] == sample)
            .unwrap();
        let mut game = boundary_table();
        let material = game
            .create_content_card(CardKind::Shan, Suit::Spade, 2, None)
            .unwrap();
        let slash = game
            .create_content_card(CardKind::Sha, Suit::Club, 3, None)
            .unwrap();
        game.seat_mut(0).hand.extend([material, slash]);
        let active = game.register_active_skill(registration(0)).unwrap();
        let cancel = probe(timing, |_, _, _| TriggerStep::CancelSubject);
        game.register_trigger(cancel).unwrap();
        submit(
            &mut game,
            0,
            Action::ActivateSkill {
                registration: active,
                cards: vec![material.id],
                targets: vec![],
            },
        );
        let after = &expected["activated"];
        assert_eq!(
            game.seat(0).unwrap().hand.contains(&material),
            after["zone"] == format!("{}:h", expected["owner"].as_str().unwrap())
        );
        assert_eq!(
            !game.expansion_tags(material.id).is_empty(),
            after["tagged"]
        );
        assert_eq!(!game.expansions(0, "zhongyi2").is_empty(), after["mark"]);
        assert_eq!(
            game.known_to_all.contains(&material.id),
            after["knownToSpectator"]
        );
        assert_eq!(game.has_skill(0, "zhongyi2"), after["buff"]);
        assert_eq!(game.limited_skill_used(0, "zhongyi"), after["spent"]);
        assert_eq!(game.active_plays(0).is_empty(), after["disabled"]);
        submit(
            &mut game,
            0,
            Action::PlayCard {
                card: slash.id,
                targets: vec![1],
            },
        );
        finish_decisions(&mut game);
        assert_eq!(
            game.damage_history().last().unwrap().amount,
            expected["attack"][0]["amount"].as_u64().unwrap() as u16
        );
    }
}
#[test]
fn black_equipment_diverted_from_expansion_keeps_live_buff() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/zhongyi-boundaries.json"
    ))
    .unwrap();
    let expected = golden["trials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|trial| trial["name"] == "move-away")
        .unwrap();
    let mut game = boundary_table();
    let equipment = game
        .create_content_card(CardKind::Qinggang, Suit::Spade, 6, None)
        .unwrap();
    let slash = game
        .create_content_card(CardKind::Sha, Suit::Club, 3, None)
        .unwrap();
    game.seat_mut(0).equipment.push(equipment);
    game.seat_mut(0).hand.push(slash);
    let active = game.register_active_skill(registration(0)).unwrap();
    submit(
        &mut game,
        0,
        Action::ActivateSkill {
            registration: active,
            cards: vec![equipment.id],
            targets: vec![],
        },
    );
    assert_eq!(game.expansions(0, "zhongyi2"), [equipment]);
    assert_eq!(game.has_skill(0, "zhongyi2"), expected["activated"]["buff"]);
    let mut divert = probe("useCard2", move |ctx, _, _| {
        ctx.gain_expansion(0, 1, equipment.id).unwrap();
        TriggerStep::Done
    });
    divert.filter = Arc::new(move |_, e, _| e.cards.contains(&slash));
    game.register_trigger(divert).unwrap();
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: slash.id,
            targets: vec![1],
        },
    );
    finish_decisions(&mut game);
    assert!(game.seat(1).unwrap().hand.contains(&equipment));
    assert_eq!(
        game.is_known_by(equipment.id, None),
        expected["diverted"]["knownToSpectator"]
    );
    assert_eq!(
        !game.expansion_tags(equipment.id).is_empty(),
        expected["diverted"]["tagged"]
    );
    assert_eq!(game.has_skill(0, "zhongyi2"), expected["diverted"]["buff"]);
    assert_eq!(
        game.damage_history().last().unwrap().amount,
        expected["attack"][0]["amount"].as_u64().unwrap() as u16
    );
    let moments = Arc::new(Mutex::new(Vec::new()));
    let request = moments.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.timing == "roundStart" {
            request.lock().unwrap().push(game.has_skill(0, "zhongyi2"));
        }
    }));
    let callback = moments.clone();
    game.register_trigger(probe("roundStart", move |ctx, _, _| {
        callback
            .lock()
            .unwrap()
            .push(ctx.game().has_skill(0, "zhongyi2"));
        TriggerStep::Done
    }))
    .unwrap();
    for _ in 0..100 {
        if game.view(None).unwrap().turn.unwrap().round == 2 {
            break;
        }
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        if game.view(Some(seat)).unwrap().choice.is_some() {
            finish_decisions(&mut game);
        } else {
            submit(&mut game, seat, Action::EndPlay);
        }
    }
    assert_eq!(
        *moments.lock().unwrap(),
        [
            golden["expiry"]["request"]["buff"].as_bool().unwrap(),
            golden["expiry"]["callback"]["buff"].as_bool().unwrap()
        ]
    );
    assert!(game.seat(1).unwrap().hand.contains(&equipment));
    assert!(!game.has_skill(0, "zhongyi2"));
}
#[test]
fn stacked_holders_remove_dead_holders_material_once_and_survivor_remains() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/zhongyi-boundaries.json"
    ))
    .unwrap();
    let expected = golden["trials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|trial| trial["name"] == "holder-death")
        .unwrap();
    let mut game = boundary_table();
    let mat0 = game
        .create_content_card(CardKind::Tao, Suit::Heart, 6, None)
        .unwrap();
    let mat1 = game
        .create_content_card(CardKind::Tao, Suit::Heart, 7, None)
        .unwrap();
    let first = game
        .create_content_card(CardKind::Sha, Suit::Club, 3, None)
        .unwrap();
    let second = game
        .create_content_card(CardKind::Sha, Suit::Club, 4, None)
        .unwrap();
    let weapon = game
        .create_content_card(CardKind::Zhuge, Suit::Club, 1, None)
        .unwrap();
    game.seat_mut(0).hand.push(mat0);
    game.seat_mut(1).hand.push(mat1);
    game.seat_mut(1).hp = 3;
    game.seat_mut(2).hand.extend([first, second]);
    game.seat_mut(2).equipment.push(weapon);
    let death_rows = Arc::new(Mutex::new(Vec::new()));
    let output = death_rows.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.player != Some(1)
            || ![
                "loseToDiscardpileBefore",
                "loseToDiscardpileAfter",
                "loseBefore",
                "loseAfter",
            ]
            .contains(&event.timing.as_str())
        {
            return;
        }
        if !game.seat(1).unwrap().alive {
            let where_ = if game.seat(1).unwrap().expansions.contains(&mat1) {
                "E:x"
            } else if game.discard.contains(&mat1) {
                "d"
            } else {
                "other"
            };
            output.lock().unwrap().push(json!([
                event.timing,
                where_,
                !game.expansion_tags(mat1.id).is_empty(),
                game.has_skill(1, "zhongyi2")
            ]));
        }
    }));
    let first_skill = game.register_active_skill(registration(0)).unwrap();
    let second_skill = game.register_active_skill(registration(1)).unwrap();
    submit(
        &mut game,
        0,
        Action::ActivateSkill {
            registration: first_skill,
            cards: vec![mat0.id],
            targets: vec![],
        },
    );
    let mut activated = false;
    let mut hits = 0;
    for _ in 0..100 {
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        let private = game.view(Some(seat)).unwrap();
        if let Some(choice) = private.choice {
            let action = match choice.kind {
                ChoiceKind::Discard => Action::Discard {
                    event: choice.event,
                    cards: choice.cards[..choice.min].to_vec(),
                },
                ChoiceKind::Shan | ChoiceKind::Save => Action::Choose {
                    event: choice.event,
                    card: None,
                },
                ChoiceKind::Content => {
                    let b = &choice.branches[0];
                    Action::Select {
                        event: choice.event,
                        branch: b.id,
                        cards: b.cards[..b.min_cards].to_vec(),
                        targets: b.targets[..b.min_targets].to_vec(),
                    }
                }
            };
            submit(&mut game, seat, action);
            continue;
        }
        if seat == 1 && !activated {
            activated = true;
            submit(
                &mut game,
                1,
                Action::ActivateSkill {
                    registration: second_skill,
                    cards: vec![mat1.id],
                    targets: vec![],
                },
            );
            continue;
        }
        if seat == 2 && hits < 2 {
            let (card, target) = if hits == 0 { (first, 1) } else { (second, 0) };
            assert!(private.actions.contains(&Action::PlayCard {
                card: card.id,
                targets: vec![target]
            }));
            hits += 1;
            submit(
                &mut game,
                seat,
                Action::PlayCard {
                    card: card.id,
                    targets: vec![target],
                },
            );
            continue;
        }
        if hits == 2 {
            break;
        }
        submit(&mut game, seat, Action::EndPlay);
    }
    assert!(activated && hits == 2);
    assert_eq!(
        game.seat(1).unwrap().alive,
        !expected["dead"].as_bool().unwrap()
    );
    assert_eq!(game.discard.iter().filter(|c| **c == mat1).count(), 1);
    assert!(game.expansion_tags(mat1.id).is_empty());
    assert!(!game.has_skill(1, "zhongyi2"));
    assert!(game.has_skill(0, "zhongyi2"));
    assert!(game.seat(0).unwrap().expansions.contains(&mat0));
    let expected_death: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| {
            e["sample"] == "holder-death"
                && e["kind"] == "request"
                && [
                    "loseToDiscardpileBefore",
                    "loseToDiscardpileAfter",
                    "loseBefore",
                    "loseAfter",
                ]
                .contains(&e["timing"].as_str().unwrap())
                && e["buff"] == false
        })
        .map(|e| json!([e["timing"], e["zone"], e["tagged"], e["buff"]]))
        .collect();
    assert_eq!(*death_rows.lock().unwrap(), expected_death);
    let damage: Vec<_> = game.damage_history().iter().map(|d| d.amount).collect();
    assert_eq!(
        damage,
        [
            expected["stacked"][0]["amount"].as_u64().unwrap() as u16,
            expected["afterDeathAttack"][0]["amount"].as_u64().unwrap() as u16
        ]
    );
}
#[test]
fn virtual_slash_and_fire_chain_use_the_same_bonus_without_reapplying_it() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/zhongyi-boundaries.json"
    ))
    .unwrap();
    let expected = golden["trials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|trial| trial["name"] == "move-away")
        .unwrap();
    let mut game = boundary_table();
    game.seat_mut(0).hero = crate::classic_data::roster()
        .iter()
        .find(|hero| hero.id == "guanyu");
    let material = game
        .create_content_card(CardKind::Shan, Suit::Spade, 2, None)
        .unwrap();
    let red = game
        .create_content_card(CardKind::Tao, Suit::Heart, 6, None)
        .unwrap();
    let fire = game
        .create_content_card(
            CardKind::Sha,
            Suit::Heart,
            7,
            Some(crate::DamageNature::Fire),
        )
        .unwrap();
    let weapon = game
        .create_content_card(CardKind::Zhuge, Suit::Club, 1, None)
        .unwrap();
    game.seat_mut(0).hand.extend([material, red, fire]);
    game.seat_mut(0).equipment.push(weapon);
    let active = game.register_active_skill(registration(0)).unwrap();
    submit(
        &mut game,
        0,
        Action::ActivateSkill {
            registration: active,
            cards: vec![material.id],
            targets: vec![],
        },
    );
    submit(
        &mut game,
        0,
        Action::PlayVirtual {
            kind: CardKind::Sha,
            cards: vec![red.id],
            targets: vec![1],
        },
    );
    finish_decisions(&mut game);
    game.seat_mut(1).linked = true;
    game.seat_mut(2).linked = true;
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: fire.id,
            targets: vec![1],
        },
    );
    finish_decisions(&mut game);
    let hits = game.damage_history();
    assert_eq!(hits.len(), 3);
    assert_eq!(
        (hits[0].amount, hits[0].card_kind, hits[0].cards.as_slice()),
        (
            expected["attack"][0]["amount"].as_u64().unwrap() as u16,
            Some(CardKind::Sha),
            &[red][..]
        )
    );
    assert_eq!(
        (hits[1].amount, hits[1].propagated),
        (
            expected["attack"][0]["amount"].as_u64().unwrap() as u16,
            false
        )
    );
    assert_eq!(
        (hits[2].amount, hits[2].propagated),
        (
            expected["attack"][1]["amount"].as_u64().unwrap() as u16,
            true
        )
    );
    assert_eq!(hits[2].target, 2);
}
#[test]
fn cleanup_during_damage_does_not_skip_dying_and_die_after_sees_reward() {
    let mut game = boundary_table();
    let material = game
        .create_content_card(CardKind::Tao, Suit::Heart, 3, None)
        .unwrap();
    let slash = game
        .create_content_card(CardKind::Sha, Suit::Club, 4, None)
        .unwrap();
    game.seat_mut(0).hand.extend([material, slash]);
    game.seat_mut(0).role = Some(crate::classic_data::Role::Lord);
    game.seat_mut(1).role = Some(crate::classic_data::Role::Rebel);
    game.seat_mut(1).hp = 1;
    let active = game.register_active_skill(registration(0)).unwrap();
    submit(
        &mut game,
        0,
        Action::ActivateSkill {
            registration: active,
            cards: vec![material.id],
            targets: vec![],
        },
    );
    let mut expiry = TemporarySkill::new(
        "cleanup_damage",
        vec![SkillExpiry::new(TriggerRole::Global, "damage")],
    );
    expiry.on_remove = Some(Arc::new(|ctx| ctx.discard_expansions("zhongyi2")));
    game.grant_temporary(0, expiry).unwrap();
    let seen = Arc::new(Mutex::new(None));
    let out = seen.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.timing == "dieAfter" && event.player == Some(1) {
            *out.lock().unwrap() = Some((
                game.seat(0).unwrap().hand.len(),
                game.seat(1).unwrap().alive,
            ));
        }
    }));
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: slash.id,
            targets: vec![1],
        },
    );
    finish_decisions(&mut game);
    assert!(!game.seat(1).unwrap().alive);
    assert_eq!(
        game.discard
            .iter()
            .filter(|card| **card == material)
            .count(),
        1
    );
    assert_eq!(*seen.lock().unwrap(), Some((3, false)));
}
#[test]
fn zero_material_virtual_fire_slash_and_chain_match_boundary_amounts() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/zhongyi-boundaries.json"
    ))
    .unwrap();
    let expected = golden["trials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|trial| trial["name"] == "move-away")
        .unwrap();
    let mut game = boundary_table();
    let material = game
        .create_content_card(CardKind::Tao, Suit::Heart, 3, None)
        .unwrap();
    game.seat_mut(0).hand.push(material);
    game.seat_mut(1).linked = true;
    game.seat_mut(2).linked = true;
    let active = game.register_active_skill(registration(0)).unwrap();
    game.register_trigger(probe("useSkillAfter", |ctx, _, _| {
        ctx.use_virtual_sha(0, 1, Some(crate::DamageNature::Fire))
            .unwrap();
        TriggerStep::Done
    }))
    .unwrap();
    submit(
        &mut game,
        0,
        Action::ActivateSkill {
            registration: active,
            cards: vec![material.id],
            targets: vec![],
        },
    );
    finish_decisions(&mut game);
    let hits = game.damage_history();
    assert_eq!(hits.len(), 2);
    for (i, hit) in hits.iter().enumerate() {
        assert_eq!(
            hit.amount,
            expected["attack"][i]["amount"].as_u64().unwrap() as u16
        );
        assert_eq!(
            hit.cards.len(),
            expected["attack"][i]["physicalCards"].as_u64().unwrap() as usize
        );
        assert_eq!(hit.target, i as SeatId + 1);
        assert_eq!(hit.propagated, i == 1);
    }
}
#[test]
fn death_during_skill_begin_does_not_spend_or_run_zhongyi() {
    let mut game = boundary_table();
    game.seat_mut(1).role = Some(crate::classic_data::Role::Rebel);
    game.seat_mut(1).hp = 1;
    let material = game
        .create_content_card(CardKind::Shan, Suit::Spade, 2, None)
        .unwrap();
    game.seat_mut(1).hand.push(material);
    let active = game.register_active_skill(registration(1)).unwrap();
    game.register_trigger(probe("useSkillBegin", |ctx, _, _| {
        ctx.damage(Some(0), 1, 1, None, None, &[]).unwrap();
        TriggerStep::Done
    }))
    .unwrap();
    submit(&mut game, 0, Action::EndPlay);
    finish_decisions(&mut game);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    submit(
        &mut game,
        1,
        Action::ActivateSkill {
            registration: active,
            cards: vec![material.id],
            targets: vec![],
        },
    );
    finish_decisions(&mut game);
    assert!(!game.seat(1).unwrap().alive);
    assert!(!game.limited_skill_used(1, "zhongyi"));
    assert!(!game.has_skill(1, "zhongyi2"));
    assert!(game.expansions(1, "zhongyi2").is_empty());
}
#[test]
fn expansion_identity_knowledge_matches_hand_and_equipment_reference() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/zhongyi-boundaries.json"
    ))
    .unwrap();
    for (sample, equipped) in [("holder-death", false), ("move-away", true)] {
        let mut game = boundary_table();
        let material = game
            .create_content_card(
                if equipped {
                    CardKind::Qinggang
                } else {
                    CardKind::Tao
                },
                if equipped { Suit::Spade } else { Suit::Heart },
                6,
                None,
            )
            .unwrap();
        if equipped {
            game.seat_mut(0).equipment.push(material);
        } else {
            game.seat_mut(0).hand.push(material);
        }
        let active = game.register_active_skill(registration(0)).unwrap();
        let rows = Arc::new(Mutex::new(Vec::new()));
        let output = rows.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if ![
                "addToExpansionBefore",
                "loseBefore",
                "loseAfter",
                "addToExpansionAfter",
            ]
            .contains(&event.timing.as_str())
                || !event.cards.contains(&material)
            {
                return;
            }
            let spectator = game.view(None).unwrap();
            let in_special = game.transferring.contains(&material);
            let known = game.is_known_by(material.id, None);
            assert_eq!(
                spectator.known_special.contains(&material),
                in_special && known
            );
            if game.seat(0).unwrap().expansions.contains(&material) {
                assert!(spectator.seats[0]
                    .expansions
                    .iter()
                    .any(|entry| entry.card == material));
                assert!(spectator
                    .card_faces
                    .iter()
                    .any(|face| face.id == material.id));
            }
            output.lock().unwrap().push((event.timing.clone(), known));
        }));
        submit(
            &mut game,
            0,
            Action::ActivateSkill {
                registration: active,
                cards: vec![material.id],
                targets: vec![],
            },
        );
        let reference: Vec<_> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| {
                e["sample"] == sample
                    && e["kind"] == "request"
                    && [
                        "addToExpansionBefore",
                        "loseBefore",
                        "loseAfter",
                        "addToExpansionAfter",
                    ]
                    .contains(&e["timing"].as_str().unwrap())
            })
            .take(4)
            .map(|e| {
                (
                    e["timing"].as_str().unwrap().to_string(),
                    e["knownToSpectator"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(*rows.lock().unwrap(), reference);
    }
}

#[test]
fn death_during_expansion_transit_leaves_public_material_in_special() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/zhongyi-transit-death.json"
    ))
    .unwrap();
    let mut game = boundary_table();
    game.seat_mut(1).role = Some(crate::classic_data::Role::Rebel);
    game.seat_mut(1).hp = 1;
    let material = game
        .create_content_card(CardKind::Qinggang, Suit::Spade, 6, None)
        .unwrap();
    game.seat_mut(1).equipment.push(material);
    let active = game.register_active_skill(registration(1)).unwrap();
    let rows = Arc::new(Mutex::new(Vec::new()));
    let output = rows.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if ![
            "useSkillBegin",
            "addToExpansionBefore",
            "loseBefore",
            "loseAfter",
            "dieAfter",
            "addToExpansionAfter",
            "useSkillAfter",
        ]
        .contains(&event.timing.as_str())
        {
            return;
        }
        if event.player != Some(1) {
            return;
        }
        if event.timing.starts_with("lose") && !event.cards.contains(&material) {
            return;
        }
        let special = game.special_cards().contains(&material);
        let equipped = game.seat(1).unwrap().equipment.contains(&material);
        let expansion = game.seat(1).unwrap().expansions.contains(&material);
        let discarded = game.discard.contains(&material);
        let current_zone = if special {
            "s"
        } else if equipped {
            "e"
        } else if expansion {
            "x"
        } else if discarded {
            "d"
        } else {
            "?"
        };
        let known = game.is_known_by(material.id, None);
        let view = game.view(None).unwrap();
        assert_eq!(view.known_special.contains(&material), special && known);
        output.lock().unwrap().push(json!({"timing": event.timing, "zone": current_zone, "special": special, "discard": discarded, "equipped": equipped, "inExpansion": expansion, "knownToSpectator": known, "alive": game.seat(1).unwrap().alive, "tag": !game.expansion_tags(material.id).is_empty(), "buff": game.has_skill(1, "zhongyi2"), "spent": game.limited_skill_used(1, "zhongyi")}));
    }));
    let mut kill = probe("loseAfter", |ctx, _, _| {
        ctx.damage(Some(0), 1, 1, None, None, &[]).unwrap();
        TriggerStep::Done
    });
    kill.filter =
        Arc::new(move |_, event, _| event.player == Some(1) && event.cards.contains(&material));
    game.register_trigger(kill).unwrap();
    submit(&mut game, 0, Action::EndPlay);
    finish_decisions(&mut game);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    submit(
        &mut game,
        1,
        Action::ActivateSkill {
            registration: active,
            cards: vec![material.id],
            targets: vec![],
        },
    );
    finish_decisions(&mut game);
    let observed = rows.lock().unwrap();
    let expected: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| {
            e["kind"] == "request"
                && [
                    "useSkillBegin",
                    "addToExpansionBefore",
                    "loseBefore",
                    "loseAfter",
                    "dieAfter",
                    "addToExpansionAfter",
                    "useSkillAfter",
                ]
                .contains(&e["timing"].as_str().unwrap())
        })
        .collect();
    assert_eq!(observed.len(), expected.len(), "{observed:?}");
    for (actual, reference) in observed.iter().zip(expected) {
        for key in [
            "timing",
            "zone",
            "special",
            "discard",
            "equipped",
            "inExpansion",
            "knownToSpectator",
            "alive",
            "tag",
            "buff",
            "spent",
        ] {
            assert_eq!(actual[key], reference[key], "{}: {key}", actual["timing"]);
        }
    }
    assert_eq!(
        game.special_cards()
            .iter()
            .filter(|card| **card == material)
            .count(),
        1
    );
    assert!(game.match_result().is_none());
}
#[test]
fn bot_nonlimited_zero_cost_and_cancelled_skill_do_not_loop() {
    for cancel in [false, true] {
        let mut game = boundary_table();
        let ran = Arc::new(Mutex::new(0));
        let out = ran.clone();
        let skill = crate::ActiveSkillRegistration {
            owner: 1,
            skill: "repeatable".into(),
            limited: false,
            choice: Arc::new(|_, _| {
                Some(ChoiceBranch {
                    id: 0,
                    label: "zero".into(),
                    cards: vec![],
                    min_cards: 0,
                    max_cards: 0,
                    targets: vec![],
                    min_targets: 0,
                    max_targets: 0,
                })
            }),
            effect: Arc::new(move |_, _, _| {
                *out.lock().unwrap() += 1;
                TriggerStep::Done
            }),
        };
        let active = game.register_active_skill(skill).unwrap();
        if cancel {
            game.register_trigger(probe("useSkillBegin", |_, _, _| TriggerStep::CancelSubject))
                .unwrap();
        }
        submit(&mut game, 0, Action::EndPlay);
        finish_decisions(&mut game);
        assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
        game.seat_mut(1).hand.clear();
        let first = game.bot_command().unwrap();
        assert!(
            matches!(first.action, Action::ActivateSkill { registration, .. } if registration == active)
        );
        game.submit(first).unwrap();
        assert_eq!(*ran.lock().unwrap(), usize::from(!cancel));
        let second = game.bot_command().unwrap();
        assert_eq!(second.action, Action::EndPlay);
        game.submit(second).unwrap();
        assert_ne!(game.view(None).unwrap().turn.unwrap().seat, 1);
    }
}
