use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::{Role, Suit};
use crate::{
    Action, Command, EventStatus, GameEvent, Phase, TriggerOrder, TriggerRegistration, TriggerRole,
    TriggerStep,
};
use serde_json::{json, Value};
use std::sync::Mutex;

const TIMINGS: &[&str] = &[
    "recastBefore",
    "recastBegin",
    "recastingLose",
    "recastingLost",
    "recast",
    "recastingGain",
    "recastingGained",
    "recastEnd",
    "recastAfter",
    "recastCancelled",
    "loseToDiscardpileBefore",
    "loseToDiscardpileBegin",
    "loseToDiscardpile",
    "loseToDiscardpileEnd",
    "loseToDiscardpileAfter",
    "loseToDiscardpileCancelled",
    "loseBefore",
    "loseBegin",
    "loseEnd",
    "loseAfter",
    "loseCancelled",
    "drawBefore",
    "drawBegin",
    "drawEnd",
    "drawAfter",
    "drawCancelled",
    "gainBefore",
    "gainBegin",
    "gainEnd",
    "gainAfter",
    "gainCancelled",
];

fn event_name(event: EventKind) -> &'static str {
    match event {
        EventKind::Recast { .. } => "recast",
        EventKind::LoseToDiscardpile { .. } => "loseToDiscardpile",
        EventKind::Lose { .. } => "lose",
        EventKind::Gain { .. } => "gain",
        EventKind::Draw { .. } => "draw",
        // The Rust play phase and registered observer are the two fixture entry points.
        EventKind::Phase {
            phase: Phase::Play, ..
        } => "reference_fixture_use",
        EventKind::RegisteredTrigger { .. } => "reference_recast_observe",
        other => panic!("unexpected parent or subject {other:?}"),
    }
}

