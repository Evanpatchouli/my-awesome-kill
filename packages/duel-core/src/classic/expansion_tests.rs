use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::scheduler::Work;
use crate::{
    CardKind, EventKind, Game, Program, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};

fn pump(game: &mut Game, program: Program) {
    game.scheduler.start(program);
    while let Some(work) = game.scheduler.poll() {
        if let Work::Run(event, program, child) = work {
            game.resolve(event, program, child, &mut None);
            assert!(
                game.decision.is_none(),
                "test callbacks do not request choices"
            );
        }
    }
}

fn reg(timing: &str) -> TriggerRegistration {
    TriggerRegistration {
        owner: 0,
        skill: format!("probe_{timing}"),
        timing: timing.into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|_, _, _| TriggerStep::Done),
    }
}

fn table() -> Game {
    scene(&[Role::Lord, Role::Rebel, Role::Loyalist, Role::Rebel], 0)
}

#[test]
fn expansion_and_cleanup_match_reference_zones_and_callbacks() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/zhongyi-expansion.json"
    ))
    .unwrap();
    let material = card(1, CardKind::Tao);
    let mut game = table();
    game.seat_mut(0).hand.push(material);
    let rows = Arc::new(Mutex::new(Vec::new()));
    let snapshot = move |game: &Game, timing: &str, point: &str| {
        let zone = if game.seat(0).unwrap().hand.contains(&material) {
            "h"
        } else if game.transferring.contains(&material) {
            "s"
        } else if game.seat(0).unwrap().expansions.contains(&material) {
            "x"
        } else if game.discard.contains(&material) {
            "d"
        } else {
            "?"
        };
        (
            timing.to_string(),
            point.to_string(),
            zone.to_string(),
            game.expansion_tags(material.id)
                .iter()
                .any(|t| t == "zhongyi2"),
        )
    };
    for timing in [
        "addToExpansionBefore",
        "loseAfter",
        "addToExpansionAfter",
        "loseToDiscardpileBegin",
        "loseToDiscardpileAfter",
    ] {
        let out = rows.clone();
        let mut registration = reg(timing);
        registration.effect = Arc::new(move |ctx, _, _| {
            out.lock()
                .unwrap()
                .push(snapshot(ctx.game(), &ctx.event().timing, "effect"));
            TriggerStep::Done
        });
        game.register_trigger(registration).unwrap();
    }
    let out = rows.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if [
            "addToExpansionBefore",
            "loseAfter",
            "addToExpansionAfter",
            "loseToDiscardpileBegin",
            "loseToDiscardpileAfter",
        ]
        .contains(&event.timing.as_str())
        {
            assert_eq!(event.player, Some(0));
            let (frame, program) = game.scheduler.active_frame(event.subject).unwrap();
            assert_eq!(frame.parent, event.parent);
            match event.timing.as_str() {
                "addToExpansionBefore" | "addToExpansionAfter" => {
                    assert!(matches!(
                        program.kind(),
                        EventKind::AddToExpansion { seat: 0, .. }
                    ));
                    assert_eq!(event.source, Some(0));
                }
                "loseAfter" => {
                    assert!(matches!(program.kind(), EventKind::Lose { seat: 0, .. }));
                    let parent = game
                        .scheduler
                        .active_frame(event.parent.unwrap())
                        .unwrap()
                        .1
                        .kind();
                    assert!(matches!(
                        parent,
                        EventKind::AddToExpansion { .. } | EventKind::LoseToDiscardpile { .. }
                    ));
                    assert_eq!(event.source, None);
                }
                _ => assert!(matches!(
                    program.kind(),
                    EventKind::LoseToDiscardpile { seat: 0, .. }
                )),
            }
            out.lock()
                .unwrap()
                .push(snapshot(game, &event.timing, "request"));
        }
    }));
    let add = game
        .expansion_program(0, material, "zhongyi2".into())
        .unwrap();
    pump(&mut game, add);
    assert_eq!(game.expansions(0, "zhongyi2"), [material]);
    let cleanup = game.discard_expansions_program(0, "zhongyi2").unwrap();
    pump(&mut game, cleanup);
    let expected: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| ["activate", "between"].contains(&r[1].as_str().unwrap()))
        .filter(|r| {
            [
                "addToExpansionBefore",
                "loseAfter",
                "addToExpansionAfter",
                "loseToDiscardpileBegin",
                "loseToDiscardpileAfter",
            ]
            .contains(&r[3].as_str().unwrap())
        })
        .map(|r| {
            (
                r[3].as_str().unwrap().to_string(),
                r[2].as_str().unwrap().to_string(),
                r[9].as_str().unwrap().to_string(),
                r[10].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(*rows.lock().unwrap(), expected);
    assert_eq!(game.discard.iter().filter(|c| **c == material).count(), 1);
    assert!(game.discard_expansions_program(0, "zhongyi2").is_none());
}

#[test]
fn cancelled_add_before_is_private_but_cancelled_loss_stays_public() {
    for timing in ["addToExpansionBefore", "loseBefore"] {
        let mut game = table();
        let material = card(1, CardKind::Tao);
        game.seat_mut(0).hand.push(material);
        let mut cancel = reg(timing);
        cancel.effect = Arc::new(|_, _, _| TriggerStep::CancelSubject);
        game.register_trigger(cancel).unwrap();
        let add = game.expansion_program(0, material, "tag".into()).unwrap();
        pump(&mut game, add);
        assert!(game.seat(0).unwrap().hand.contains(&material));
        assert_eq!(
            game.known_to_all.contains(&material.id),
            timing == "loseBefore"
        );
        assert!(game.transferring.is_empty() && game.expansions(0, "tag").is_empty());
    }
}

#[test]
fn expansion_gain_clears_tag_and_dead_owner_cleanup_is_idempotent() {
    let mut game = table();
    let first = card(1, CardKind::Tao);
    let second = card(2, CardKind::Sha);
    game.seat_mut(0).hand.extend([first, second]);
    for card in [first, second] {
        let add = game.expansion_program(0, card, "tag".into()).unwrap();
        pump(&mut game, add);
    }
    let stale_cleanup = game.discard_all_expansions_program(0).unwrap();
    let gain = game.gain_expansion_program(0, 1, first).unwrap();
    pump(&mut game, gain);
    assert!(game.seat(1).unwrap().hand.contains(&first));
    assert!(game.known_to_all.contains(&first.id));
    assert!(game.expansion_tags(first.id).is_empty());
    game.seat_mut(0).alive = false;
    let attempted = game.discard_expansions_program(0, "tag").unwrap();
    pump(&mut game, attempted);
    assert_eq!(game.expansions(0, "tag"), [second]);
    assert!(!game.discard.contains(&second));
    pump(&mut game, stale_cleanup);
    assert_eq!(game.discard.iter().filter(|c| **c == second).count(), 1);
    assert!(!game.discard.contains(&first));
    assert!(game.expansion_tags(second.id).is_empty());
    assert!(game.discard_all_expansions_program(0).is_none());
}

#[test]
fn cleanup_revalidates_batch_after_interception_and_reports_actual_cards() {
    let mut game = table();
    let first = card(1, CardKind::Tao);
    let second = card(2, CardKind::Sha);
    game.seat_mut(0).hand.extend([first, second]);
    for card in [first, second] {
        let add = game.expansion_program(0, card, "tag".into()).unwrap();
        pump(&mut game, add);
    }
    let mut intercept = reg("loseBefore");
    intercept.filter = Arc::new(|_, event, _| event.cards.len() == 2);
    intercept.effect = Arc::new(move |ctx, _, _| {
        ctx.gain_expansion(0, 1, first.id).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(intercept).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let out = seen.clone();
    game.timing_observer = Some(Arc::new(move |_, event| {
        if event.timing == "loseToDiscardpileAfter" {
            out.lock().unwrap().push(event.cards.clone());
        }
    }));
    let cleanup = game.discard_expansions_program(0, "tag").unwrap();
    pump(&mut game, cleanup);
    assert!(game.seat(1).unwrap().hand.contains(&first));
    assert!(game.discard.contains(&second));
    assert!(!game.discard.contains(&first));
    assert_eq!(*seen.lock().unwrap(), [vec![second]]);
    assert!(game.transferring.is_empty());
}
