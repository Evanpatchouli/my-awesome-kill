use super::*;
use crate::classic::tests::scene;
use crate::classic_data::Role;
use crate::TriggerEvent;
use crate::{
    Action, ActiveSkillRegistration, ChoiceBranch, Command, DamageNature, EventStatus, Phase,
    RestClock, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const TIMINGS: &[&str] = &[
    "rest",
    "damageBefore",
    "damageBegin",
    "damageBegin1",
    "damageBegin2",
    "damageBegin3",
    "damageBegin4",
    "damage",
    "damageSource",
    "damageEnd",
    "damageAfter",
    "damageCancelled",
    "changeHpBefore",
    "changeHpBegin",
    "changeHp",
    "changeHpEnd",
    "changeHpAfter",
    "changeHpCancelled",
];
fn trigger(timing: &str) -> TriggerRegistration {
    TriggerRegistration {
        owner: 2,
        skill: format!("rest_probe_{timing}"),
        timing: timing.into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, event, _| event.player == Some(1)),
        effect: Arc::new(|_, _, _| TriggerStep::Done),
    }
}
fn frame(game: &Game, id: EventId) -> &'static str {
    match game.event_kind(id).unwrap() {
        EventKind::Damage { .. } => "damage",
        EventKind::ChangeHp { .. } => "changeHp",
        EventKind::Rest { .. } => "rest",
        EventKind::RegisteredTrigger { .. } => "interrupt",
        EventKind::ActiveSkillEffect { .. } | EventKind::ActivateSkill { .. } => "fixture",
        other => panic!("unexpected frame {other:?}"),
    }
}
fn sample(game: &Game, event: &TriggerEvent, kind: &str) -> Value {
    let p = game.seat(1).unwrap();
    let clock = game
        .rest_clock(1)
        .map(|clock| json!({"type":"phase", "count":clock.count}));
    json!([
        kind,
        event.timing,
        frame(game, event.subject),
        event.parent.map(|id| frame(game, id)),
        p.hp,
        p.alive,
        game.is_out(1),
        game.is_in(1),
        clock,
        !game.damage_history().is_empty()
    ])
}
#[test]
fn damage_rest_interruptions_leave_health_armor_and_link_unchanged() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/rest-interruptions.json"
    ))
    .unwrap();
    let mut compared = 0;
    for stop in [
        "damageBefore",
        "damageBegin",
        "damageBegin1",
        "damageBegin4",
        "changeHpBefore",
    ] {
        let mut game = scene(
            &[
                Role::Lord,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
            ],
            0,
        );
        game.seat_mut(1).armor = 1;
        game.seat_mut(1).linked = true;
        let rows = Arc::new(Mutex::new(Vec::new()));
        let log = rows.clone();
        game.timing_observer = Some(Arc::new(move |game, e| {
            if e.player == Some(1) && TIMINGS.contains(&e.timing.as_str()) {
                log.lock().unwrap().push(sample(game, e, "request"));
            }
        }));
        for &timing in TIMINGS {
            let mut probe = trigger(timing);
            let log = rows.clone();
            probe.effect = Arc::new(move |ctx, _, _| {
                log.lock()
                    .unwrap()
                    .push(sample(ctx.game(), ctx.event(), "callback"));
                TriggerStep::Done
            });
            game.register_trigger(probe).unwrap();
        }
        let mut rest = trigger(stop);
        rest.skill = "enter_rest".into();
        rest.order = TriggerOrder::Normal;
        rest.effect = Arc::new(|ctx, _, _| {
            ctx.rest(1, RestClock::default()).unwrap();
            TriggerStep::Done
        });
        game.register_trigger(rest).unwrap();
        let id = game
            .register_active_skill(ActiveSkillRegistration {
                owner: 0,
                skill: "deal_fire_damage".into(),
                limited: false,
                choice: Arc::new(|_, _| {
                    Some(ChoiceBranch {
                        id: 0,
                        label: "fixture".into(),
                        cards: vec![],
                        min_cards: 0,
                        max_cards: 0,
                        targets: vec![],
                        min_targets: 0,
                        max_targets: 0,
                    })
                }),
                effect: Arc::new(|ctx, _, _| {
                    ctx.damage(Some(0), 1, 1, Some(DamageNature::Fire), None, &[])
                        .unwrap();
                    TriggerStep::Done
                }),
            })
            .unwrap();
        game.scheduler.start(Program::event(EventKind::Phase {
            seat: 0,
            phase: Phase::Play,
        }));
        game.run(None);
        game.submit(Command {
            version: game.version,
            seat: 0,
            action: Action::ActivateSkill {
                registration: id,
                cards: vec![],
                targets: vec![],
            },
        })
        .unwrap();
        assert!(game.is_resting(1), "{stop}");
        assert_eq!(game.health(1), Some((4, 4)), "{stop}");
        assert_eq!(game.seat(1).unwrap().armor, 1, "{stop}");
        assert!(game.seat(1).unwrap().linked, "{stop}");
        let early = matches!(stop, "damageBefore" | "damageBegin");
        assert_eq!(game.damage_history().len(), usize::from(!early), "{stop}");
        if !early {
            assert_eq!(game.damage_history()[0].absorbed, 0);
        }
        let rows = rows.lock().unwrap();
        let expected: Vec<_> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row[0] == stop)
            .map(|row| {
                let mut row = row.as_array().unwrap().clone();
                row.remove(0); // trial name is selected above
                row.truncate(10); // JS finished and cancelled are not exposed on live frames
                if row[3] == "reference_fixture_use" {
                    row[3] = json!("fixture");
                }
                if row[3] == "reference_rest_interrupt" {
                    row[3] = json!("interrupt");
                }
                Value::Array(row)
            })
            .collect();
        assert_eq!(rows.as_slice(), expected.as_slice(), "{stop}");
        compared += rows.len();
        assert!(
            !game
                .events
                .iter()
                .any(|e| e.status == EventStatus::Cancelled
                    && matches!(
                        e.kind,
                        EventKind::Damage { target: 1, .. } | EventKind::ChangeHp { seat: 1, .. }
                    )),
            "{stop}"
        );
        let damage_finish = game
            .events
            .iter()
            .find(|e| {
                e.status == EventStatus::Finished
                    && matches!(e.kind, EventKind::Damage { target: 1, .. })
            })
            .unwrap();
        assert_eq!(damage_finish.body_skipped, early, "{stop}");
        if !early {
            let hp_finish = game
                .events
                .iter()
                .find(|e| {
                    e.status == EventStatus::Finished
                        && matches!(e.kind, EventKind::ChangeHp { seat: 1, .. })
                })
                .unwrap();
            assert!(hp_finish.body_skipped, "{stop}");
        }
        for e in game
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::ChangeHp { seat: 1, .. }))
        {
            if let EventKind::ChangeHp { before, after, .. } = e.kind {
                assert_eq!((before, after), (4, 4), "{stop}");
            }
        }
    }
    assert_eq!(compared, 110);
}
