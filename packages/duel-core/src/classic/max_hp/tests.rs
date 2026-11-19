use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::{Role, Suit};
use crate::{
    Action, ActiveSkillRegistration, CardFace, CardKind, ChoiceBranch, ChoiceKind, Command,
    Decision, Phase, TriggerEvent, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

// These are exactly the hooks sampled by the upstream fixture, plus dying hooks
// so an accidental rescue path cannot silently disappear from the comparison.
const TIMINGS: &[&str] = &[
    "gainMaxHpBefore",
    "gainMaxHpBegin",
    "gainMaxHpEnd",
    "gainMaxHpAfter",
    "gainMaxHpCancelled",
    "loseMaxHpBefore",
    "loseMaxHpBegin",
    "loseMaxHpEnd",
    "loseMaxHpAfter",
    "loseMaxHpCancelled",
    "changeHpBefore",
    "changeHpBegin",
    "changeHp",
    "changeHpEnd",
    "changeHpAfter",
    "changeHpCancelled",
    "dieBefore",
    "dieBegin",
    "die",
    "dieEnd",
    "dieAfter",
    "dieCancelled",
    "dyingBefore",
    "dyingBegin",
    "dying",
    "dyingEnd",
    "dyingAfter",
    "dyingCancelled",
    "damageBefore",
    "damageBegin4",
    "damage",
    "damageEnd",
    "damageAfter",
    "damageCancelled",
    "drawBefore",
    "drawEnd",
    "drawAfter",
];

fn label(seat: SeatId) -> &'static str {
    match seat {
        0 => "A",
        1 => "B",
        2 => "C",
        _ => "other",
    }
}

fn frame_name(game: &Game, event: EventId) -> String {
    match game
        .event_kind(event)
        .expect("observed event frame is active")
    {
        EventKind::GainMaxHp { .. } => "gainMaxHp".into(),
        EventKind::LoseMaxHp { .. } => "loseMaxHp".into(),
        EventKind::ChangeHp { .. } => "changeHp".into(),
        EventKind::Death { .. } => "die".into(),
        EventKind::Dying { .. } => "dying".into(),
        EventKind::Damage { .. } => "damage".into(),
        EventKind::Draw { .. } => "draw".into(),
        EventKind::ShaEffect { .. } => "sha".into(),
        EventKind::ActiveSkillEffect { .. } | EventKind::ActivateSkill { .. } => "fixture".into(),
        EventKind::RegisteredTrigger { registration, .. } => game
            .trigger_registrations
            .get(&registration)
            .expect("content registration remains active")
            .skill
            .clone(),
        other => panic!("unexpected reference frame {other:?}"),
    }
}

// The upstream finished flag has no common live-frame API. It is intentionally
// omitted here; cancellation and the other common fields remain compared.
fn snapshot(game: &Game, event: &TriggerEvent, trial: &str, observation: &str) -> Value {
    let kind = game.event_kind(event.subject).expect("live subject");
    let subject = frame_name(game, event.subject);
    let parent = event.parent.map(|id| frame_name(game, id));
    let player = game.seat(1).unwrap();
    let change = game.max_hp_change(event.subject);
    if let Some(change) = change {
        assert_eq!(change.event, event.subject);
        assert_eq!(change.seat, 1);
        assert_eq!(change.source, event.source);
    }
    let applied_hp = !matches!(event.timing.as_str(), "changeHpBefore" | "changeHpBegin");
    let number = match kind {
        EventKind::GainMaxHp { .. } | EventKind::LoseMaxHp { .. } => {
            change.map(|c| i64::from(c.num))
        }
        EventKind::ChangeHp { .. } => event.delta,
        EventKind::Damage { .. } | EventKind::Draw { .. } => Some(i64::from(event.amount)),
        _ => None,
    };
    let (original_hp, original_max, changed_hp, changed_max, lose_hp) = if let Some(change) = change
    {
        if let Some(metrics) = &change.metrics {
            assert_eq!(
                i64::from(metrics.max_hp_after),
                i64::from(metrics.original_max_hp) + metrics.changed_max_hp
            );
            (
                Some(metrics.original_hp),
                Some(metrics.original_max_hp),
                Some(metrics.changed_hp),
                Some(metrics.changed_max_hp),
                metrics.lose_hp,
            )
        } else {
            (None, None, None, None, None)
        }
    } else if let EventKind::ChangeHp { before, .. } = kind {
        // changeHp stores its raw original HP at creation. Its other sampled
        // fields become available when its body applies the change.
        (
            Some(before),
            applied_hp.then_some(player.max_hp),
            applied_hp.then_some(player.hp.max(0) - before.max(0)),
            applied_hp.then_some(0),
            None,
        )
    } else {
        (None, None, None, None, None)
    };
    let cancelled = change.map_or(event.timing.ends_with("Cancelled"), |c| c.cancelled);
    let reason = matches!(kind, EventKind::Death { .. })
        .then(|| event.parent.and_then(|id| game.max_hp_change(id)))
        .flatten();
    let reason_name = reason.map(|_| "loseMaxHp");
    let reason_source = reason.and_then(|c| c.source).map(label);
    json!([
        trial,
        observation,
        event.timing,
        subject,
        parent,
        event.player.map(label),
        event.source.map(label),
        number,
        player.hp,
        player.max_hp,
        player.alive,
        player.hand.len(),
        original_hp,
        original_max,
        changed_hp,
        changed_max,
        lose_hp,
        cancelled,
        reason_name,
        reason_source
    ])
}

