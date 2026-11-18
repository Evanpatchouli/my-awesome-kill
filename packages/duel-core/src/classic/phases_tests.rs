use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::scheduler::Work;
use crate::{CardKind, Decision, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep};
use std::sync::{Arc, Mutex};

fn table() -> Game {
    scene(&[Role::Lord, Role::Rebel, Role::Loyalist, Role::Rebel], 0)
}
fn pump(game: &mut Game, mut answer: Option<Answer>) {
    while let Some(work) = game.scheduler.poll() {
        if let Work::Run(event, program, child) = work {
            game.resolve(event, program, child, &mut answer);
        }
        if game.decision.is_some() {
            break;
        }
    }
}
fn start(game: &mut Game, phase: Phase, reason: Option<&str>, index: usize) {
    game.scheduler.start(Program::Phase(Flow::new(
        0,
        phase,
        reason.map(str::to_owned),
        index,
    )));
    pump(game, None);
}
fn registration(timing: &str) -> TriggerRegistration {
    TriggerRegistration {
        owner: 0,
        skill: timing.into(),
        timing: timing.into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|_, _, _| TriggerStep::Done),
    }
}

#[test]
fn skip_and_cancel_have_distinct_timings_and_metadata() {
    for skipped in [true, false] {
        let mut game = table();
        if skipped {
            game.skip_next_phase(0, Phase::Play).unwrap();
            game.skip_next_phase(0, Phase::Play).unwrap();
        } else {
            let mut cancel = registration("phaseUseBefore");
            cancel.effect = Arc::new(|_, _, _| TriggerStep::CancelSubject);
            game.register_trigger(cancel).unwrap();
        }
        let finished = Arc::new(Mutex::new(Vec::new()));
        let callback = finished.clone();
        let mut probe = registration(if skipped {
            "phaseUseSkipped"
        } else {
            "phaseUseCancelled"
        });
        probe.effect = Arc::new(move |ctx, _, _| {
            callback
                .lock()
                .unwrap()
                .push(ctx.game().phase_info(ctx.event().subject).unwrap().finished);
            TriggerStep::Done
        });
        game.register_trigger(probe).unwrap();
        let rows = Arc::new(Mutex::new(Vec::new()));
        let out = rows.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if event.timing.starts_with("phaseUse") {
                out.lock().unwrap().push((
                    event.timing.clone(),
                    event.subject,
                    event.parent,
                    game.phase_info(event.subject).unwrap(),
                ));
            }
        }));
        start(&mut game, Phase::Play, Some("test"), 2);
        assert!(game.decision.is_none());
        let rows = rows.lock().unwrap();
        let names: Vec<_> = rows.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(
            names,
            if skipped {
                vec!["phaseUseSkipped"]
            } else {
                vec!["phaseUseBefore", "phaseUseCancelled"]
            }
        );
        assert!(rows.iter().all(|row| row.1 == rows[0].1
            && row.2.is_none()
            && row.3.reason.as_deref() == Some("test")
            && row.3.index == 2));
        assert_eq!(rows.last().unwrap().3.skipped, skipped);
        assert_eq!(rows.last().unwrap().3.cancelled, !skipped);
        assert_eq!(rows.last().unwrap().3.finished, skipped);
        assert_eq!(*finished.lock().unwrap(), [true]);
    }
}

#[test]
fn consecutive_play_phases_each_allow_a_sha() {
    let mut game = table();
    let first = card(1, CardKind::Sha);
    let second = card(2, CardKind::Sha);
    game.seat_mut(0).hand.extend([first, second]);
    for (index, card) in [first, second].into_iter().enumerate() {
        start(
            &mut game,
            Phase::Play,
            (index == 0).then_some("extra"),
            index,
        );
        assert!(matches!(game.decision.take(), Some(Decision::Play(0))));
        assert!(game
            .card_uses(game.seat(0).unwrap(), card)
            .is_some_and(|uses| uses.iter().any(|u| u.targets == [1])));
        let attack = game.attack_program(0, vec![card], vec![1]);
        pump(&mut game, Some(Answer::Play(attack)));
        while matches!(game.decision, Some(Decision::Cards(_))) {
            game.decision = None;
            pump(&mut game, Some(Answer::Pass));
        }
        assert!(matches!(game.decision.take(), Some(Decision::Play(0))));
        assert!(game.classic.as_ref().unwrap().sha_played);
        pump(&mut game, Some(Answer::EndPlay));
    }
    assert_eq!(game.seat(1).unwrap().hp, 2);
}

