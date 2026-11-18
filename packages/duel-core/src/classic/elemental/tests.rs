use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::{Role, Suit};
use crate::{
    Action, CardFace, ChoiceBranch, ChoiceKind, Command, Phase, TriggerOrder, TriggerRegistration,
    TriggerRole, TriggerStep,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[path = "hp_tests.rs"]
mod hp;

const CALLBACKS: [&str; 6] = [
    "linkAfter",
    "changeHp",
    "damage",
    "damageSource",
    "damageEnd",
    "damageAfter",
];

fn registration(owner: SeatId, name: &str, timing: &str) -> TriggerRegistration {
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

fn option(id: u32, label: &str) -> ChoiceBranch {
    ChoiceBranch {
        id,
        label: label.into(),
        cards: vec![],
        min_cards: 0,
        max_cards: 0,
        targets: vec![],
        min_targets: 0,
        max_targets: 0,
    }
}

fn submit(game: &mut Game, seat: SeatId, action: Action) {
    game.submit(Command {
        version: game.version,
        seat,
        action,
    })
    .unwrap();
}

fn label(actor: SeatId, seat: SeatId) -> &'static str {
    match (seat + 8 - actor) % 8 {
        0 => "A",
        1 => "B",
        2 => "C",
        3 => "D",
        _ => "other",
    }
}

fn states(game: &Game, actor: SeatId, count: u32) -> Value {
    let public = game.view(None).unwrap();
    (0..count)
        .map(|offset| {
            let seat = (actor + offset) % 8;
            let p = &public.seats[seat as usize];
            assert!(p.hand.is_none());
            (
                label(actor, seat).to_owned(),
                json!({
                    "hp": p.hp, "hujia": p.armor, "linked": p.linked, "handSize": p.hand_count,
                }),
            )
        })
        .collect()
}

fn frame_name(game: &Game, id: EventId) -> String {
    let (_, program) = game
        .scheduler
        .active_frame(id)
        .expect("the observed event is running");
    match program.kind() {
        EventKind::TrickEffect {
            kind: CardKind::Tiesuo,
            ..
        } => "tiesuo",
        EventKind::Phase {
            phase: Phase::Play, ..
        } => "play",
        EventKind::UseSha { .. } | EventKind::UseTrick { .. } => "useCard",
        EventKind::ShaEffect { .. } => "sha",
        EventKind::Damage { .. } => "damage",
        EventKind::ChangeHp { .. } => "changeHp",
        EventKind::ChangeArmor { .. } => "changeHujia",
        EventKind::Link { .. } => "link",
        EventKind::UnlinkAfterDamage { .. } => "_lianhuan4",
        EventKind::SpreadDamage { .. } => "_lianhuan",
        EventKind::RegisteredTrigger { registration, .. } => {
            return game.trigger_registrations[&registration].skill.clone()
        }
        other => panic!("unexpected reference event {other:?}"),
    }
    .to_owned()
}

fn row(game: &Game, event: &TriggerEvent, sample: &str, kind: &str) -> Value {
    let actor = game.active_seat;
    let player = game.seat(event.player.unwrap()).unwrap();
    let subject = frame_name(game, event.subject);
    let amount = match subject.as_str() {
        "changeHp" | "changeHujia" => event.delta,
        "link" => None,
        _ => Some(i32::from(event.amount)),
    };
    json!([
        sample,
        kind,
        event.timing,
        subject,
        event.parent.map(|id| frame_name(game, id)),
        label(actor, player.id),
        event.source.map(|id| label(actor, id)),
        event.card_kind,
        event.nature,
        amount,
        player.hp,
        player.armor,
        player.linked
    ])
}

fn answer_effects(game: &mut Game, choices: &[Value]) {
    let mut answered = 0;
    while let Some(crate::Decision::Cards(_)) = &game.decision {
        let seat = game.view(None).unwrap().waiting_for.unwrap();
        let private = game.view(Some(seat)).unwrap();
        let choice = private.choice.unwrap();
        let action = match choice.kind {
            ChoiceKind::Shan => Action::Choose {
                event: choice.event,
                card: None,
            },
            ChoiceKind::Content => {
                let branch = if choice.counter.is_some() {
                    0
                } else {
                    assert_eq!(
                        choice
                            .branches
                            .iter()
                            .map(|b| b.label.as_str())
                            .collect::<Vec<_>>(),
                        ["放弃", "发动法恩"]
                    );
                    assert_eq!(
                        label(game.active_seat, seat),
                        choices[answered]["owner"].as_str().unwrap()
                    );
                    assert!(game.view(None).unwrap().choice.is_none());
                    let accepted = choices[answered]["accepted"].as_bool().unwrap();
                    answered += 1;
                    u32::from(accepted)
                };
                Action::Select {
                    event: choice.event,
                    branch,
                    cards: vec![],
                    targets: vec![],
                }
            }
            other => panic!("unexpected golden choice {other:?}"),
        };
        submit(game, seat, action);
    }
    assert_eq!(answered, choices.len());
}

fn face(game: &Game, physical: Card) -> Value {
    let printed = game.face(physical.id).unwrap();
    json!([physical.kind, printed.suit, printed.rank])
}

#[test]
fn three_turns_match_upstream_link_armor_and_damage() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/linked-damage.json"
    ))
    .unwrap();
    let roles = [
        Role::Lord,
        Role::Rebel,
        Role::Loyalist,
        Role::Rebel,
        Role::Loyalist,
        Role::Rebel,
        Role::Loyalist,
        Role::Renegade,
    ];
    let mut game = scene(&roles, 0);
    game.classic.as_mut().unwrap().turn = None;
    // Before the relevant turn, apply its setup through trusted content and real events.
    let dealt = Arc::new(Mutex::new(BTreeMap::new()));
    for (actor, sample) in [(0, "unarmored"), (1, "armored"), (2, "recheck")] {
        let mut setup = registration(actor, "reference_setup", "phaseZhunbeiBegin");
        setup.role = TriggerRole::Player;
        let dealt = dealt.clone();
        setup.effect = Arc::new(move |ctx, _, _| {
            if sample == "armored" {
                ctx.change_armor(actor + 1, 1).unwrap();
            } else if sample == "recheck" {
                ctx.link(actor + 3, Some(true)).unwrap();
            }
            let tiesuo = ctx
                .gain_created_card(actor, CardKind::Tiesuo, Suit::Club, 10, None)
                .unwrap();
            let sha = ctx
                .gain_created_card(
                    actor,
                    CardKind::Sha,
                    Suit::Heart,
                    4,
                    Some(DamageNature::Fire),
                )
                .unwrap();
            dealt.lock().unwrap().insert(
                actor,
                (card(tiesuo, CardKind::Tiesuo), card(sha, CardKind::Sha)),
            );
            TriggerStep::Done
        });
        game.register_trigger(setup).unwrap();
    }
    game.run(None);
    let mut all_rows = Vec::new();
    for trial in golden["trials"].as_array().unwrap() {
        let sample = trial["sample"].as_str().unwrap().to_owned();
        let turn = game.classic.as_ref().unwrap().turn.unwrap();
        assert_eq!(turn.number, trial["turn"].as_u64().unwrap());
        let a = turn.seat;
        let (b, c, d) = (a + 1, a + 2, a + 3);
        let count = if sample == "recheck" { 4 } else { 3 };
        let (tiesuo, sha) = dealt.lock().unwrap()[&a];
        for seat in &mut game.seats {
            let (keep, clear): (Vec<_>, Vec<_>) = seat
                .hand
                .drain(..)
                .partition(|&held| seat.id == a && (held == tiesuo || held == sha));
            seat.hand = keep;
            game.discard.extend(clear);
            seat.hp = 4;
        }
        let first = game
            .create_content_card(CardKind::Tao, Suit::Heart, 7, None)
            .unwrap();
        let second = game
            .create_content_card(CardKind::Tao, Suit::Heart, 8, None)
            .unwrap();
        game.classic.as_mut().unwrap().deck.extend([second, first]);
        assert_eq!(states(&game, a, count), trial["initial"]["states"]);
        assert_eq!(
            json!([face(&game, tiesuo), face(&game, sha)]),
            trial["initial"]["actorHand"]
        );
        let private = game.view(Some(a)).unwrap();
        assert_eq!(private.card_natures[&sha.id], DamageNature::Fire);
        assert!(!game.view(None).unwrap().card_natures.contains_key(&sha.id));
        assert!(!game
            .view(Some(b))
            .unwrap()
            .card_natures
            .contains_key(&sha.id));
        assert!(private.actions.contains(&Action::PlayCard {
            card: tiesuo.id,
            targets: vec![b, c]
        }));
        assert!(private
            .actions
            .contains(&Action::Recast { card: tiesuo.id }));
        assert!(private.actions.contains(&Action::PlayCard {
            card: sha.id,
            targets: vec![b]
        }));
        assert!(!game.classic.as_ref().unwrap().sha_played);

        let log = Arc::new(Mutex::new(Vec::<Value>::new()));
        let parents = Arc::new(Mutex::new(BTreeMap::new()));
        let observed_parents = parents.clone();
        let request_log = log.clone();
        let sample_name = sample.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if event.timing == "damage" {
                let parent = event.parent.unwrap();
                observed_parents
                    .lock()
                    .unwrap()
                    .insert(event.subject, (parent, frame_name(game, parent)));
            }
            if CALLBACKS.contains(&event.timing.as_str())
                || ["linkBefore", "changeHujiaAfter", "useCardAfter"]
                    .contains(&event.timing.as_str())
            {
                request_log
                    .lock()
                    .unwrap()
                    .push(row(game, event, &sample_name, "request"));
            }
        }));
        let mut registrations = Vec::new();
        let used = Arc::new(Mutex::new(Vec::new()));
        let materials = used.clone();
        let mut use_card = registration(a, "reference_trick_material", "useCard");
        use_card.role = TriggerRole::Player;
        use_card.effect = Arc::new(move |ctx, _, _| {
            materials
                .lock()
                .unwrap()
                .push((ctx.event().card_kind, ctx.event().cards.clone()));
            TriggerStep::Done
        });
        registrations.push(game.register_trigger(use_card).unwrap());
        for timing in CALLBACKS {
            let mut observer = registration(a, "reference_link_observer", timing);
            observer.order = TriggerOrder::First;
            let callback_log = log.clone();
            let sample_name = sample.clone();
            observer.effect = Arc::new(move |ctx, _, _| {
                callback_log.lock().unwrap().push(row(
                    ctx.game(),
                    ctx.event(),
                    &sample_name,
                    "callback",
                ));
                TriggerStep::Done
            });
            registrations.push(game.register_trigger(observer).unwrap());
        }

        let decisions = Arc::new(Mutex::new(Vec::<Value>::new()));
        let draws = Arc::new(Mutex::new(Vec::<Value>::new()));
        let mut faen = registration(b, "faen", "linkAfter");
        faen.filter = Arc::new(|game, event, _| {
            game.view(None)
                .unwrap()
                .seats
                .iter()
                .any(|s| Some(s.id) == event.player && s.linked)
        });
        let faen_decisions = decisions.clone();
        let faen_draws = draws.clone();
        let prior = Mutex::new(None);
        faen.effect = Arc::new(move |ctx, stage, selected| {
            let target = ctx.event().player.unwrap();
            match stage {
                0 => TriggerStep::Choose {
                    branches: vec![option(0, "放弃"), option(1, "发动法恩")],
                    then: 1,
                },
                1 => {
                    let accepted = selected.unwrap().branch == 1;
                    faen_decisions.lock().unwrap().push(json!({
                        "owner": label(a, ctx.owner()),
                        "target": label(a, target),
                        "accepted": accepted,
                    }));
                    if accepted {
                        let view = ctx.game().view(Some(target)).unwrap();
                        let seat = &view.seats[target as usize];
                        *prior.lock().unwrap() = Some((seat.linked, seat.hand.clone().unwrap()));
                        ctx.draw(target).unwrap();
                        TriggerStep::Continue(2)
                    } else {
                        TriggerStep::Done
                    }
                }
                _ => {
                    let (linked_before, previous) = prior.lock().unwrap().take().unwrap();
                    let private = ctx.game().view(Some(target)).unwrap();
                    let drawn: Vec<_> = private.seats[target as usize]
                        .hand
                        .as_ref()
                        .unwrap()
                        .iter()
                        .filter(|card| !previous.contains(card))
                        .copied()
                        .collect();
                    let seat = &ctx.game().view(None).unwrap().seats[target as usize];
                    assert!(drawn.iter().all(|card| !seat.known_hand.contains(card)));
                    let faces: Vec<_> = drawn
                        .into_iter()
                        .map(|card| face(ctx.game(), card))
                        .collect();
                    faen_draws.lock().unwrap().push(json!({
                        "name": "faen",
                        "owner": label(a, ctx.owner()),
                        "target": label(a, target),
                        "linkedBefore": linked_before,
                        "linkedAfter": seat.linked,
                        "timing": ctx.event().timing,
                        "drawn": faces,
                    }));
                    TriggerStep::Done
                }
            }
        });
        registrations.push(game.register_trigger(faen).unwrap());
        let recheck = Arc::new(Mutex::new(Value::Null));
        if sample == "recheck" {
            let mut release = registration(c, "reference_unlink_pending", "damageEnd");
            release.role = TriggerRole::Player;
            let observed = recheck.clone();
            release.effect = Arc::new(move |ctx, stage, _| {
                if stage == 0 {
                    let pending: Vec<_> = ctx
                        .game()
                        .pending_linked_targets(ctx.event().subject)
                        .unwrap()
                        .map(|seat| label(a, seat))
                        .collect();
                    *observed.lock().unwrap() = json!({
                        "duringDamageTo": label(a, ctx.event().player.unwrap()),
                        "pending": pending,
                        "linkedBefore": ctx.game().view(None).unwrap().seats[d as usize].linked,
                    });
                    ctx.link(d, Some(false)).unwrap();
                    TriggerStep::Continue(1)
                } else {
                    observed.lock().unwrap()["linkedAfter"] =
                        json!(ctx.game().view(None).unwrap().seats[d as usize].linked);
                    TriggerStep::Done
                }
            });
            registrations.push(game.register_trigger(release).unwrap());
        }
        // Reverse the user's target selection. Card effects run in current-turn order.
        submit(
            &mut game,
            a,
            Action::PlayCard {
                card: tiesuo.id,
                targets: vec![c, b],
            },
        );
        answer_effects(&mut game, trial["choices"].as_array().unwrap());
        assert_eq!(states(&game, a, count), trial["afterTiesuo"]);
        assert_eq!(
            *used.lock().unwrap(),
            [(Some(CardKind::Tiesuo), vec![tiesuo])]
        );
        assert_eq!(json!(*decisions.lock().unwrap()), trial["choices"]);
        let faen_effects: Vec<_> = trial["effects"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|effect| effect["name"] == "faen")
            .cloned()
            .collect();
        assert_eq!(*draws.lock().unwrap(), faen_effects);
        submit(
            &mut game,
            a,
            Action::PlayCard {
                card: sha.id,
                targets: vec![b],
            },
        );
        answer_effects(&mut game, &[]);
        assert_eq!(states(&game, a, count), trial["final"]["states"]);
        if sample == "recheck" {
            assert_eq!(*recheck.lock().unwrap(), trial["recheck"]);
        }
        let damage: Vec<_> = game
            .damage_dealt(a)
            .filter(|hit| hit.turn == Some(turn.number))
            .collect();
        let actual_damage: Vec<_> = damage.iter().map(|hit| {
            let observed = parents.lock().unwrap();
            let (parent, name) = &observed[&hit.event];
            assert_eq!(hit.parent, Some(*parent));
            json!({
                "source": label(a, hit.source.unwrap()),
                "target": label(a, hit.target),
                "nature": hit.nature,
                "amount": hit.amount,
                "absorbed": hit.absorbed,
                "parent": name,
                "physicalSha": hit.cards == [sha],
                "inTargetHistory": game.damage_received(hit.target).any(|received| std::ptr::eq(received, *hit)),
            })
        }).collect();
        assert_eq!(json!(actual_damage), trial["damage"]);
        assert_eq!(
            damage.iter().map(|hit| hit.propagated).collect::<Vec<_>>(),
            [false, true]
        );
        for offset in 0..count {
            let seat = a + offset;
            assert_eq!(
                json!(damage.iter().filter(|h| h.target == seat).count()),
                trial["final"]["damageCounts"][label(a, seat)]
            );
        }
        let public = game.view(None).unwrap();
        assert_eq!(public.card_natures[&sha.id], DamageNature::Fire);
        assert!(game.classic.as_ref().unwrap().sha_played);
        for (kind, uses) in [("tiesuo", "tiesuoUses"), ("sha", "shaUses")] {
            let observed = log
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r[1] == "request" && r[2] == "useCardAfter" && r[7] == kind)
                .count();
            assert_eq!(json!(observed), trial["final"][uses]);
        }
        for played in [tiesuo, sha] {
            assert_eq!(public.discard.iter().filter(|c| **c == played).count(), 1);
            assert!(!public.resolving.contains(&played));
        }
        all_rows.extend(log.lock().unwrap().iter().cloned());
        game.timing_observer = None;
        for registration in registrations {
            assert!(game.unregister_trigger(registration));
        }
        if turn.number < 3 {
            submit(&mut game, a, Action::EndPlay);
        }
    }
    let mut expected = golden["events"].as_array().unwrap().clone();
    // The upstream fixture runs cards in a named wrapper. Our public commands run them in phaseUse.
    for row in &mut expected {
        if row[4] == "reference_fixture_use" {
            row[4] = json!("play");
        }
    }
    assert_eq!(all_rows, expected);
}

