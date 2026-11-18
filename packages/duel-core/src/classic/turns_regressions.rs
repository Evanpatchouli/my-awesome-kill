use super::*;
use crate::{ChoiceBranch, TriggerEvent};

fn table() -> Game {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist, Role::Rebel], 0);
    game.classic.as_mut().unwrap().turn = None;
    game.classic.as_mut().unwrap().deck = (100..300).map(|id| card(id, CardKind::Tao)).collect();
    game.seat_mut(0).hand.push(card(1, CardKind::Tao));
    game
}
fn choose(game: &mut Game, recipient: SeatId) {
    let view = game.view(None).unwrap();
    let seat = view.waiting_for.unwrap();
    let Some(choice) = game.view(Some(seat)).unwrap().choice else {
        game.submit(Command {
            version: view.version,
            seat,
            action: Action::EndPlay,
        })
        .unwrap();
        return;
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
            let (branch, cards, targets) =
                if let Some(b) = choice.branches.iter().find(|b| b.label == "发动放权") {
                    (b.id, vec![], vec![])
                } else if let Some(b) = choice.branches.iter().find(|b| b.label == "弃置一张手牌")
                {
                    (b.id, vec![b.cards[0]], vec![])
                } else if let Some(b) = choice
                    .branches
                    .iter()
                    .find(|b| b.label == "选择额外回合角色")
                {
                    (b.id, vec![], vec![recipient])
                } else {
                    let b = &choice.branches[0];
                    (
                        b.id,
                        b.cards[..b.min_cards].to_vec(),
                        b.targets[..b.min_targets].to_vec(),
                    )
                };
            Action::Select {
                event: choice.event,
                branch,
                cards,
                targets,
            }
        }
    };
    game.submit(Command {
        version: view.version,
        seat,
        action,
    })
    .unwrap();
}
#[test]
fn cancelled_granted_turn_dispatches_cleanup_without_phase_after() {
    let mut game = table();
    game.register_trigger(crate::phase_content::fangquan(0))
        .unwrap();
    let mut cancel = reg("cancel_extra", "phaseBegin", |ctx, _, _| {
        assert_eq!(ctx.game().skill_mark_count(1, "fangquan", Some(0)), 1);
        assert!(ctx.game().has_skill(1, "fangquan3"));
        TriggerStep::CancelSubject
    });
    cancel.owner = 1;
    cancel.role = TriggerRole::Player;
    cancel.filter = Arc::new(|game, e, _| {
        game.turn_info(e.subject)
            .is_some_and(|t| t.reason.as_deref() == Some("fangquan2"))
    });
    game.register_trigger(cancel).unwrap();
    let rows = Arc::new(Mutex::new(Vec::new()));
    let out = rows.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.player == Some(1)
            && game
                .turn_info(event.subject)
                .is_some_and(|turn| turn.reason.as_deref() == Some("fangquan2"))
            && ["phaseCancelled", "phaseAfter"].contains(&event.timing.as_str())
        {
            out.lock().unwrap().push((
                "request",
                event.timing.clone(),
                game.skill_mark_count(1, "fangquan", Some(0)),
                game.has_skill(1, "fangquan3"),
            ));
        }
    }));
    for order in [TriggerOrder::First, TriggerOrder::Last] {
        let out = rows.clone();
        let mut observer = reg(
            if order == TriggerOrder::First {
                "first_cancel"
            } else {
                "last_cancel"
            },
            "phaseCancelled",
            move |ctx, _, _| {
                if ctx.event().player == Some(1)
                    && ctx
                        .game()
                        .turn_info(ctx.event().subject)
                        .is_some_and(|turn| turn.reason.as_deref() == Some("fangquan2"))
                {
                    out.lock().unwrap().push((
                        if order == TriggerOrder::First {
                            "first"
                        } else {
                            "last"
                        },
                        ctx.event().timing.clone(),
                        ctx.game().skill_mark_count(1, "fangquan", Some(0)),
                        ctx.game().has_skill(1, "fangquan3"),
                    ));
                }
                TriggerStep::Done
            },
        );
        observer.order = order;
        game.register_trigger(observer).unwrap();
    }
    game.run(None);
    for _ in 0..80 {
        if rows.lock().unwrap().iter().any(|r| r.0 == "last") {
            break;
        }
        choose(&mut game, 1);
    }
    assert_eq!(
        *rows.lock().unwrap(),
        [
            ("request", "phaseCancelled".into(), 1, true),
            ("first", "phaseCancelled".into(), 1, true),
            ("last", "phaseCancelled".into(), 0, false)
        ]
    );
    let turn = game
        .turn_history()
        .iter()
        .find(|t| t.reason.as_deref() == Some("fangquan2"))
        .unwrap();
    assert!(turn.cancelled && !turn.skipped);
    assert!(!game.has_skill(1, "fangquan3"));
    assert_eq!(game.skill_mark_count(1, "fangquan", Some(0)), 0);
}
#[test]
fn dead_queued_recipient_runs_turn_and_only_its_marks_are_cleared() {
    let mut game = table();
    // Start with C's natural turn so C can spend a real limited skill before A uses 放权.
    game.classic.as_mut().unwrap().turn = Some(crate::TurnView {
        seat: 1,
        number: 1,
        round: 1,
        phase: Phase::End,
    });
    game.seat_mut(1).hp = 1;
    game.add_skill_mark(1, "other_dead", Some(0), 2).unwrap();
    game.add_skill_mark(2, "other_living", Some(0), 3).unwrap();
    let limited = crate::ActiveSkillRegistration {
        owner: 2,
        skill: "unrelated_limited".into(),
        limited: true,
        choice: Arc::new(|_, _| {
            Some(ChoiceBranch {
                id: 0,
                label: "limited".into(),
                cards: vec![],
                min_cards: 0,
                max_cards: 0,
                targets: vec![],
                min_targets: 0,
                max_targets: 0,
            })
        }),
        effect: Arc::new(|_, _, _| TriggerStep::Done),
    };
    let limited_id = game.register_active_skill(limited).unwrap();
    game.register_trigger(crate::phase_content::fangquan(0))
        .unwrap();
    let mut kill = reg("kill_queued", "phaseAfter", |ctx, _, _| {
        if ctx.event().player == Some(0) {
            assert_eq!(ctx.game().skill_mark_count(1, "fangquan", Some(0)), 1);
            assert!(ctx.game().has_skill(1, "fangquan3"));
            ctx.damage(Some(0), 1, 1, None, None, &[]).unwrap();
        }
        TriggerStep::Done
    });
    kill.order = TriggerOrder::Normal;
    game.register_trigger(kill).unwrap();
    let rows = Arc::new(Mutex::new(Vec::new()));
    let out = rows.clone();
    game.timing_observer = Some(Arc::new(move |game, event: &TriggerEvent| {
        if event.timing == "dieAfter" && event.player == Some(1) {
            out.lock().unwrap().push("dieAfter".to_owned());
        }
        if event.player == Some(1)
            && game
                .turn_info(event.subject)
                .is_some_and(|t| t.reason.as_deref() == Some("fangquan2"))
            && [
                "phaseBefore",
                "phaseBegin",
                "phaseAfter",
                "phaseChange",
                "phaseCancelled",
            ]
            .contains(&event.timing.as_str())
        {
            assert!(!game.is_alive(1));
            out.lock().unwrap().push(event.timing.clone());
        }
    }));
    game.run(None);
    let mut spent = false;
    for _ in 0..120 {
        if rows.lock().unwrap().iter().any(|row| row == "phaseAfter") {
            break;
        }
        let view = game.view(None).unwrap();
        if !spent && view.waiting_for == Some(2) && game.view(Some(2)).unwrap().choice.is_none() {
            game.submit(Command {
                version: view.version,
                seat: 2,
                action: Action::ActivateSkill {
                    registration: limited_id,
                    cards: vec![],
                    targets: vec![],
                },
            })
            .unwrap();
            spent = true;
        } else {
            choose(&mut game, 1);
        }
    }
    assert!(spent && game.limited_skill_used(2, "unrelated_limited"));
    assert_eq!(game.skill_mark_count(2, "other_living", Some(0)), 3);
    assert!(game.skill_marks(1).is_empty());
    assert!(!game.has_skill(1, "fangquan3"));
    let turn = game
        .turn_history()
        .iter()
        .find(|t| t.seat == 1 && t.reason.as_deref() == Some("fangquan2"))
        .unwrap();
    assert!(!turn.cancelled && !turn.skipped && turn.personal_started);
    assert_eq!(
        *rows.lock().unwrap(),
        ["dieAfter", "phaseBefore", "phaseBegin", "phaseAfter"]
    );
}