fn locations(state: RecastState<'_>, card: Card) -> Vec<&'static str> {
    [
        ("A:h", state.seat(0).unwrap().hand),
        ("B:h", state.seat(1).unwrap().hand),
        ("observer:h", state.seat(2).unwrap().hand),
        ("c", state.draw_pile()),
        ("d", state.discard()),
        ("s", state.transferring()),
        ("o", state.resolving()),
    ]
    .into_iter()
    .filter_map(|(label, zone)| zone.contains(&card).then_some(label))
    .collect()
}

#[derive(Clone)]
struct Probe {
    phase: &'static str,
    material: Card,
    replacement: Card,
    parent_events: Vec<GameEvent>,
    actual: Arc<Mutex<Vec<Value>>>,
}

impl Probe {
    fn event<'a>(&'a self, game: &'a Game, id: EventId) -> &'a GameEvent {
        game.events
            .iter()
            .chain(&self.parent_events)
            .find(|event| event.id == id)
            .expect("the callback subject and parent have real scheduler IDs")
    }

    fn record(&self, game: &Game, observation: &str, event: &TriggerEvent) {
        let state = RecastState(game);
        let subject = self.event(game, event.subject);
        let parent = self.event(game, subject.parent.unwrap());
        let material = locations(state, self.material);
        let replacement = locations(state, self.replacement);
        assert!(material.len() <= 1 && replacement.len() <= 1);
        let label = |seat| match seat {
            Some(0) => Some("actor"),
            Some(1) => Some("target"),
            Some(other) => panic!("unexpected seat {other}"),
            None => None,
        };
        let cards: Vec<_> = event
            .cards
            .iter()
            .map(|c| {
                if *c == self.material {
                    "material"
                } else if *c == self.replacement {
                    "replacement"
                } else {
                    panic!("unrelated card {c:?}");
                }
            })
            .collect();
        assert_eq!(
            event.amount as usize,
            if matches!(subject.kind, EventKind::Draw { .. }) {
                1
            } else {
                cards.len()
            }
        );
        let observer = game.view(Some(2)).unwrap();
        let known = observer.seats[0].known_hand.contains(&self.replacement);
        // Inspect all public projections before, during and after physical gain.
        for viewer in [None, Some(1), Some(2)] {
            let view = game.view(viewer).unwrap();
            assert_eq!(view.seats[0].known_hand.contains(&self.replacement), known);
            assert!(view.seats[0].hand.is_none());
            assert_eq!(
                view.card_faces.iter().any(|f| f.id == self.replacement.id),
                known
            );
            assert!(!view.resolving.contains(&self.replacement));
            if !known {
                assert!(!view.events.iter().any(|e| matches!(e.kind, EventKind::Gain { card: Some(c), .. } if c == self.replacement)));
            }
        }
        let actor = game.view(Some(0)).unwrap();
        assert_eq!(
            actor.seats[0]
                .hand
                .as_ref()
                .unwrap()
                .contains(&self.replacement),
            replacement.contains(&"A:h")
        );
        self.actual.lock().unwrap().push(json!([
            self.phase,
            observation,
            event.timing,
            event_name(subject.kind),
            event_name(parent.kind),
            label(event.player),
            label(event.source),
            label(event.target),
            material.first(),
            replacement.first(),
            actor.seats[0].hand_count,
            cards,
            known
        ]));
    }

    fn factory(&self, state: RecastState<'_>, timing: &str, seat: SeatId, card: Card) {
        self.record(
            state.0,
            "factory",
            &TriggerEvent {
                card_kind: None,
                delta: None,
                nature: None,
                parent: None,
                subject: state.0.scheduler.current_id(),
                timing: timing.to_owned(),
                player: Some(seat),
                source: None,
                target: None,
                amount: 1,
                cards: vec![card],
            },
        );
    }
}

fn exercise(phase: &'static str) {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/recast-callbacks.json"
    ))
    .unwrap();
    let mut game = play_scene();
    let material = card(1, CardKind::Tiesuo);
    let control = card(2, CardKind::Sha);
    game.seat_mut(0).hand.extend([material, control]);
    let initial_count = match phase {
        "default" => golden["setup"]["handSize"].as_u64().unwrap(),
        "override" => golden["overrideSetup"]["handSize"].as_u64().unwrap(),
        _ => golden["interrupted"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["phase"] == phase)
            .unwrap()["startingHand"]
            .as_u64()
            .unwrap(),
    };
    let prior_gains = [CardKind::Tao, CardKind::Shan, CardKind::Tao, CardKind::Tao];
    for (index, &kind) in prior_gains
        .iter()
        .take(initial_count as usize - 2)
        .enumerate()
    {
        game.seat_mut(0).hand.push(card(3 + index as u32, kind));
    }
    let is_override = phase == "override";
    let replacement = card(
        109,
        if is_override {
            CardKind::Shan
        } else {
            CardKind::Tao
        },
    );
    let table = game.classic.as_mut().unwrap();
    *table.deck.last_mut().unwrap() = replacement;
    for (card, suit, rank) in [
        (
            material,
            Suit::Club,
            match phase {
                "default" => 10,
                "override" => 11,
                _ => 12,
            },
        ),
        (control, Suit::Spade, 7),
        (
            replacement,
            Suit::Heart,
            match phase {
                "default" => 2,
                "override" => 3,
                _ => 4,
            },
        ),
    ] {
        table.faces.push(CardFace {
            id: card.id,
            suit,
            rank,
        });
    }
    let probe = Probe {
        phase,
        material,
        replacement,
        parent_events: game.events.clone(),
        actual: Arc::default(),
    };
    let requests = probe.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if TIMINGS.contains(&event.timing.as_str()) {
            requests.record(game, "request", event);
        }
    }));
    for &timing in TIMINGS {
        let sample = probe.clone();
        game.register_trigger(TriggerRegistration {
            owner: 2,
            skill: "reference_recast_observe".into(),
            timing: timing.into(),
            role: TriggerRole::Global,
            order: TriggerOrder::Normal,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(move |ctx, _, _| {
                sample.record(ctx.game(), "effect", ctx.event());
                if matches!(
                    (phase, timing),
                    ("cancel-after-loss", "recast")
                        | ("cancel-recasting-lose", "recastingLose")
                        | ("cancel-recasting-gain", "recastingGain")
                        | ("cancel-draw-child", "drawBefore")
                        | ("cancel-loss-child", "loseBefore")
                ) {
                    return TriggerStep::CancelSubject;
                }
                if phase == "material-intercept" && timing == "loseBefore" {
                    let game = ctx.game();
                    let loss = sample.event(game, ctx.event().subject);
                    if matches!(
                        sample.event(game, loss.parent.unwrap()).kind,
                        EventKind::LoseToDiscardpile { .. }
                    ) {
                        ctx.give_hand(0, 1, material.id).unwrap();
                    }
                }
                TriggerStep::Done
            }),
        })
        .unwrap();
    }
    let loss_probe = probe.clone();
    let gain_probe = probe.clone();
    let rule = RecastRule {
        filter: None,
        loss: Arc::new(move |state, seat, card| {
            loss_probe.factory(state, "recastingLose", seat, card);
            assert!(state.seat(seat).unwrap().hand.contains(&card));
            if is_override {
                RecastLoss::Give { to: 1 }
            } else {
                RecastLoss::Discard
            }
        }),
        gain: Arc::new(move |state, seat, card, loss| {
            gain_probe.factory(state, "recastingGain", seat, card);
            assert_eq!(loss.card, material);
            assert_eq!(
                loss.moved,
                !matches!(phase, "material-intercept" | "cancel-loss-child")
            );
            assert_eq!(
                loss.kind,
                if is_override {
                    RecastLoss::Give { to: 1 }
                } else {
                    RecastLoss::Discard
                }
            );
            assert!(state.0.events.iter().any(|e| e.id == loss.event
                && e.status == EventStatus::Finished
                && event_name(e.kind)
                    == if is_override {
                        "gain"
                    } else {
                        "loseToDiscardpile"
                    }));
            if is_override {
                RecastGain::FromDeck {
                    card: replacement.id,
                }
            } else {
                RecastGain::Draw
            }
        }),
    };
    game.register_recast(RecastKey::Card(material.id), rule);
    assert!(game.can_recast(0, material.id));
    assert!(!game.can_recast(0, control.id));
    let before = game.view(Some(0)).unwrap();
    assert!(before
        .actions
        .contains(&Action::Recast { card: material.id }));
    assert!(game
        .submit(Command {
            version: before.version,
            seat: 0,
            action: Action::Recast { card: control.id }
        })
        .is_err());
    assert_eq!(game.view(Some(0)).unwrap(), before);
    game.submit(Command {
        version: before.version,
        seat: 0,
        action: Action::Recast { card: material.id },
    })
    .unwrap();

    let expected: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row[0] == phase)
        .cloned()
        .collect();
    let actual = probe.actual.lock().unwrap();
    for index in 0..actual.len().max(expected.len()) {
        assert_eq!(
            actual.get(index),
            expected.get(index),
            "{phase} observation {index}"
        );
    }
    drop(actual);
    let state = RecastState(&game);
    let material_zone = locations(state, material);
    let replacement_zone = locations(state, replacement);
    assert_eq!(material_zone.len(), 1);
    assert_eq!(replacement_zone.len(), 1);
    assert_eq!(locations(state, control), ["A:h"]);
    assert!(!game.classic.as_ref().unwrap().sha_played);
    assert!(!game.events.iter().any(|e| matches!(
        e.kind,
        EventKind::UseTrick { .. } | EventKind::UseCost { .. } | EventKind::UseVirtual { .. }
    )));
    assert_eq!(game.events.iter().find(|e| matches!(e.kind, EventKind::Recast { .. }) && e.status != EventStatus::Started).unwrap().status,
        if matches!(phase, "cancel-after-loss" | "cancel-recasting-lose" | "cancel-recasting-gain") {
            EventStatus::Cancelled
        } else { EventStatus::Finished });
    match phase {
        "default" => assert_public_draw(&game, EventStatus::Finished, 1),
        "cancel-draw-child" => assert_public_draw(&game, EventStatus::Cancelled, 0),
        _ => {}
    }
    if phase == "cancel-loss-child" {
        assert!(game.events.iter().any(
            |e| matches!(e.kind, EventKind::Lose { .. }) && e.status == EventStatus::Cancelled
        ));
        assert!(game
            .events
            .iter()
            .any(|e| matches!(e.kind, EventKind::LoseToDiscardpile { .. })
                && e.status == EventStatus::Finished));
    }
    if matches!(phase, "default" | "override") {
        let final_state = &golden[if is_override {
            "afterOverride"
        } else {
            "afterDefault"
        }];
        assert_eq!(
            game.seat(0).unwrap().hand.len() as u64,
            final_state["handSize"]
        );
        assert_eq!(
            material_zone,
            if final_state["material"]["targetHand"] == true {
                ["B:h"]
            } else {
                ["d"]
            }
        );
        assert_eq!(replacement_zone, ["A:h"]);
        assert_eq!(
            game.events
                .iter()
                .filter_map(|e| match e.kind {
                    EventKind::Gain { seat: 0, card, .. } if e.status == EventStatus::Finished =>
                        Some(card),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [is_override.then_some(replacement)]
        );
    } else {
        let result = golden["interrupted"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["phase"] == phase)
            .unwrap();
        assert_eq!(before.seats[0].hand_count as u64, result["startingHand"]);
        assert_eq!(
            game.seat(0).unwrap().hand.len() as u64,
            result["endingHand"]
        );
        assert_eq!(material_zone[0], result["material"]);
        assert_eq!(replacement_zone[0], result["replacement"]);
    }
}

fn assert_public_draw(game: &Game, terminal: EventStatus, gained: u16) {
    let draws: Vec<_> = game
        .view(None)
        .unwrap()
        .events
        .into_iter()
        .filter_map(|event| match event.kind {
            EventKind::Draw {
                seat: 0,
                reason: DrawReason::CardEffect,
                amount,
            } => Some((event.id, event.status, amount)),
            _ => None,
        })
        .collect();
    assert_eq!(draws.len(), 2);
    let id = draws[0].0;
    assert_eq!(
        draws,
        [(id, EventStatus::Started, 1), (id, terminal, gained)]
    );
}

#[test]
fn default_reference() {
    exercise("default");
}
#[test]
fn override_reference() {
    exercise("override");
}
#[test]
fn cancel_after_loss_reference() {
    exercise("cancel-after-loss");
}
#[test]
fn material_interception_reference() {
    exercise("material-intercept");
}
#[test]
fn cancelled_recasting_lose_drains_queued_loss() {
    exercise("cancel-recasting-lose");
}
#[test]
fn cancelled_recasting_gain_drains_queued_gain() {
    exercise("cancel-recasting-gain");
}
#[test]
fn cancelled_draw_child_allows_outer_recast_to_finish() {
    exercise("cancel-draw-child");
}
#[test]
fn cancelled_loss_child_keeps_material_and_allows_the_outer_draw() {
    exercise("cancel-loss-child");
}

fn interrupted(timing: &'static str, remove_material: bool) {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist], 0);
    let material = card(1, CardKind::Sha);
    game.seat_mut(0).hand.push(material);
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: crate::Phase::Play,
    }));
    game.run(None);
    let deck_before = game.classic.as_ref().unwrap().deck.clone();
    game.register_recast(RecastKey::Card(material.id), RecastRule::default());
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "interrupt".into(),
        timing: timing.into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |ctx, _, _| {
            if remove_material {
                ctx.discard_hand(0, &[material.id]).unwrap();
                TriggerStep::Done
            } else {
                TriggerStep::CancelSubject
            }
        }),
    })
    .unwrap();
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::Recast { card: material.id },
    })
    .unwrap();
    if remove_material {
        assert_eq!(
            game.classic.as_ref().unwrap().deck.len() + 1,
            deck_before.len()
        );
        assert_eq!(game.seat(0).unwrap().hand, [*deck_before.last().unwrap()]);
    } else {
        assert_eq!(game.classic.as_ref().unwrap().deck, deck_before);
    }
    assert_eq!(game.discard.iter().filter(|c| **c == material).count(), 1);
    assert!(!game.seat(0).unwrap().hand.contains(&material));
}
#[test]
fn intervening_discard_pays_the_material_once() {
    interrupted("loseBefore", true);
}
#[test]
fn cancelled_gain_keeps_paid_loss() {
    interrupted("drawBefore", false);
}