fn prescribed() -> Game {
    Game::new(crate::GameSetup {
        seats: [10, 42, 7, 99]
            .map(|id| crate::SeatSetup {
                id,
                hp: 4,
                hand: vec![],
            })
            .to_vec(),
        active_seat: 7,
    })
    .unwrap()
}

fn run_hit(
    game: &mut Game,
    source: Option<SeatId>,
    target: SeatId,
    amount: u16,
    nature: Option<DamageNature>,
) {
    let hit = game.elemental_damage_program(
        source,
        target,
        amount,
        DamageCard {
            nature,
            ..Default::default()
        },
        false,
    );
    game.scheduler.start(hit);
    game.run(None);
}

#[test]
fn propagation_uses_turn_anchor_original_amount_and_live_rule_priority() {
    let mut game = prescribed();
    for id in [99, 42, 10] {
        game.seat_mut(id).linked = true;
    }
    game.seat_mut(42).armor = 1;
    // An earlier normal candidate can remove a target before the -10 rule filters it.
    let mut rescue = registration(10, "intervening_change", "changeHp");
    rescue.role = TriggerRole::Player;
    rescue.priority = -9;
    rescue.effect = Arc::new(|ctx, _, _| {
        ctx.link(ctx.owner(), Some(false)).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(rescue).unwrap();
    run_hit(&mut game, Some(7), 42, 2, Some(DamageNature::Thunder));
    let hits: Vec<_> = game
        .damage_history()
        .iter()
        .map(|hit| {
            (
                hit.target,
                hit.amount,
                hit.absorbed,
                hit.nature,
                hit.propagated,
            )
        })
        .collect();
    assert_eq!(
        hits,
        [
            (42, 2, 1, Some(DamageNature::Thunder), false),
            (99, 2, 0, Some(DamageNature::Thunder), true),
            (10, 2, 0, Some(DamageNature::Thunder), true)
        ]
    );
    let after = game.view(None).unwrap();
    assert_eq!(
        after
            .seats
            .iter()
            .map(|p| (p.id, p.hp, p.armor, p.linked))
            .collect::<Vec<_>>(),
        [
            (10, 2, 0, false),
            (42, 3, 0, false),
            (7, 4, 0, false),
            (99, 2, 0, false)
        ]
    );
    assert!(!after
        .events
        .iter()
        .any(|e| matches!(e.kind, EventKind::UnlinkAfterDamage { seat: 10 })));
    assert!(after
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::Damage { .. }))
        .all(|e| e.nature == Some(DamageNature::Thunder)));
}