#[test]
fn normal_phase_requests_follow_upstream_natural_turn_trace() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/phase-scheduling.json"
    ))
    .unwrap();
    for (phase, name, index) in [
        (Phase::Preparation, "phaseZhunbei", 1),
        (Phase::Judgment, "phaseJudge", 2),
        (Phase::Draw, "phaseDraw", 3),
        (Phase::Play, "phaseUse", 4),
        (Phase::Discard, "phaseDiscard", 5),
        (Phase::End, "phaseJieshu", 6),
    ] {
        let expected: Vec<_> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                (r[0] == "request" || r[0] == "callback")
                    && r[2] == name
                    && r[5] == "A1"
                    && r[9][0] == index
            })
            .map(|r| {
                (
                    r[1].as_str().unwrap().to_owned(),
                    r[0].as_str().unwrap().to_owned(),
                    r[9][4] == 1,
                )
            })
            .collect();
        let mut game = table();
        if phase == Phase::Discard {
            game.seat_mut(0).hand = (1..=5).map(|id| card(id, CardKind::Tao)).collect();
        }
        let rows = Arc::new(Mutex::new(Vec::new()));
        let names: std::collections::BTreeSet<_> =
            expected.iter().map(|row| row.0.as_str()).collect();
        for timing in names {
            let out = rows.clone();
            let mut probe = registration(timing);
            probe.effect = Arc::new(move |ctx, _, _| {
                let event = ctx.event();
                let info = ctx.game().phase_info(event.subject).unwrap();
                out.lock().unwrap().push((
                    event.timing.clone(),
                    event.subject,
                    event.parent,
                    info.finished,
                    "callback",
                ));
                TriggerStep::Done
            });
            game.register_trigger(probe).unwrap();
        }
        let out = rows.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if let Some(info) = game.phase_info(event.subject) {
                out.lock().unwrap().push((
                    event.timing.clone(),
                    event.subject,
                    event.parent,
                    info.finished,
                    "request",
                ));
            }
        }));
        start(&mut game, phase, None, index as usize);
        match game.decision.take() {
            Some(Decision::Play(0)) => pump(&mut game, Some(Answer::EndPlay)),
            Some(Decision::Cards(choice)) => pump(
                &mut game,
                Some(Answer::Discard(choice.cards[..choice.min].to_vec())),
            ),
            None => {}
            _ => panic!("unexpected phase decision"),
        }
        let rows = rows.lock().unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| (r.0.clone(), r.4.to_owned(), r.3))
                .collect::<Vec<_>>(),
            expected,
            "{phase:?}"
        );
        assert!(rows.iter().all(|r| r.1 == rows[0].1 && r.2.is_none()));
    }
}

#[test]
fn keji_considers_skipped_play_even_after_sha_in_an_earlier_play() {
    for skipped in [false, true] {
        let mut game = table();
        game.seat_mut(0).hero = crate::classic_data::roster()
            .iter()
            .find(|hero| hero.id == "lvmeng");
        game.note_hero_sha_play(0);
        game.seat_mut(0).hand = (1..=5).map(|id| card(id, CardKind::Tao)).collect();
        if skipped {
            game.skip_next_phase(0, Phase::Play).unwrap();
            start(&mut game, Phase::Play, None, 3);
            assert!(game.decision.is_none());
        }
        start(&mut game, Phase::Discard, None, 4);
        let Some(Decision::Cards(choice)) = &game.decision else {
            panic!("expected discard or keji choice")
        };
        assert_eq!(
            choice.branches.iter().any(|branch| branch.label == "克己"),
            skipped
        );
    }
}