#[test]
fn cancelled_draw_gain_restores_staged_card_to_top() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist], 0);
    let material = card(1, CardKind::Sha);
    game.seat_mut(0).hand.push(material);
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: crate::Phase::Play,
    }));
    game.run(None);
    let deck = game.classic.as_ref().unwrap().deck.clone();
    game.register_recast(RecastKey::Card(material.id), RecastRule::default());
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "cancel-gain".into(),
        timing: "gainBegin".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|_, _, _| TriggerStep::CancelSubject),
    })
    .unwrap();
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::Recast { card: material.id },
    })
    .unwrap();
    assert_eq!(game.classic.as_ref().unwrap().deck, deck);
    assert_eq!(game.discard.iter().filter(|c| **c == material).count(), 1);
    assert!(game.view(None).unwrap().events.iter().any(|event| {
        matches!(event.kind, EventKind::Gain { .. }) && event.status == EventStatus::Cancelled
    }));
    assert_public_draw(&game, EventStatus::Finished, 0);
}

fn play_scene() -> Game {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist], 0);
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: crate::Phase::Play,
    }));
    game.run(None);
    game
}

#[test]
fn recast_validates_permission_phase_actor_version_and_hand() {
    let mut game = play_scene();
    let allowed = card(1, CardKind::Sha);
    let denied = card(2, CardKind::Sha);
    let elsewhere = card(3, CardKind::Sha);
    game.seat_mut(0).hand.extend([allowed, denied]);
    game.seat_mut(1).hand.push(elsewhere);
    game.register_recast(RecastKey::Kind(CardKind::Sha), RecastRule::default());
    let mut denied_rule = RecastRule::default();
    denied_rule.filter = Some(Arc::new(|_, _, _| false));
    game.register_recast(RecastKey::Card(denied.id), denied_rule);
    assert!(game.can_recast(0, allowed.id));
    assert!(!game.can_recast(0, denied.id));
    assert!(!game.can_recast(0, elsewhere.id));
    let before = game.view(Some(0)).unwrap();
    for (version, seat, id) in [
        (before.version, 0, denied.id),
        (before.version, 0, elsewhere.id),
        (before.version, 1, elsewhere.id),
        (before.version + 1, 0, allowed.id),
    ] {
        assert!(game
            .submit(Command {
                version,
                seat,
                action: Action::Recast { card: id }
            })
            .is_err());
        assert_eq!(game.view(Some(0)).unwrap(), before);
    }
    game.decision = None;
    assert!(game
        .submit(Command {
            version: before.version,
            seat: 0,
            action: Action::Recast { card: allowed.id }
        })
        .is_err());
}