#[test]
fn cancellation_distinguishes_committed_damage_and_preserves_sha_cleanup() {
    for timing in ["damageBegin4", "damage"] {
        let mut game = prescribed();
        let sha = card(1, CardKind::Sha);
        game.seat_mut(7).hand.push(sha);
        game.set_sha_nature(sha.id, Some(DamageNature::Fire))
            .unwrap();
        for id in [99, 42] {
            game.seat_mut(id).linked = true;
        }
        game.seat_mut(99).armor = 1;
        let mut cancel = registration(99, "cancel_hit", timing);
        cancel.role = TriggerRole::Player;
        cancel.effect = Arc::new(|ctx, _, _| {
            assert_eq!(ctx.event().nature, Some(DamageNature::Fire));
            TriggerStep::CancelSubject
        });
        game.register_trigger(cancel).unwrap();
        submit(
            &mut game,
            7,
            Action::PlaySha {
                card: sha.id,
                target: 99,
            },
        );
        let choose = game.view(Some(99)).unwrap().choice.unwrap();
        submit(
            &mut game,
            99,
            Action::Choose {
                event: choose.event,
                card: None,
            },
        );
        let committed = timing == "damage";
        assert_eq!(game.damage_history().len(), usize::from(committed));
        if committed {
            assert_eq!(game.damage_history()[0].absorbed, 1);
        }
        assert_eq!(
            (
                game.seat(99).unwrap().hp,
                game.seat(99).unwrap().armor,
                game.seat(99).unwrap().linked
            ),
            (4, u16::from(!committed), !committed)
        );
        assert_eq!(
            (game.seat(42).unwrap().hp, game.seat(42).unwrap().linked),
            (4, true)
        );
        assert_eq!(game.discard, [sha]);
        assert!(game.resolving.is_empty());
    }
}

