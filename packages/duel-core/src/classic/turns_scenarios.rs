use super::*;
use crate::classic_data::Suit;
use serde_json::{json, Value};

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Preparation => "phaseZhunbei",
        Phase::Judgment => "phaseJudge",
        Phase::Draw => "phaseDraw",
        Phase::Play => "phaseUse",
        Phase::Discard => "phaseDiscard",
        Phase::End => "phaseJieshu",
    }
}
fn letter(seat: SeatId) -> String {
    ((b'A' + seat as u8) as char).to_string()
}
fn observe(game: &Game, event: &crate::TriggerEvent, point: &str, turns: &Value) -> Option<Value> {
    let timing = event.timing.as_str();
    let supported = [
        "phaseBefore",
        "phaseBeforeStart",
        "phaseBeforeEnd",
        "phaseBeginStart",
        "phaseBegin",
        "phaseEnd",
        "phaseAfter",
        "phaseOver",
        "roundStart",
        "roundEnd",
        "phaseCancelled",
        "phaseUseBefore",
        "phaseUseCancelled",
        "phaseDrawSkipped",
        "phaseUseSkipped",
    ];
    if !supported.contains(&timing) {
        return None;
    }
    let subject = game.event_kind(event.subject)?;
    let (name, parent, turn, stage) = match subject {
        EventKind::Turn { .. } => ("phase", "phaseLoop", game.turn_info(event.subject), None),
        EventKind::PhaseLoop => ("phaseLoop", "game", None, None),
        EventKind::Phase { phase, .. } => {
            let info = game.phase_info(event.subject).unwrap();
            if timing.ends_with("Skipped") {
                assert!(info.skipped);
            }
            let stage = json!([
                info.index,
                info.reason,
                u8::from(info.cancelled),
                u8::from(info.finished)
            ]);
            (
                phase_name(phase),
                "phase",
                game.turn_info(event.parent.unwrap()),
                Some(stage),
            )
        }
        _ => return None,
    };
    let id = turn
        .as_ref()
        .map(|turn| turns[(turn.number - 1) as usize]["id"].clone())
        .unwrap_or(Value::Null);
    Some(json!([
        point,
        timing,
        name,
        event.player.map(letter),
        parent,
        id,
        game.turn_number(),
        game.round_number(),
        game.current_phase_player().map(letter),
        stage
    ]))
}
#[test]
fn natural_dangxian_fangquan_and_delayed_trick_match_reference_observations() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/phase-scheduling.json"
    ))
    .unwrap();
    let turns = golden["turns"].clone();
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
    game.classic.as_mut().unwrap().turn = None;
    game.seat_mut(6).turned_over = true;
    game.seat_mut(7).hp = 9;
    game.seat_mut(7).max_hp = 9;
    let mut slash = Vec::new();
    for rank in 3..=7 {
        slash.push(
            game.create_content_card(CardKind::Sha, Suit::Spade, rank, None)
                .unwrap(),
        );
    }
    game.seat_mut(0).hand.extend(slash.iter().copied());
    let mut costs = Vec::new();
    for seat in 1..=4 {
        let cost = game
            .create_content_card(CardKind::Tao, Suit::Heart, seat as u8 + 3, None)
            .unwrap();
        game.seat_mut(seat).hand.push(cost);
        costs.push(cost);
    }
    let lebu = game
        .create_content_card(CardKind::Lebu, Suit::Club, 6, None)
        .unwrap();
    game.seat_mut(7).judges.push(lebu);
    let draw1 = game
        .create_content_card(CardKind::Tao, Suit::Heart, 2, None)
        .unwrap();
    let draw2 = game
        .create_content_card(CardKind::Tao, Suit::Heart, 3, None)
        .unwrap();
    let judge = game
        .create_content_card(CardKind::Sha, Suit::Spade, 8, None)
        .unwrap();
    let mut deck = vec![draw1, draw2];
    for i in 0..12 {
        deck.push(
            game.create_content_card(CardKind::Tao, Suit::Heart, (i % 13 + 1) as u8, None)
                .unwrap(),
        );
    }
    deck.push(judge);
    for i in 0..40 {
        deck.push(
            game.create_content_card(CardKind::Tao, Suit::Heart, (i % 13 + 1) as u8, None)
                .unwrap(),
        );
    }
    deck.reverse();
    game.classic.as_mut().unwrap().deck = deck;
    game.skip_next_phase(2, Phase::Draw).unwrap();
    game.register_trigger(crate::phase_content::dangxian(0))
        .unwrap();
    for seat in 1..=4 {
        game.register_trigger(crate::phase_content::fangquan(seat))
            .unwrap();
    }
    let rows = Arc::new(Mutex::new(Vec::new()));
    let out = rows.clone();
    let t = turns.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if let Some(row) = observe(game, event, "request", &t) {
            out.lock().unwrap().push(row);
        }
    }));
    for timing in [
        "phaseBefore",
        "phaseBeforeStart",
        "phaseBeforeEnd",
        "phaseBeginStart",
        "phaseBegin",
        "phaseEnd",
        "phaseAfter",
        "phaseOver",
        "roundStart",
        "roundEnd",
        "phaseCancelled",
        "phaseUseBefore",
        "phaseUseCancelled",
        "phaseDrawSkipped",
        "phaseUseSkipped",
    ] {
        let out = rows.clone();
        let t = turns.clone();
        game.register_trigger(reg(
            &format!("observer_{timing}"),
            timing,
            move |ctx, _, _| {
                assert!(matches!(
                    ctx.game().event_kind(ctx.game().scheduler.current_id()),
                    Some(EventKind::RegisteredTrigger { .. })
                ));
                if let Some(row) = observe(ctx.game(), ctx.event(), "callback", &t) {
                    out.lock().unwrap().push(row);
                }
                TriggerStep::Done
            },
        ))
        .unwrap();
    }
    let recipient_marks = Arc::new(Mutex::new(Vec::new()));
    for timing in ["phaseBefore", "phaseAfter", "phaseCancelled"] {
        let marks = recipient_marks.clone();
        game.register_trigger(reg(
            &format!("recipient_{timing}"),
            timing,
            move |ctx, _, _| {
                let Some(turn) = ctx.game().turn_info(ctx.event().subject) else {
                    return TriggerStep::Done;
                };
                if turn.reason.as_deref() == Some("fangquan2") {
                    let source = if turn.seat == 0 { 1 } else { 4 };
                    assert_eq!(
                        ctx.game()
                            .skill_mark_count(turn.seat, "fangquan", Some(source)),
                        1
                    );
                    assert!(ctx.game().has_skill(turn.seat, "fangquan3"));
                    marks
                        .lock()
                        .unwrap()
                        .push((turn.seat, ctx.event().timing.clone()));
                }
                TriggerStep::Done
            },
        ))
        .unwrap();
    }
    game.run(None);
    let mut slashes = 0;
    let mut last_play_phase = None;
    for _ in 0..250 {
        let view = game.view(None).unwrap();
        if game.turn_history().len() >= 11 {
            break;
        }
        let seat = view.waiting_for.unwrap();
        let private = game.view(Some(seat)).unwrap();
        if let Some(choice) = private.choice {
            let action = match choice.kind {
                ChoiceKind::Discard => {
                    let mut cards: Vec<_> = choice
                        .cards
                        .iter()
                        .copied()
                        .filter(|id| !slash.iter().any(|c| c.id == *id))
                        .collect();
                    cards.extend(
                        choice
                            .cards
                            .iter()
                            .copied()
                            .filter(|id| slash.iter().any(|c| c.id == *id)),
                    );
                    Action::Discard {
                        event: choice.event,
                        cards: cards[..choice.min].to_vec(),
                    }
                }
                ChoiceKind::Shan | ChoiceKind::Save => Action::Choose {
                    event: choice.event,
                    card: None,
                },
                ChoiceKind::Content => {
                    let (branch, cards, targets) = if let Some(b) =
                        choice.branches.iter().find(|b| b.label == "发动放权")
                    {
                        (if seat == 2 { 0 } else { b.id }, vec![], vec![])
                    } else if let Some(b) =
                        choice.branches.iter().find(|b| b.label == "弃置一张手牌")
                    {
                        if seat == 3 {
                            (0, vec![], vec![])
                        } else {
                            (b.id, vec![costs[(seat - 1) as usize].id], vec![])
                        }
                    } else if let Some(b) = choice
                        .branches
                        .iter()
                        .find(|b| b.label == "选择额外回合角色")
                    {
                        (b.id, vec![], vec![if seat == 1 { 0 } else { 6 }])
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
            continue;
        }
        if seat == 0 && slashes < 4 && last_play_phase != Some(game.scheduler.current_id()) {
            last_play_phase = Some(game.scheduler.current_id());
            let id = slash[slashes].id;
            assert!(private.actions.contains(&Action::PlayCard {
                card: id,
                targets: vec![7]
            }));
            slashes += 1;
            game.submit(Command {
                version: view.version,
                seat,
                action: Action::PlayCard {
                    card: id,
                    targets: vec![7],
                },
            })
            .unwrap();
        } else {
            game.submit(Command {
                version: view.version,
                seat,
                action: Action::EndPlay,
            })
            .unwrap();
        }
    }
    assert_eq!(slashes, 4);
    assert_eq!(
        game.seat(7).unwrap().hp,
        golden["final"]["targetHp"].as_i64().unwrap() as i32
    );
    assert!(game.discard.contains(&costs[0]) && game.discard.contains(&costs[3]));
    assert!(
        game.seat(2).unwrap().hand.contains(&costs[1])
            && game.seat(3).unwrap().hand.contains(&costs[2])
    );
    assert!(game.discard.contains(&lebu));
    assert_eq!(
        *recipient_marks.lock().unwrap(),
        [
            (0, "phaseBefore".to_owned()),
            (0, "phaseAfter".to_owned()),
            (6, "phaseBefore".to_owned()),
            (6, "phaseCancelled".to_owned())
        ]
    );
    assert_eq!(game.skill_mark_count(0, "fangquan", Some(1)), 0);
    assert_eq!(game.skill_mark_count(6, "fangquan", Some(4)), 0);
    for (actual, expected) in game.turn_history().iter().zip(turns.as_array().unwrap()) {
        assert_eq!(letter(actual.seat), expected["owner"]);
        assert_eq!(actual.reason.as_deref(), expected["source"].as_str());
    }
    assert_eq!(game.turn_history().len(), turns.as_array().unwrap().len());
    let reference: Vec<Value> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            ["request", "callback"].contains(&row[0].as_str().unwrap_or(""))
                && [
                    "phaseBefore",
                    "phaseBeforeStart",
                    "phaseBeforeEnd",
                    "phaseBeginStart",
                    "phaseBegin",
                    "phaseEnd",
                    "phaseAfter",
                    "phaseOver",
                    "roundStart",
                    "roundEnd",
                    "phaseCancelled",
                    "phaseUseBefore",
                    "phaseUseCancelled",
                    "phaseDrawSkipped",
                    "phaseUseSkipped",
                ]
                .contains(&row[1].as_str().unwrap_or(""))
        })
        .map(|row| {
            let mut columns = row.as_array().unwrap()[..10].to_vec();
            if let Some(stage) = columns[9].as_array() {
                columns[9] = json!([stage[0], stage[1], stage[3], stage[4]]);
            }
            Value::Array(columns)
        })
        .collect();
    let actual = rows.lock().unwrap();
    let mut cursor = 0;
    for (index, expected) in reference.iter().enumerate() {
        let Some(offset) = actual[cursor..].iter().position(|row| row == expected) else {
            panic!(
                "missing reference row {index}: {expected}; next actual: {:?}",
                actual.get(cursor)
            );
        };
        cursor += offset + 1;
    }
    eprintln!(
        "compared {} phase request/callback rows of {} upstream rows",
        reference.len(),
        golden["events"].as_array().unwrap().len()
    );
}
fn queue_observation(game: &Game, event: &crate::TriggerEvent, point: &str) -> Option<Value> {
    let timing = event.timing.as_str();
    if ![
        "phaseBefore",
        "roundStart",
        "phaseBeforeStart",
        "phaseBeforeEnd",
        "phaseBeginStart",
        "phaseBegin",
        "phaseEnd",
        "phaseAfter",
        "phaseOver",
    ]
    .contains(&timing)
    {
        return None;
    }
    let turn = game.turn_info(event.subject);
    let event_kind = if turn.is_some() {
        "phase"
    } else if matches!(game.event_kind(event.subject), Some(EventKind::PhaseLoop)) {
        "phaseLoop"
    } else {
        return None;
    };
    let turn_name = turn.as_ref().map(|turn| {
        turn.reason
            .clone()
            .unwrap_or_else(|| format!("natural-{}", letter(turn.seat)))
    });
    let personal = event
        .player
        .map(|seat| game.personal_turn_number(seat))
        .unwrap_or(0);
    let queue: Vec<_> = game
        .queued_turns()
        .map(|turn| {
            turn.reason
                .clone()
                .unwrap_or_else(|| format!("natural-{}", letter(turn.seat)))
        })
        .collect();
    let info = if let Some(t) = turn.as_ref() {
        json!({"queue": queue, "personalPhaseNumber": personal, "historyMe": t.personal_started, "historySkipped": t.skipped})
    } else {
        json!({"queue": queue})
    };
    Some(json!([
        point,
        timing,
        event_kind,
        event.player.map(letter),
        if turn.is_some() { "phaseLoop" } else { "game" },
        turn_name,
        game.natural_phase_player().map(letter),
        game.current_phase_player().map(letter),
        game.turn_number(),
        game.round_number(),
        info
    ]))
}
#[test]
fn queued_dead_extra_and_nested_append_follow_reference_observations() {
    let golden: Value =
        serde_json::from_str(include_str!("../../../../tests/reference/phase-queue.json")).unwrap();
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
    game.classic.as_mut().unwrap().turn = None;
    game.classic.as_mut().unwrap().deck = (100..300).map(|id| card(id, CardKind::Tao)).collect();
    game.seat_mut(4).hp = 1;
    let rows = Arc::new(Mutex::new(Vec::new()));
    let deaths = Arc::new(Mutex::new(Vec::new()));
    let death_requests = deaths.clone();
    let out = rows.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.player == Some(4) && ["die", "dieAfter"].contains(&event.timing.as_str()) {
            assert!(matches!(
                game.event_kind(event.subject),
                Some(EventKind::Death { target: 4, .. })
            ));
            assert!(matches!(
                event.parent.and_then(|id| game.event_kind(id)),
                Some(EventKind::Dying { target: 4, .. })
            ));
            death_requests.lock().unwrap().push(json!([
                "request",
                event.timing,
                letter(4),
                game.seat(4).unwrap().alive
            ]));
        }
        if let Some(row) = queue_observation(game, event, "request") {
            out.lock().unwrap().push(row);
        }
    }));
    for timing in [
        "phaseBefore",
        "roundStart",
        "phaseBeforeStart",
        "phaseBeforeEnd",
        "phaseBeginStart",
        "phaseBegin",
        "phaseEnd",
        "phaseAfter",
        "phaseOver",
    ] {
        let out = rows.clone();
        game.register_trigger(reg(
            &format!("queue_observer_{timing}"),
            timing,
            move |ctx, _, _| {
                assert!(matches!(
                    ctx.game().event_kind(ctx.game().scheduler.current_id()),
                    Some(EventKind::RegisteredTrigger { .. })
                ));
                if let Some(row) = queue_observation(ctx.game(), ctx.event(), "callback") {
                    out.lock().unwrap().push(row);
                }
                TriggerStep::Done
            },
        ))
        .unwrap();
    }
    for timing in ["die", "dieAfter"] {
        let deaths = deaths.clone();
        game.register_trigger(reg(
            &format!("death_observer_{timing}"),
            timing,
            move |ctx, _, _| {
                if ctx.event().player == Some(4) {
                    deaths.lock().unwrap().push(json!([
                        "callback",
                        ctx.event().timing,
                        letter(4),
                        ctx.game().seat(4).unwrap().alive
                    ]));
                }
                TriggerStep::Done
            },
        ))
        .unwrap();
    }
    let operations = Arc::new(Mutex::new(Vec::new()));
    let seen_ops = operations.clone();
    let returned = Arc::new(Mutex::new(None));
    let returned_callback = returned.clone();
    let mut enqueue = reg("queue_fixture", "phaseEnd", move |ctx, stage, _| {
        if stage == 1 {
            assert!(matches!(
                ctx.game().event_kind(ctx.event().subject),
                Some(EventKind::Turn { .. })
            ));
            assert!(matches!(
                ctx.event().parent.and_then(|id| ctx.game().event_kind(id)),
                Some(EventKind::PhaseLoop)
            ));
            *returned_callback.lock().unwrap() = Some(
                json!(["operation", "returned-from-die", "phase", letter(ctx.owner()), "phaseLoop", "natural-A", ctx.game().natural_phase_player().map(letter), ctx.game().current_phase_player().map(letter), ctx.game().turn_number(), ctx.game().round_number(), {"deadSeatAlive":ctx.game().is_alive(4),"queue":queue_names(ctx.game())}]),
            );
            return TriggerStep::Done;
        }
        let turn = ctx.game().turn_info(ctx.event().subject).unwrap();
        if turn.seat == 0 && turn.reason.is_none() {
            for (seat, reason, front) in [
                (2, "append-c", false),
                (3, "append-d", false),
                (4, "append-dead-e", false),
                (1, "front-b", true),
            ] {
                let before = queue_names(ctx.game());
                ctx.insert_extra_turn_at(
                    seat,
                    reason,
                    if front {
                        crate::TurnPosition::Front
                    } else {
                        crate::TurnPosition::Back
                    },
                )
                .unwrap();
                seen_ops.lock().unwrap().push(json!({"recipient": letter(seat), "source": reason, "front": front, "before": before, "after": queue_names(ctx.game())}));
            }
            ctx.damage(Some(0), 4, 1, None, None, &[]).unwrap();
            return TriggerStep::Continue(1);
        } else if turn.reason.as_deref() == Some("append-c") {
            let before = queue_names(ctx.game());
            ctx.insert_extra_turn(5, "nested-append-f").unwrap();
            seen_ops.lock().unwrap().push(json!({"recipient": "F", "source": "nested-append-f", "front": false, "before": before, "after": queue_names(ctx.game())}));
        }
        TriggerStep::Done
    });
    enqueue.order = TriggerOrder::Normal;
    game.register_trigger(enqueue).unwrap();
    game.run(None);
    let mut lifecycle = Vec::new();
    for _ in 0..120 {
        lifecycle.extend(game.view(None).unwrap().events);
        if game.turn_history().len() >= 9 {
            break;
        }
        advance(&mut game);
    }
    assert!(!game.seat(4).unwrap().alive);
    assert!(game.match_result().is_none());
    let actual = rows.lock().unwrap();
    let reference: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            ["request", "callback"].contains(&row[0].as_str().unwrap_or(""))
                && [
                    "phaseBefore",
                    "roundStart",
                    "phaseBeforeStart",
                    "phaseBeforeEnd",
                    "phaseBeginStart",
                    "phaseBegin",
                    "phaseEnd",
                    "phaseAfter",
                    "phaseOver",
                ]
                .contains(&row[1].as_str().unwrap_or(""))
        })
        .collect();
    let mut cursor = 0;
    for (index, expected) in reference.iter().enumerate() {
        let Some(offset) = actual[cursor..]
            .iter()
            .position(|row| row.as_array().unwrap()[..10] == expected.as_array().unwrap()[..10])
        else {
            panic!(
                "missing queue row {index}: {expected}; next {:?}",
                actual.get(cursor)
            );
        };
        cursor += offset + 1;
        if let (Some(actual_info), Some(expected_info)) =
            (actual[cursor - 1][10].as_object(), expected[10].as_object())
        {
            for key in [
                "queue",
                "personalPhaseNumber",
                "historyMe",
                "historySkipped",
            ] {
                if expected_info.contains_key(key) {
                    assert_eq!(
                        actual_info[key], expected_info[key],
                        "queue row {index} {key}"
                    );
                }
            }
        }
    }
    let reference_ops: Vec<_> = golden["events"].as_array().unwrap().iter().filter(|row| row[0] == "operation" && row[1] == "insertPhase").map(|row| {
        let d = &row[10]; json!({"recipient": d["recipient"], "source": d["source"], "front": d["front"], "before": d["before"], "after": d["after"]})
    }).collect();
    assert_eq!(*operations.lock().unwrap(), reference_ops);
    let reference_deaths: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            ["request", "callback"].contains(&row[0].as_str().unwrap_or(""))
                && ["die", "dieAfter"].contains(&row[1].as_str().unwrap_or(""))
        })
        .map(|row| json!([row[0], row[1], row[3], row[10]["deadSeatAlive"]]))
        .collect();
    assert_eq!(*deaths.lock().unwrap(), reference_deaths);
    let expected_return = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == "operation" && row[1] == "returned-from-die")
        .unwrap();
    assert_eq!(returned.lock().unwrap().as_ref(), Some(expected_return));
    assert!(game
        .turn_history()
        .iter()
        .all(|turn| turn.reason.as_deref() != Some("front-b")));
    assert!(game
        .queued_turns()
        .all(|turn| turn.reason.as_deref() != Some("front-b")));
    for expected in golden["final"]["queuedTurns"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["source"] != "front-b")
    {
        let turn = game
            .turn_history()
            .iter()
            .find(|turn| turn.reason.as_deref() == expected["source"].as_str())
            .expect("queued turn observed");
        assert_eq!(letter(turn.seat), expected["player"]);
        assert_eq!(
            lifecycle
                .iter()
                .any(|event| event.id == turn.event && event.status == crate::EventStatus::Started),
            expected["started"]
        );
        assert_eq!(lifecycle.iter().any(|event| event.id == turn.event && event.status == crate::EventStatus::Finished), expected["finished"]);
        assert_eq!(
            game.queued_turns()
                .any(|item| item.reason.as_deref() == expected["source"].as_str()),
            expected["stillQueued"]
        );
    }
    eprintln!(
        "compared {} queue request/callback rows, {} queue operations, {} death rows and returned-from-die",
        reference.len(),
        reference_ops.len(),
        reference_deaths.len()
    );
}

fn queue_names(game: &Game) -> Vec<String> {
    game.queued_turns()
        .map(|turn| {
            turn.reason
                .clone()
                .unwrap_or_else(|| format!("natural-{}", letter(turn.seat)))
        })
        .collect()
}