fn expected_row(row: &Value) -> Value {
    let mut values = row.as_array().unwrap().clone();
    if values[4] == "reference_fixture_use" {
        values[4] = json!("fixture");
    }
    // The two upstream death cases use non-lord C/D, while each independent
    // Rust scene uses non-lord B and gives it the matching role.
    if values[5] == "C" || values[5] == "D" {
        values[5] = json!("B");
    }
    values.remove(17); // Upstream finished, not exposed on the live Rust frame.
    Value::Array(values)
}

fn registration(owner: SeatId, skill: &str, timing: &str) -> TriggerRegistration {
    TriggerRegistration {
        owner,
        skill: skill.into(),
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

fn install_observers(game: &mut Game, trial: &str) -> Arc<Mutex<Vec<Value>>> {
    let rows = Arc::new(Mutex::new(Vec::new()));
    let requests = rows.clone();
    let sample = trial.to_owned();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.player == Some(1) && TIMINGS.contains(&event.timing.as_str()) {
            requests
                .lock()
                .unwrap()
                .push(snapshot(game, event, &sample, "request"));
        }
    }));
    for timing in TIMINGS {
        let mut probe = registration(2, &format!("max_hp_probe_{timing}"), timing);
        probe.order = TriggerOrder::First;
        probe.filter = Arc::new(|_, event, _| event.player == Some(1));
        let callbacks = rows.clone();
        let sample = trial.to_owned();
        probe.effect = Arc::new(move |ctx, _, _| {
            callbacks
                .lock()
                .unwrap()
                .push(snapshot(ctx.game(), ctx.event(), &sample, "callback"));
            TriggerStep::Done
        });
        game.register_trigger(probe).unwrap();
    }
    rows
}

fn start_play(game: &mut Game) {
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
}

fn submit(game: &mut Game, seat: SeatId, action: Action) {
    game.submit(Command {
        version: game.version,
        seat,
        action,
    })
    .unwrap();
}

fn default_choice() -> ChoiceBranch {
    ChoiceBranch {
        id: 0,
        label: "max hp fixture".into(),
        cards: vec![],
        min_cards: 0,
        max_cards: 0,
        targets: vec![],
        min_targets: 0,
        max_targets: 0,
    }
}

fn register_cancellation(game: &mut Game, timing: &str) {
    let mut stop = registration(2, "max_hp_cancel", timing);
    stop.filter = Arc::new(|_, event, _| event.player == Some(1));
    stop.effect = Arc::new(|_, _, _| TriggerStep::CancelSubject);
    game.register_trigger(stop).unwrap();
}

fn setup_draws(game: &mut Game, sample: &str) {
    let specifications: &[(u32, u8)] = if sample == "weizhong-tied-min" {
        &[(109, 7), (108, 8)]
    } else {
        &[(109, 9)]
    };
    let classic = game.classic.as_mut().unwrap();
    for &(id, rank) in specifications {
        *classic.deck.iter_mut().find(|item| item.id == id).unwrap() = card(id, CardKind::Tao);
        classic.faces.push(CardFace {
            id,
            suit: Suit::Heart,
            rank,
        });
    }
}