#[test]
fn lose_hp_bypasses_armor_and_cardless_normal_damage_keeps_links() {
    let mut game = prescribed();
    for id in [42, 10] {
        game.seat_mut(id).linked = true;
    }
    game.seat_mut(42).armor = 2;
    let mut loss = registration(42, "lose_hp", "damageEnd");
    loss.role = TriggerRole::Player;
    loss.effect = Arc::new(|ctx, _, _| {
        ctx.lose_hp(ctx.owner(), 1).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(loss).unwrap();
    run_hit(&mut game, None, 42, 1, None);
    assert_eq!(
        (
            game.seat(42).unwrap().hp,
            game.seat(42).unwrap().armor,
            game.seat(42).unwrap().linked
        ),
        (3, 1, true)
    );
    assert_eq!(game.seat(10).unwrap().hp, 4);
    assert_eq!(game.damage_history().len(), 1);
    assert_eq!(
        (
            game.damage_history()[0].nature,
            game.damage_history()[0].absorbed
        ),
        (None, 1)
    );
}

#[test]
fn a_turned_over_player_turns_face_up_and_skips_all_phases_once() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Renegade, Role::Rebel], 0);
    game.classic.as_mut().unwrap().turn = None;
    let mut setup = registration(0, "face_down", "phaseZhunbeiBegin");
    setup.role = TriggerRole::Player;
    setup.effect = Arc::new(|ctx, _, _| {
        ctx.turn_over(1, Some(true)).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(setup).unwrap();
    game.run(None);
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = events.clone();
    let mut on_flip = registration(1, "face_up", "turnOverAfter");
    on_flip.effect = Arc::new(move |ctx, _, _| {
        observed
            .lock()
            .unwrap()
            .push(ctx.game().view(None).unwrap().seats[1].turned_over);
        TriggerStep::Done
    });
    game.register_trigger(on_flip).unwrap();
    assert!(game.view(None).unwrap().seats[1].turned_over);
    submit(&mut game, 0, Action::EndPlay);
    let public = game.view(None).unwrap();
    assert_eq!(public.turn.unwrap().seat, 2);
    assert!(!public.seats[1].turned_over);
    assert_eq!(*events.lock().unwrap(), [false]);
    assert!(!public
        .events
        .iter()
        .any(|e| matches!(e.kind, EventKind::Phase { seat: 1, .. })));
    assert!(public
        .events
        .iter()
        .any(|e| matches!(e.kind, EventKind::Turn { seat: 1, .. })
            && e.status == crate::EventStatus::Cancelled));
}

#[test]
fn tiesuo_validates_targets_counters_each_effect_and_can_recast_from_hand() {
    let mut game = scene(
        &[Role::Lord, Role::Rebel, Role::Loyalist, Role::Renegade],
        0,
    );
    game.classic.as_mut().unwrap().turn = None;
    game.run(None);
    game.seat_mut(3).alive = false;
    let trick = card(31, CardKind::Tiesuo);
    let recast = card(32, CardKind::Tiesuo);
    let counter = card(33, CardKind::Wuxie);
    game.seat_mut(0).hand.extend([trick, recast]);
    game.seat_mut(1).hand.push(counter);
    assert!(game
        .view(Some(0))
        .unwrap()
        .actions
        .contains(&Action::PlayCard {
            card: trick.id,
            targets: vec![0]
        }));
    for targets in [vec![], vec![1, 1], vec![1, 2, 0], vec![3], vec![1, 4]] {
        let before = game.view(Some(0)).unwrap();
        assert_eq!(
            game.submit(Command {
                version: game.version,
                seat: 0,
                action: Action::PlayCard {
                    card: trick.id,
                    targets
                }
            }),
            Err(RuleError::InvalidTarget)
        );
        assert_eq!(game.view(Some(0)).unwrap(), before);
    }
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: trick.id,
            targets: vec![1, 2],
        },
    );
    let mut negated = false;
    while let Some(crate::Decision::Cards(choice)) = &game.decision {
        let context = choice.counter.unwrap();
        let seat = choice.seat;
        let play = seat == 1 && context.target == 1 && !negated;
        let action = Action::Select {
            event: choice.event,
            branch: u32::from(play),
            cards: if play { vec![counter.id] } else { vec![] },
            targets: vec![],
        };
        negated |= play;
        submit(&mut game, seat, action);
    }
    assert!(negated);
    assert!(!game.seat(1).unwrap().linked);
    assert!(game.seat(2).unwrap().linked);
    let before = game.seat(0).unwrap().hand.len();
    let drawn = *game.classic.as_ref().unwrap().deck.last().unwrap();
    submit(&mut game, 0, Action::Recast { card: recast.id });
    assert_eq!(game.seat(0).unwrap().hand.len(), before);
    assert!(game.seat(0).unwrap().hand.contains(&drawn));
    assert!(game.discard.contains(&recast));
    assert!(!game
        .events
        .iter()
        .any(|e| matches!(e.kind, EventKind::Link { .. } | EventKind::UseTrick { .. })));
}