#[test]
fn natural_turns_continue_after_cancelled_play_and_skipped_draw() {
    let mut game = table();
    game.classic.as_mut().unwrap().turn = None;
    game.skip_next_phase(1, Phase::Draw).unwrap();
    let mut cancel = registration("phaseUseBefore");
    cancel.effect = Arc::new(|_, _, _| TriggerStep::CancelSubject);
    game.register_trigger(cancel).unwrap();
    let rows = Arc::new(Mutex::new(Vec::new()));
    let out = rows.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if [
            "phaseUseBefore",
            "phaseUseCancelled",
            "phaseUseBegin",
            "phaseDiscardBegin",
            "phaseDrawSkipped",
        ]
        .contains(&event.timing.as_str())
        {
            if let Some(info) = game.phase_info(event.subject) {
                out.lock().unwrap().push((
                    event.timing.clone(),
                    info.seat,
                    event.subject,
                    event.parent,
                ));
            }
        }
    }));
    game.run(None);
    assert_eq!(game.view(None).unwrap().waiting_for, Some(1));
    let rows = rows.lock().unwrap();
    assert!(rows.iter().any(|r| r.0 == "phaseUseCancelled" && r.1 == 0));
    assert!(!rows.iter().any(|r| r.0 == "phaseUseBegin" && r.1 == 0));
    assert!(rows.iter().any(|r| r.0 == "phaseDiscardBegin" && r.1 == 0));
    assert!(rows.iter().any(|r| r.0 == "phaseDrawSkipped" && r.1 == 1));
    assert!(rows.iter().all(|r| r.3.is_some()));
}

#[test]
fn luoyi_survives_extra_play_and_skipped_end_until_a_real_end_begin() {
    let mut game = table();
    game.seat_mut(0).hero = crate::classic_data::roster()
        .iter()
        .find(|hero| hero.id == "xuzhu");
    start(&mut game, Phase::Draw, None, 2);
    let Some(Decision::Cards(choice)) = game.decision.take() else {
        panic!("expected luoyi choice")
    };
    let branch = choice
        .branches
        .iter()
        .find(|branch| branch.label == "裸衣")
        .unwrap()
        .id;
    pump(
        &mut game,
        Some(Answer::Select(crate::Selection {
            branch,
            cards: vec![],
            targets: vec![],
        })),
    );
    assert!(game.has_skill(0, "luoyi2"));
    for index in 0..2 {
        start(&mut game, Phase::Play, Some("extra"), index);
        assert!(matches!(game.decision.take(), Some(Decision::Play(0))));
        pump(&mut game, Some(Answer::EndPlay));
        assert!(game.has_skill(0, "luoyi2"));
    }
    game.skip_next_phase(0, Phase::End).unwrap();
    start(&mut game, Phase::End, None, 6);
    assert!(game.has_skill(0, "luoyi2"));
    game.classic.as_mut().unwrap().hero_turn = Default::default();
    game.phase_state.begin_turn();
    assert!(game.has_skill(0, "luoyi2"));
    start(&mut game, Phase::End, None, 6);
    assert!(!game.has_skill(0, "luoyi2"));
}

#[test]
fn discard_count_stays_fixed_when_phase_discard_callback_changes_hand() {
    for gain in [true, false] {
        let mut game = table();
        game.seat_mut(0).hand = (1..=5).map(|id| card(id, CardKind::Tao)).collect();
        let mut change = registration("phaseDiscard");
        change.effect = Arc::new(move |ctx, _, _| {
            assert_eq!(
                ctx.game()
                    .phase_info(ctx.event().subject)
                    .unwrap()
                    .discard_required,
                Some(1)
            );
            if gain {
                ctx.gain_created_card(0, CardKind::Sha, crate::classic_data::Suit::Spade, 1, None)
                    .unwrap();
            } else {
                ctx.discard_hand(0, &[1, 2]).unwrap();
            }
            TriggerStep::Done
        });
        game.register_trigger(change).unwrap();
        start(&mut game, Phase::Discard, None, 4);
        let Some(Decision::Cards(choice)) = game.decision.take() else {
            panic!("fixed count requires one card")
        };
        assert_eq!((choice.min, choice.max), (1, 1));
        assert_eq!(choice.cards.len(), if gain { 6 } else { 3 });
        pump(&mut game, Some(Answer::Discard(vec![choice.cards[0]])));
        assert_eq!(game.seat(0).unwrap().hand.len(), if gain { 5 } else { 2 });
    }
}

