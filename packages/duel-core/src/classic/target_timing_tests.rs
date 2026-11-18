use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    CardKind, CardTargetTiming, EventKind, EventStatus, TriggerOrder, TriggerRegistration,
    TriggerRole, TriggerStep,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[test]
fn eight_seat_borrowed_swords_target_requests_match_reference() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/qiaoshui-targets.json"
    ))
    .unwrap();
    let mut game = scene(
        &[
            Role::Lord,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Rebel,
            Role::Rebel,
            Role::Rebel,
            Role::Rebel,
        ],
        0,
    );
    let material = card(1, CardKind::Jiedao);
    game.seat_mut(0).hand.push(material);
    game.seat_mut(1).equipment.push(card(5, CardKind::Qinglong));
    game.seat_mut(3).equipment.push(card(6, CardKind::Qinglong));
    let mut add = registration(0, "useCard2", TriggerRole::Player);
    add.effect = Arc::new(|ctx, _, _| {
        ctx.add_use_target(1).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(add).unwrap();
    game.register_trigger(registration(0, "useCard", TriggerRole::Player))
        .unwrap();
    let mut cancel = registration(1, "useCardToPlayer", TriggerRole::Target);
    cancel.effect = Arc::new(|ctx, _, _| {
        assert_eq!(ctx.event().player, Some(0));
        assert_eq!(ctx.event().target, Some(1));
        TriggerStep::CancelSubject
    });
    game.register_trigger(cancel).unwrap();

    let observed = Arc::new(Mutex::new(Vec::new()));
    let output = observed.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.player != Some(0) || !event.cards.contains(&material) {
            return;
        }
        let is_target = CardTargetTiming::ALL
            .iter()
            .any(|t| t.name() == event.timing);
        if !is_target && !["loseAfter", "useCard2", "useCard"].contains(&event.timing.as_str()) {
            return;
        }
        if is_target {
            assert_eq!(event.card_kind, Some(CardKind::Jiedao));
            let (frame, program) = game.scheduler.active_frame(event.subject).unwrap();
            assert_eq!(frame.parent, event.parent);
            assert!(
                matches!(program.kind(), EventKind::CardTargetTiming { seat: 0, target, card_kind: CardKind::Jiedao, .. } if Some(target) == event.target)
            );
            let parent = event.parent.unwrap();
            assert!(
                matches!(game.scheduler.active_frame(parent).unwrap().1.kind(), EventKind::UseTrick { card, .. } if card == material)
            );
            assert_eq!(game.use_targets(parent), Some([1, 3].as_slice()));
        }
        assert!(game.resolving.contains(&material));
        output.lock().unwrap().push((
            event.timing.clone(),
            event.target,
            event.subject,
            event.parent,
        ));
    }));

    let flow = game
        .trick_uses(game.seat(0).unwrap(), material)
        .unwrap()
        .into_iter()
        .find(|item| item.targets == [3, 4])
        .unwrap()
        .program;
    game.scheduler.start(flow);
    game.run(None);
    // All four layers run before the extra borrowed-sword victim is requested.
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    assert_eq!(choice.target, Some(1));
    assert_eq!(choice.branches[0].label, "选择被杀目标");
    let rows = observed.lock().unwrap();
    let expected: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["sample"] == "borrowed-swords" && row["kind"] == "request")
        .filter(|row| {
            let timing = row["timing"].as_str().unwrap();
            [
                "loseAfter",
                "useCard2",
                "useCard",
                "useCardToPlayer",
                "useCardToTarget",
                "useCardToPlayered",
                "useCardToTargeted",
            ]
            .contains(&timing)
                && row["material"][0][0] == "jiedao"
        })
        .map(|row| {
            (
                row["timing"].as_str().unwrap().to_string(),
                row["target"]
                    .as_str()
                    .map(|s| u32::from(s.as_bytes()[0] - b'A')),
            )
        })
        .collect();
    let actual: Vec<_> = rows.iter().map(|row| (row.0.clone(), row.1)).collect();
    assert_eq!(actual, expected);
    let root = rows.iter().find(|row| row.0 == "useCard2").unwrap().2;
    let children: Vec<_> = rows
        .iter()
        .filter(|row| row.0.starts_with("useCardTo"))
        .collect();
    assert_eq!(children.len(), 8);
    assert!(children
        .iter()
        .all(|row| row.3 == Some(root) && row.2 != root));
    let unique: std::collections::HashSet<_> = children.iter().map(|row| row.2).collect();
    assert_eq!(unique.len(), 8);
    assert!(game
        .events
        .iter()
        .any(|e| e.id == children[0].2 && e.status == EventStatus::Cancelled));
}

fn registration(owner: u32, timing: &str, role: TriggerRole) -> TriggerRegistration {
    TriggerRegistration {
        owner,
        skill: format!("test_{timing}"),
        timing: timing.into(),
        role,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|_, _, _| TriggerStep::Done),
    }
}

#[test]
fn unobserved_trick_uses_no_target_timing_frames() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist, Role::Rebel], 0);
    let material = card(1, CardKind::Tiesuo);
    game.seat_mut(0).hand.push(material);
    let flow = game
        .trick_uses(game.seat(0).unwrap(), material)
        .unwrap()
        .into_iter()
        .find(|item| item.targets == [1, 2])
        .unwrap()
        .program;
    game.scheduler.start(flow);
    game.run(None);
    crate::classic::tests::pass_counters(&mut game);
    assert!(game.seat(1).unwrap().linked && game.seat(2).unwrap().linked);
    assert!(!game
        .events
        .iter()
        .any(|event| matches!(event.kind, EventKind::CardTargetTiming { .. })));
}

#[test]
fn a_target_killed_by_an_earlier_callback_has_no_later_target_frames() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist, Role::Rebel], 0);
    let material = card(1, CardKind::Tiesuo);
    game.seat_mut(0).hand.push(material);
    let mut trigger = registration(1, "useCardToPlayer", TriggerRole::Target);
    trigger.effect = Arc::new(|ctx, _, _| {
        ctx.lose_hp(2, 4).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(trigger).unwrap();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let output = observed.clone();
    game.timing_observer = Some(Arc::new(move |_, event| {
        if event.timing.starts_with("useCardTo") {
            output
                .lock()
                .unwrap()
                .push((event.timing.clone(), event.target));
        }
    }));
    let flow = game
        .trick_uses(game.seat(0).unwrap(), material)
        .unwrap()
        .into_iter()
        .find(|item| item.targets == [1, 2])
        .unwrap()
        .program;
    game.scheduler.start(flow);
    game.run(None);
    for _ in 0..12 {
        let view = game.view(None).unwrap();
        let Some(seat) = view.waiting_for else {
            break;
        };
        let Some(choice) = game.view(Some(seat)).unwrap().choice else {
            break;
        };
        if choice.kind != crate::ChoiceKind::Save {
            break;
        }
        game.submit(crate::Command {
            version: view.version,
            seat,
            action: crate::Action::Choose {
                event: choice.event,
                card: None,
            },
        })
        .unwrap();
    }
    assert!(!game.seat(2).unwrap().alive);
    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 4);
    assert!(observed.iter().all(|(_, target)| *target == Some(1)));
}