#[test]
fn lightning_carries_source_less_thunder_and_physical_card_through_terminal_damage() {
    let mut game = scene(
        &[Role::Lord, Role::Rebel, Role::Loyalist, Role::Renegade],
        2,
    );
    let lightning = card(42, CardKind::Shandian);
    game.seat_mut(2).judges.push(lightning);
    game.seat_mut(2).linked = true;
    game.seat_mut(0).linked = true;
    game.seat_mut(0).hp = 2;
    let top = game.classic.as_ref().unwrap().deck.last().unwrap().id;
    game.classic.as_mut().unwrap().faces.push(CardFace {
        id: top,
        suit: Suit::Spade,
        rank: 5,
    });
    game.scheduler.start(game.trick_judgment(2, lightning));
    game.run(None);
    while let Some(crate::Decision::Cards(choice)) = &game.decision {
        let action = if choice.counter.is_some() {
            Action::Select {
                event: choice.event,
                branch: 0,
                cards: vec![],
                targets: vec![],
            }
        } else {
            assert_eq!(choice.kind, ChoiceKind::Save);
            Action::Choose {
                event: choice.event,
                card: None,
            }
        };
        let seat = choice.seat;
        submit(&mut game, seat, action);
    }
    assert!(game.match_result().is_some());
    assert!(game.seat(2).unwrap().alive);
    assert!(!game.seat(0).unwrap().alive);
    assert_eq!(game.discard.iter().filter(|&&c| c == lightning).count(), 1);
    assert_eq!(
        game.damage_history()
            .iter()
            .map(|r| (r.target, r.amount, r.propagated))
            .collect::<Vec<_>>(),
        [(2, 3, false), (0, 3, true)]
    );
    assert!(game.damage_history().iter().all(|r| r.source.is_none()
        && r.nature == Some(DamageNature::Thunder)
        && r.card_kind == Some(CardKind::Shandian)
        && r.cards == [lightning]));
}