fn register_red_sha_branch(game: &mut Game) {
    // This fixture exercises the real red-Sha branch of 恃勇. The separate
    // 酒杀 branch is outside the currently supported card set.
    let mut skill = registration(1, "shiyong", "damageEnd");
    skill.role = TriggerRole::Player;
    skill.filter = Arc::new(|game, event, _| {
        event.card_kind == Some(CardKind::Sha)
            && event.cards.iter().any(|card| {
                game.face(card.id)
                    .is_some_and(|face| matches!(face.suit, Suit::Heart | Suit::Diamond))
            })
    });
    skill.effect = Arc::new(|ctx, _, _| {
        ctx.lose_max_hp(1, 1, None).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(skill).unwrap();
}

#[test]
fn registered_content_matches_all_sampled_upstream_max_hp_observations() {
    let golden: Value =
        serde_json::from_str(include_str!("../../../../../tests/reference/max-hp.json")).unwrap();
    let all_rows = golden["events"].as_array().unwrap();
    let mut compared = 0;
    let mut direct = 0;
    let mut weizhong = 0;
    let mut shiyong = 0;
    for trial in golden["trials"].as_array().unwrap() {
        let name = trial["name"].as_str().unwrap();
        let skill_sample = name == "shiyong-red-sha";
        let weizhong_sample = name.starts_with("weizhong-");
        let role = if name == "death-negative-source" {
            Role::Rebel
        } else {
            Role::Loyalist
        };
        let mut game = scene(
            &[
                Role::Lord,
                role,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Renegade,
            ],
            0,
        );
        let before = trial["before"].as_array().unwrap();
        game.seat_mut(1).hp = before[0].as_i64().unwrap() as i32;
        game.seat_mut(1).max_hp = before[1].as_i64().unwrap() as i32;
        if name == "weizhong-not-min" {
            game.seat_mut(1)
                .hand
                .extend([card(201, CardKind::Tao), card(202, CardKind::Tao)]);
        }
        if weizhong_sample {
            setup_draws(&mut game, name);
            for skill in crate::vitality_content::weizhong(1) {
                game.register_trigger(skill).unwrap();
            }
        }
        if skill_sample {
            let sha = card(80, CardKind::Sha);
            game.seat_mut(0).hand.push(sha);
            game.classic.as_mut().unwrap().faces.push(CardFace {
                id: 80,
                suit: Suit::Heart,
                rank: 4,
            });
            register_red_sha_branch(&mut game);
        }
        let rows = install_observers(&mut game, name);
        if let Some(timing) = trial["cancelAt"].as_str() {
            register_cancellation(&mut game, timing);
        }
        let active = if skill_sample {
            None
        } else {
            let gain = trial["method"] == "gainMaxHp";
            let num = trial["requested"].as_i64().unwrap() as i32;
            let source = (trial["event"]["source"] == "A").then_some(0);
            Some(
                game.register_active_skill(ActiveSkillRegistration {
                    owner: 0,
                    skill: "max_hp_fixture".into(),
                    limited: false,
                    choice: Arc::new(|_, _| Some(default_choice())),
                    effect: Arc::new(move |ctx, _, _| {
                        if gain {
                            ctx.gain_max_hp(1, num, source).unwrap();
                        } else {
                            ctx.lose_max_hp(1, num, source).unwrap();
                        }
                        TriggerStep::Done
                    }),
                })
                .unwrap(),
            )
        };
        start_play(&mut game);
        if let Some(registration) = active {
            submit(
                &mut game,
                0,
                Action::ActivateSkill {
                    registration,
                    cards: vec![],
                    targets: vec![],
                },
            );
        } else {
            submit(
                &mut game,
                0,
                Action::PlayCard {
                    card: 80,
                    targets: vec![1],
                },
            );
            while let Some(Decision::Cards(choice)) = &game.decision {
                assert_eq!(choice.kind, ChoiceKind::Shan);
                let (seat, event) = (choice.seat, choice.event);
                submit(&mut game, seat, Action::Choose { event, card: None });
            }
            assert!(game.discard.iter().any(|card| card.id == 80));
        }
        let player = game.seat(1).unwrap();
        assert_eq!(
            json!([player.hp, player.max_hp, player.alive, player.hand.len()]),
            trial["after"],
            "{name}: final state"
        );
        if weizhong_sample {
            let drawn: Vec<_> = player
                .hand
                .iter()
                .filter(|card| card.id < 200)
                .map(|card| {
                    let face = game.face(card.id).unwrap();
                    json!(["tao", face.suit, face.rank])
                })
                .collect();
            assert_eq!(json!(drawn), trial["drawn"], "{name}: physical draws");
        }
        let expected: Vec<_> = all_rows
            .iter()
            .filter(|row| row[0] == name && (row[1] == "request" || row[1] == "callback"))
            .map(expected_row)
            .collect();
        let actual = rows.lock().unwrap();
        assert_eq!(
            actual.len(),
            expected.len(),
            "{name}: observed row count\nactual={actual:#?}\nexpected={expected:#?}"
        );
        for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
            assert_eq!(actual, expected, "{name}: observation {index}");
        }
        compared += expected.len();
        if skill_sample {
            shiyong += expected.len();
        } else if weizhong_sample {
            weizhong += expected.len();
        } else {
            direct += expected.len();
        }
    }
    assert_eq!((direct, weizhong, shiyong, compared), (164, 28, 28, 220));
}