#[test]
fn cancelling_phase_body_or_end_emits_cancelled_without_after() {
    for (phase, timing, prefix) in [
        (Phase::Discard, "phaseDiscard", "phaseDiscard"),
        (Phase::Play, "phaseUseEnd", "phaseUse"),
    ] {
        let mut game = table();
        game.seat_mut(0).hand = (1..=5).map(|id| card(id, CardKind::Tao)).collect();
        let mut cancel = registration(timing);
        cancel.effect = Arc::new(|_, _, _| TriggerStep::CancelSubject);
        game.register_trigger(cancel).unwrap();
        let rows = Arc::new(Mutex::new(Vec::new()));
        let out = rows.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if game.phase_info(event.subject).is_some() {
                out.lock().unwrap().push(event.timing.clone());
            }
        }));
        start(&mut game, phase, None, 0);
        if phase == Phase::Play {
            assert!(matches!(game.decision.take(), Some(Decision::Play(0))));
            pump(&mut game, Some(Answer::EndPlay));
        }
        assert!(game.decision.is_none());
        let rows = rows.lock().unwrap();
        assert!(rows.contains(&format!("{prefix}Cancelled")));
        assert!(!rows.contains(&format!("{prefix}After")));
        if phase == Phase::Discard {
            assert!(!rows.contains(&"phaseDiscardEnd".into()));
        }
    }
}

#[test]
fn cancelling_a_played_card_does_not_cancel_its_play_phase() {
    let mut game = table();
    let sha = card(1, CardKind::Sha);
    game.seat_mut(0).hand.push(sha);
    let mut cancel = registration("useCard2");
    cancel.effect = Arc::new(|_, _, _| TriggerStep::CancelSubject);
    game.register_trigger(cancel).unwrap();
    start(&mut game, Phase::Play, None, 0);
    assert!(matches!(game.decision.take(), Some(Decision::Play(0))));
    let attack = game.attack_program(0, vec![sha], vec![1]);
    pump(&mut game, Some(Answer::Play(attack)));
    assert!(matches!(game.decision.take(), Some(Decision::Play(0))));
    assert_eq!(game.seat(1).unwrap().hp, 4);
    pump(&mut game, Some(Answer::EndPlay));
}

#[test]
fn luoyi_damage_runs_after_first_do_and_can_be_revoked() {
    for revoke in [false, true] {
        let mut game = table();
        game.seat_mut(0).hero = crate::classic_data::roster()
            .iter()
            .find(|hero| hero.id == "xuzhu");
        start(&mut game, Phase::Draw, None, 2);
        let Some(Decision::Cards(choice)) = game.decision.take() else {
            panic!("expected luoyi choice")
        };
        let branch = choice
            .branches
            .iter()
            .find(|b| b.label == "裸衣")
            .unwrap()
            .id;
        pump(
            &mut game,
            Some(Answer::Select(crate::Selection {
                branch,
                cards: vec![],
                targets: vec![],
            })),
        );
        let seen = Arc::new(Mutex::new(Vec::new()));
        let output = seen.clone();
        let mut first = registration("damageBegin1");
        first.role = TriggerRole::Source;
        first.effect = Arc::new(move |ctx, _, _| {
            output.lock().unwrap().push(ctx.event().amount);
            if revoke {
                assert!(ctx.remove_temporary(0, "luoyi2"));
            }
            TriggerStep::Done
        });
        game.register_trigger(first).unwrap();
        let output = seen.clone();
        let mut last = registration("damageBegin1");
        last.skill = "damage_last".into();
        last.role = TriggerRole::Source;
        last.order = TriggerOrder::Last;
        last.effect = Arc::new(move |ctx, _, _| {
            output.lock().unwrap().push(ctx.event().amount);
            TriggerStep::Done
        });
        game.register_trigger(last).unwrap();
        let hit = game.damage_program(Some(0), 1, 1, Some(CardKind::Sha), vec![]);
        game.scheduler.start(hit);
        pump(&mut game, None);
        assert_eq!(*seen.lock().unwrap(), [1, if revoke { 1 } else { 2 }]);
        assert_eq!(game.seat(1).unwrap().hp, if revoke { 3 } else { 2 });
    }
}