#[test]
fn trusted_content_queues_armor_links_and_cardless_elemental_damage() {
    let mut game = prescribed();
    let mut setup = registration(7, "nested_hit", "setup");
    setup.effect = Arc::new(|ctx, _, _| {
        ctx.link(99, Some(true)).unwrap();
        ctx.link(10, None).unwrap();
        ctx.change_armor(99, 3).unwrap();
        ctx.change_armor(99, -1).unwrap();
        ctx.damage(Some(7), 99, 2, Some(DamageNature::Fire), None, &[])
            .unwrap();
        TriggerStep::Done
    });
    game.register_trigger(setup).unwrap();
    let event = game.subject_timing("setup", Some(7), None, None, 0, &[]);
    game.scheduler
        .start(Program::Dispatch(game.dispatch_for_timing(event)));
    game.run(None);
    let hits = game.damage_history();
    assert_eq!(
        hits.iter()
            .map(|r| (r.target, r.amount, r.absorbed, r.propagated))
            .collect::<Vec<_>>(),
        [(99, 2, 2, false), (10, 2, 0, true)]
    );
    assert_eq!(
        (game.seat(99).unwrap().hp, game.seat(10).unwrap().hp),
        (4, 2)
    );
}

#[test]
fn damage_after_registrations_can_change_the_chain_before_its_priority() {
    let mut game = prescribed();
    for seat in [42, 10, 99] {
        game.seat_mut(seat).linked = true;
    }
    let mut before = registration(42, "release_before_chain", "damageAfter");
    before.role = TriggerRole::Player;
    before.priority = 1;
    before.effect = Arc::new(|ctx, _, _| {
        ctx.link(10, Some(false)).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(before).unwrap();
    let mut later = registration(42, "inspect_after_chain", "damageAfter");
    later.role = TriggerRole::Player;
    later.priority = -1;
    let checked = Arc::new(Mutex::new(Vec::new()));
    let after = checked.clone();
    later.effect = Arc::new(move |ctx, _, _| {
        *after.lock().unwrap() = ctx
            .game()
            .damage_history()
            .iter()
            .map(|hit| hit.target)
            .collect();
        TriggerStep::Done
    });
    game.register_trigger(later).unwrap();
    run_hit(&mut game, None, 42, 1, Some(DamageNature::Thunder));
    assert_eq!(*checked.lock().unwrap(), [42, 99]);
    assert_eq!(
        (game.seat(10).unwrap().hp, game.seat(10).unwrap().linked),
        (4, false)
    );
}

#[test]
fn luoyi_sha_bonus_is_already_in_the_amount_forwarded_to_linked_seats() {
    let mut game = scene(
        &[Role::Lord, Role::Rebel, Role::Loyalist, Role::Renegade],
        0,
    );
    game.classic.as_mut().unwrap().turn = None;
    game.seat_mut(0).hero = crate::classic_data::roster()
        .iter()
        .find(|hero| hero.id == "xuzhu");
    game.run(None);
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    let branch = choice
        .branches
        .iter()
        .find(|b| b.label == "裸衣")
        .unwrap()
        .id;
    submit(
        &mut game,
        0,
        Action::Select {
            event: choice.event,
            branch,
            cards: vec![],
            targets: vec![],
        },
    );
    let sha = card(52, CardKind::Sha);
    game.seat_mut(0).hand.push(sha);
    game.set_sha_nature(sha.id, Some(DamageNature::Thunder))
        .unwrap();
    for target in [1, 2] {
        game.seat_mut(target).linked = true;
    }
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: sha.id,
            targets: vec![1],
        },
    );
    answer_effects(&mut game, &[]);
    assert_eq!(
        game.damage_history()
            .iter()
            .map(|r| (r.target, r.amount))
            .collect::<Vec<_>>(),
        [(1, 2), (2, 2)]
    );
}

