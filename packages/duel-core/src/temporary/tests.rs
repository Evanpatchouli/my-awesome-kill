use super::*;
use crate::{
    GameSetup, Program, SeatSetup, TriggerContext, TriggerOrder, TriggerSelection, TriggerStep,
};
use std::sync::Mutex;

fn game() -> Game {
    Game::new(GameSetup {
        active_seat: 0,
        seats: vec![
            SeatSetup {
                id: 0,
                hp: 4,
                hand: vec![],
            },
            SeatSetup {
                id: 1,
                hp: 4,
                hand: vec![],
            },
        ],
    })
    .unwrap()
}
fn event(timing: &str, player: SeatId) -> TriggerEvent {
    TriggerEvent {
        timing: timing.into(),
        subject: 0,
        parent: None,
        player: Some(player),
        source: None,
        target: None,
        amount: 0,
        cards: vec![],
        card_kind: None,
        nature: None,
        delta: None,
    }
}
fn registration(
    name: &str,
    order: TriggerOrder,
    effect: impl Fn(&mut TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) -> TriggerRegistration {
    TriggerRegistration {
        owner: 0,
        skill: name.into(),
        timing: "probe".into(),
        role: TriggerRole::Global,
        order,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(effect),
    }
}
fn expiry(role: TriggerRole, timing: &str) -> Vec<SkillExpiry> {
    vec![SkillExpiry::new(role, timing)]
}

#[test]
fn refresh_and_expiry_preserve_other_skills_and_registrations() {
    let mut game = Game::classic(5, 0, "guanyu", 8).unwrap();
    let native = game.seat(0).unwrap().hero.unwrap().skills[0].id.clone();
    let unrelated = game
        .register_trigger(registration("owned", TriggerOrder::Last, |_, _, _| {
            TriggerStep::Done
        }))
        .unwrap();
    let mut skill = TemporarySkill::new("owned", expiry(TriggerRole::Player, "old"));
    skill
        .triggers
        .push(registration("owned", TriggerOrder::First, |_, _, _| {
            TriggerStep::Done
        }));
    skill.card_enabled = Some(Arc::new(|_, _, kind| kind != CardKind::Juedou));
    game.grant_temporary(0, skill).unwrap();
    let mut refresh = TemporarySkill::new("owned", expiry(TriggerRole::Player, "new"));
    refresh
        .triggers
        .push(registration("owned", TriggerOrder::First, |_, _, _| {
            TriggerStep::Done
        }));
    game.grant_temporary(0, refresh).unwrap();
    assert_eq!(game.temporary_skills.len(), 1);
    assert_eq!(game.temporary_skills[0].triggers.len(), 1);
    assert_eq!(
        game.trigger_registrations
            .values()
            .filter(|r| r.skill == "owned")
            .count(),
        2
    );
    assert!(!game.card_enabled(0, CardKind::Juedou));
    game.begin_timing_request(&event("old", 0));
    game.begin_timing_request(&event("new", 1));
    assert!(game.has_skill(0, "owned"));
    game.begin_timing_request(&event("new", 0));
    assert!(!game.has_skill(0, "owned"));
    assert!(game.has_skill(0, &native));
    assert!(game.card_enabled(0, CardKind::Juedou));
    assert!(game.trigger_registrations.contains_key(&unrelated));
    assert_eq!(
        game.trigger_registrations
            .values()
            .filter(|r| r.skill == "owned")
            .count(),
        1
    );
    assert!(!game.remove_temporary(0, "owned"));
}

#[test]
fn removal_revokes_pending_callback_but_started_callback_continues() {
    let mut game = game();
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut skill = TemporarySkill::new("transient", expiry(TriggerRole::Global, "expiry"));
    let out = log.clone();
    skill.triggers.push(registration(
        "started",
        TriggerOrder::First,
        move |ctx, stage, _| {
            if stage == 0 {
                out.lock().unwrap().push("started");
                assert!(ctx.remove_temporary(0, "transient"));
                TriggerStep::Continue(1)
            } else {
                out.lock().unwrap().push("continued");
                TriggerStep::Done
            }
        },
    ));
    let out = log.clone();
    skill.triggers.push(registration(
        "pending",
        TriggerOrder::Last,
        move |_, _, _| {
            out.lock().unwrap().push("pending");
            TriggerStep::Done
        },
    ));
    game.grant_temporary(0, skill).unwrap();
    game.scheduler.start(Program::Dispatch(
        game.dispatch_for_timing(event("probe", 0)),
    ));
    game.run(None);
    assert_eq!(*log.lock().unwrap(), ["started", "continued"]);
    assert!(!game.has_skill(0, "transient"));
}

#[test]
fn global_expiry_and_next_turn_fallback_need_no_listeners() {
    let mut game = game();
    game.grant_temporary(0, TemporarySkill::until_turn_end("global"))
        .unwrap();
    assert!(game.wants_timing("phaseAfter"));
    game.begin_timing_request(&event("phaseAfter", 1));
    assert!(!game.has_skill(0, "global"));
    game.grant_temporary(0, TemporarySkill::until_turn_end("fallback"))
        .unwrap();
    assert!(game.wants_timing("phaseBeforeStart"));
    game.begin_timing_request(&event("phaseBeforeStart", 1));
    assert!(!game.has_skill(0, "fallback"));
}

#[test]
fn turn_end_expiry_runs_without_other_listeners_on_natural_turn() {
    let mut game = Game::classic(5, 0, "guanyu", 8).unwrap();
    game.grant_temporary(0, TemporarySkill::until_turn_end("alone"))
        .unwrap();
    for _ in 0..100 {
        if !game.has_skill(0, "alone") {
            break;
        }
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.expect("turn awaits a decision");
        let private = game.view(Some(seat)).unwrap();
        let action =
            if private.choice.is_none() && private.actions.contains(&crate::Action::EndPlay) {
                crate::Action::EndPlay
            } else if let Some(choice) = private.choice {
                match choice.kind {
                    crate::ChoiceKind::Discard => crate::Action::Discard {
                        event: choice.event,
                        cards: choice.cards[..choice.min].to_vec(),
                    },
                    crate::ChoiceKind::Shan | crate::ChoiceKind::Save => crate::Action::Choose {
                        event: choice.event,
                        card: None,
                    },
                    crate::ChoiceKind::Content => {
                        let branch = &choice.branches[0];
                        crate::Action::Select {
                            event: choice.event,
                            branch: branch.id,
                            cards: branch.cards[..branch.min_cards].to_vec(),
                            targets: branch.targets[..branch.min_targets].to_vec(),
                        }
                    }
                }
            } else {
                panic!("expected an end-play action or choice");
            };
        game.submit(crate::Command {
            version: view.version,
            seat,
            action,
        })
        .unwrap();
    }
    assert!(!game.has_skill(0, "alone"));
}