#[test]
fn committed_loss_survives_post_callback_cancellation() {
    let mut game = play_scene();
    let material = card(1, CardKind::Sha);
    game.seat_mut(0).hand.push(material);
    game.register_recast(RecastKey::Card(material.id), RecastRule::default());
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "post-cancel".into(),
        timing: "loseEnd".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|_, _, _| TriggerStep::CancelSubject),
    })
    .unwrap();
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::Recast { card: material.id },
    })
    .unwrap();
    assert_eq!(game.discard.iter().filter(|c| **c == material).count(), 1);
    assert_eq!(game.seat(0).unwrap().hand, vec![card(109, CardKind::Sha)]);
}

#[test]
fn transfer_restores_staged_material_when_recipient_dies() {
    let mut game = play_scene();
    let material = card(1, CardKind::Sha);
    game.seat_mut(0).hand.push(material);
    let mut rule = RecastRule::default();
    rule.loss = Arc::new(|_, _, _| RecastLoss::Give { to: 1 });
    game.register_recast(RecastKey::Card(material.id), rule);
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "pause".into(),
        timing: "loseAfter".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|_, stage, _| {
            if stage == 0 {
                TriggerStep::Choose {
                    branches: vec![crate::ChoiceBranch {
                        id: 0,
                        label: "continue".into(),
                        cards: vec![],
                        min_cards: 0,
                        max_cards: 0,
                        targets: vec![],
                        min_targets: 0,
                        max_targets: 0,
                    }],
                    then: 1,
                }
            } else {
                TriggerStep::Done
            }
        }),
    })
    .unwrap();
    let deck = game.classic.as_ref().unwrap().deck.clone();
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::Recast { card: material.id },
    })
    .unwrap();
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    game.seat_mut(1).alive = false;
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::Select {
            event: choice.event,
            branch: 0,
            cards: vec![],
            targets: vec![],
        },
    })
    .unwrap();
    assert_eq!(game.seat(0).unwrap().hand, vec![material]);
    assert!(!game.seat(1).unwrap().hand.contains(&material));
    assert_eq!(game.classic.as_ref().unwrap().deck, deck);
}