#[test]
fn opening_content_reserves_unique_ids_and_gains_private_printed_cards() {
    let physical = Arc::new(Mutex::new(Vec::new()));
    let ids = physical.clone();
    let mut grant = registration(0, "create_extra", "phaseZhunbeiBegin");
    grant.effect = Arc::new(move |ctx, _, _| {
        let seat = ctx.owner();
        assert_eq!(
            ctx.gain_created_card(seat, CardKind::Sha, Suit::Club, 0, None),
            Err(RuleError::InvalidCard)
        );
        assert_eq!(
            ctx.gain_created_card(
                seat,
                CardKind::Tao,
                Suit::Heart,
                9,
                Some(DamageNature::Fire)
            ),
            Err(RuleError::InvalidCard)
        );
        let first = ctx
            .gain_created_card(
                seat,
                CardKind::Sha,
                Suit::Heart,
                4,
                Some(DamageNature::Fire),
            )
            .unwrap();
        let second = ctx
            .gain_created_card(seat, CardKind::Tiesuo, Suit::Club, 10, None)
            .unwrap();
        *ids.lock().unwrap() = vec![first, second];
        TriggerStep::Done
    });
    let gains = Arc::new(Mutex::new(Vec::new()));
    let captured = gains.clone();
    let mut observe = registration(0, "on_new_gain", "gainAfter");
    observe.effect = Arc::new(move |ctx, _, _| {
        captured
            .lock()
            .unwrap()
            .extend(ctx.event().cards.iter().map(|c| c.id));
        TriggerStep::Done
    });
    let game = Game::classic_with_triggers(11, 0, "zhangfei", 8, vec![grant, observe]).unwrap();
    let ids = physical.lock().unwrap();
    assert_eq!(&*gains.lock().unwrap(), &*ids);
    assert_ne!(ids[0], ids[1]);
    assert!(crate::classic_data::standard_deck()
        .iter()
        .all(|c| !ids.contains(&c.id)));
    let own = game.view(Some(0)).unwrap();
    let public = game.view(None).unwrap();
    assert_eq!(own.card_natures.get(&ids[0]), Some(&DamageNature::Fire));
    assert!(ids
        .iter()
        .all(|id| own.card_faces.iter().any(|f| f.id == *id)));
    assert!(public.card_faces.iter().all(|f| !ids.contains(&f.id)));
    assert!(public.card_natures.is_empty());
    assert!(public
        .events
        .iter()
        .all(|e| !matches!(e.kind, EventKind::Gain { card: Some(_), .. })));
    assert_eq!(
        public.deck_count
            + public
                .seats
                .iter()
                .map(|s| s.hand_count + s.equipment.len() + s.judges.len())
                .sum::<usize>()
            + public.discard.len()
            + public.resolving.len(),
        106
    );
}