#[test]
fn native_lianying_runs_once_after_recast_loss() {
    let mut game = play_scene();
    game.seat_mut(0).hero = crate::classic_data::roster()
        .iter()
        .find(|hero| hero.id == "luxun");
    let material = card(1, CardKind::Sha);
    game.seat_mut(0).hand.push(material);
    game.register_recast(RecastKey::Card(material.id), RecastRule::default());
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::Recast { card: material.id },
    })
    .unwrap();
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::Select {
            event: choice.event,
            branch: 1,
            cards: vec![],
            targets: vec![],
        },
    })
    .unwrap();
    assert_eq!(
        game.seat(0).unwrap().hand,
        vec![card(109, CardKind::Sha), card(108, CardKind::Sha)]
    );
    assert!(game.view(Some(0)).unwrap().choice.is_none());
}

#[test]
fn transfer_revalidates_material_after_gain_callback() {
    let mut game = play_scene();
    let material = card(1, CardKind::Sha);
    game.seat_mut(0).hand.push(material);
    let mut rule = RecastRule::default();
    rule.loss = Arc::new(|_, _, _| RecastLoss::Give { to: 1 });
    game.register_recast(RecastKey::Card(material.id), rule);
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "move-material".into(),
        timing: "gainBegin".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, event, _| event.player == Some(1)),
        effect: Arc::new(move |ctx, _, _| {
            ctx.discard_hand(0, &[material.id]).unwrap();
            TriggerStep::Done
        }),
    })
    .unwrap();
    let deck = game.classic.as_ref().unwrap().deck.clone();
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::Recast { card: material.id },
    })
    .unwrap();
    assert_eq!(
        game.discard
            .iter()
            .filter(|card| **card == material)
            .count(),
        1
    );
    assert!(!game.seat(1).unwrap().hand.contains(&material));
    assert_eq!(game.classic.as_ref().unwrap().deck, deck);
}

#[test]
fn stock_bot_can_end_play_after_recast_is_cancelled() {
    let mut game = play_scene();
    game.classic.as_mut().unwrap().human = 2;
    let material = card(1, CardKind::Shan);
    game.seat_mut(0).hand.push(material);
    game.register_recast(RecastKey::Card(material.id), RecastRule::default());
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "decline".into(),
        timing: "recastBefore".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|_, _, _| TriggerStep::CancelSubject),
    })
    .unwrap();
    let first = game.bot_command().unwrap();
    assert_eq!(first.action, Action::Recast { card: material.id });
    game.submit(first).unwrap();
    assert!(game
        .view(Some(0))
        .unwrap()
        .actions
        .contains(&Action::Recast { card: material.id }));
    let next = game.bot_command().unwrap();
    assert_eq!(next.action, Action::EndPlay);
    game.submit(next).unwrap();
    assert_eq!(game.view(None).unwrap().turn.unwrap().seat, 1);
}

#[test]
fn a_publicly_gained_card_privately_redrawn_after_reshuffle_is_hidden_again() {
    let mut game = play_scene();
    let first = card(1, CardKind::Guohe);
    let second = card(2, CardKind::Guohe);
    let replacement = card(109, CardKind::Tao);
    game.classic.as_mut().unwrap().deck = vec![replacement];
    game.seat_mut(0).hand.extend([first, second]);
    let give: RecastLossFactory = Arc::new(|_, _, _| RecastLoss::Give { to: 1 });
    game.register_recast(
        RecastKey::Card(first.id),
        RecastRule {
            loss: give.clone(),
            gain: Arc::new(move |_, _, _, _| RecastGain::FromDeck {
                card: replacement.id,
            }),
            ..Default::default()
        },
    );
    game.register_recast(
        RecastKey::Card(second.id),
        RecastRule {
            loss: give,
            ..Default::default()
        },
    );
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "discard replacement".into(),
        timing: "recastBefore".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(move |_, e, _| e.cards == [second]),
        effect: Arc::new(move |ctx, _, _| {
            assert!(ctx.game().view(Some(2)).unwrap().seats[0]
                .known_hand
                .contains(&replacement));
            ctx.discard_hand(0, &[replacement.id]).unwrap();
            TriggerStep::Done
        }),
    })
    .unwrap();
    for material in [first, second] {
        game.submit(Command {
            version: game.version,
            seat: 0,
            action: Action::Recast { card: material.id },
        })
        .unwrap();
    }
    assert_eq!(
        game.view(Some(0)).unwrap().seats[0].hand.as_deref(),
        Some([replacement].as_slice())
    );
    let observer = game.view(Some(2)).unwrap();
    assert!(observer.seats[0].hand.is_none());
    assert!(observer.seats[0].known_hand.is_empty());
    assert!(!observer
        .events
        .iter()
        .any(|e| matches!(e.kind, EventKind::Gain { card: Some(c), .. } if c == replacement)));
}