#[test]
fn cancelled_created_gain_stays_out_of_piles_and_its_id_remains_reserved() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Renegade], 0);
    game.classic.as_mut().unwrap().turn = None;
    // Prescribed extensions may have chosen a large physical ID already.
    game.seat_mut(2)
        .hand
        .push(card(CardId::MAX, CardKind::Shan));
    let reservations = Arc::new(Mutex::new(Vec::new()));
    let seen = reservations.clone();
    let mut create = registration(0, "create_twice", "phaseZhunbeiBegin");
    create.effect = Arc::new(move |ctx, _, _| {
        let rejected = ctx
            .gain_created_card(
                0,
                CardKind::Sha,
                Suit::Spade,
                1,
                Some(DamageNature::Thunder),
            )
            .unwrap();
        let kept = ctx
            .gain_created_card(0, CardKind::Tiesuo, Suit::Club, 10, None)
            .unwrap();
        *seen.lock().unwrap() = vec![rejected, kept];
        TriggerStep::Done
    });
    let mut cancel = registration(0, "counter_gain", "gainBefore");
    cancel.filter = Arc::new(|_, event, _| event.cards.iter().any(|c| c.kind == CardKind::Sha));
    cancel.effect = Arc::new(|_, _, _| TriggerStep::CancelSubject);
    game.register_trigger(create).unwrap();
    game.register_trigger(cancel).unwrap();
    game.run(None);
    let ids = reservations.lock().unwrap();
    assert_ne!(ids[0], ids[1]);
    assert!(!game.physical_cards().any(|c| c.id == ids[0]));
    assert_eq!(game.physical_cards().filter(|c| c.id == ids[1]).count(), 1);
    assert!(!game
        .view(Some(0))
        .unwrap()
        .card_natures
        .contains_key(&ids[0]));
    let next = game
        .create_content_card(CardKind::Tao, Suit::Heart, 1, None)
        .unwrap();
    assert!(!ids.contains(&next.id));
}
