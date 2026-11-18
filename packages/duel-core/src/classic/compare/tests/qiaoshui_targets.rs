use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{Phase, Program};

fn fixture(name: &str) -> Value {
    let root: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/reference/qiaoshui-targets.json"
    ))
    .unwrap();
    root["trials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == name)
        .unwrap()
        .clone()
}
fn seats(value: &Value) -> Vec<SeatId> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|s| u32::from(s.as_str().unwrap().as_bytes()[0] - b'A'))
        .collect()
}
fn table(kind: CardKind, extra: &[(SeatId, Card)]) -> Game {
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
    game.seat_mut(0).hand.push(card(1, kind));
    for &(seat, c) in extra {
        game.seat_mut(seat).hand.push(c);
    }
    game.grant_temporary(0, crate::qiaoshui::win_skill(0))
        .unwrap();
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    game
}
fn select(
    game: &mut Game,
    seat: SeatId,
    choice: &ChoiceView,
    branch: u32,
    cards: Vec<u32>,
    targets: Vec<SeatId>,
) {
    submit(
        game,
        seat,
        Action::Select {
            event: choice.event,
            branch,
            cards,
            targets,
        },
    );
}
#[derive(Debug)]
struct Row {
    timing: String,
    subject: EventId,
    targets: Vec<SeatId>,
    bonus: bool,
    paid: bool,
}
fn observe(game: &mut Game) -> Arc<Mutex<Vec<Row>>> {
    let rows = Arc::new(Mutex::new(Vec::new()));
    let output = rows.clone();
    game.timing_observer = Some(Arc::new(move |game, e| {
        if e.player != Some(0)
            || !["useCard2", "useCardAfter"].contains(&e.timing.as_str())
            || !e.cards.iter().any(|c| c.id == 1)
        {
            return;
        }
        output.lock().unwrap().push(Row {
            timing: e.timing.clone(),
            subject: e.subject,
            targets: game.use_targets(e.subject).unwrap().to_vec(),
            bonus: game.has_skill(0, "qiaoshui3"),
            paid: !game.seat(0).unwrap().hand.iter().any(|c| c.id == 1)
                && game.resolving.iter().any(|c| c.id == 1),
        });
    }));
    rows
}
fn verify(game: &Game, f: &Value, rows: &Arc<Mutex<Vec<Row>>>) {
    let rows = rows.lock().unwrap();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(
        (&*rows[0].timing, &*rows[1].timing),
        ("useCard2", "useCardAfter")
    );
    assert_eq!(rows[0].subject, rows[1].subject);
    assert_eq!(rows[0].targets, seats(&f["initial"]["targets"]));
    assert_eq!(rows[1].targets, seats(&f["final"]["targets"]));
    assert_eq!(rows[0].bonus, f["initial"]["bonus"]);
    assert_eq!(rows[1].bonus, f["final"]["bonus"]);
    assert!(rows.iter().all(|r| r.paid));
    assert_eq!(
        game.discard.iter().filter(|c| c.id == 1).count(),
        f["final"]["useCount"].as_u64().unwrap() as usize
    );
    assert!(!game.resolving.iter().any(|c| c.id == 1));
}
fn drive(game: &mut Game, mode: &str) {
    let mut selected = false;
    let mut response = false;
    for _ in 0..100 {
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        let private = game.view(Some(seat)).unwrap();
        let Some(choice) = private.choice else {
            assert_eq!(seat, 0);
            break;
        };
        if let Some(b) = choice.branches.iter().find(|b| b.label == "巧说：减少目标") {
            assert_eq!(seat, 0);
            assert!(!game.has_skill(0, "qiaoshui3"));
            if mode == "automatic-remove" {
                assert!(b.targets.contains(&7));
                select(game, seat, &choice, b.id, vec![], vec![7]);
            } else {
                assert!(choice.branches.iter().any(|b| b.label == "巧说：增加目标"));
                select(game, seat, &choice, 0, vec![], vec![]);
            }
            selected = true;
            continue;
        }
        if choice.counter.is_some() && mode == "both-cancel" && seat == 3 && !response {
            let b = choice
                .branches
                .iter()
                .find(|b| b.cards.contains(&3))
                .unwrap();
            select(game, seat, &choice, b.id, vec![3], vec![]);
            response = true;
            continue;
        }
        if mode == "automatic-remove"
            && seat == 3
            && choice.kind == ChoiceKind::Content
            && choice.branches.iter().any(|b| b.cards.contains(&3))
        {
            let b = choice
                .branches
                .iter()
                .find(|b| b.cards.contains(&3))
                .unwrap();
            select(game, seat, &choice, b.id, vec![3], vec![]);
            response = true;
            continue;
        }
        match choice.kind {
            ChoiceKind::Shan | ChoiceKind::Save => submit(
                game,
                seat,
                Action::Choose {
                    event: choice.event,
                    card: None,
                },
            ),
            ChoiceKind::Discard => submit(
                game,
                seat,
                Action::Discard {
                    event: choice.event,
                    cards: choice.cards[..choice.min].to_vec(),
                },
            ),
            ChoiceKind::Content => select(game, seat, &choice, 0, vec![], vec![]),
        }
    }
    assert!(selected && response);
}
#[test]
fn automatic_remove_keeps_passive_responder_bonus() {
    let f = fixture("automatic-remove");
    let mut game = table(CardKind::Nanman, &[(3, card(3, CardKind::Sha))]);
    game.grant_temporary(3, crate::qiaoshui::win_skill(3))
        .unwrap();
    let rows = observe(&mut game);
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: 1,
            targets: seats(&f["initial"]["targets"]),
        },
    );
    drive(&mut game, "automatic-remove");
    verify(&game, &f, &rows);
    let hp: Vec<_> = (0..8).map(|s| game.seat(s).unwrap().hp).collect();
    assert_eq!(json!(hp), f["final"]["effects"]["hp"]);
    assert_eq!(
        game.has_skill(3, "qiaoshui3"),
        f["final"]["effects"]["responderBonus"]
    );
    assert_eq!(game.discard.iter().filter(|c| c.id == 3).count(), 1);
}
#[test]
fn both_cancel_and_active_counter_consume_bonuses() {
    let f = fixture("both-cancel");
    let mut game = table(
        CardKind::Tiesuo,
        &[(3, card(3, CardKind::Wuxie)), (0, card(7, CardKind::Sha))],
    );
    game.grant_temporary(3, crate::qiaoshui::win_skill(3))
        .unwrap();
    let rows = observe(&mut game);
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: 1,
            targets: seats(&f["initial"]["targets"]),
        },
    );
    drive(&mut game, "both-cancel");
    verify(&game, &f, &rows);
    let linked: Vec<_> = (0..8).filter(|&s| game.seat(s).unwrap().linked).collect();
    assert_eq!(linked, seats(&f["final"]["effects"]["linked"]));
    assert_eq!(
        game.has_skill(3, "qiaoshui3"),
        f["final"]["effects"]["nullifierBonus"]
    );
    assert_eq!(game.discard.iter().filter(|c| c.id == 3).count(), 1);
    assert!(!game.has_skill(0, "qiaoshui3"));
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: 7,
            targets: vec![1],
        },
    );
    assert!(!game.has_skill(0, "qiaoshui3"));
    assert!(game.view(Some(0)).unwrap().choice.is_none());
}

#[test]
fn borrowed_swords_preserves_positional_victims_and_active_shan_consumes_bonus() {
    let f = fixture("borrowed-swords");
    let mut game = table(
        CardKind::Jiedao,
        &[
            (1, card(2, CardKind::Sha)),
            (3, card(3, CardKind::Sha)),
            (4, card(4, CardKind::Shan)),
        ],
    );
    game.seat_mut(1).equipment.push(card(5, CardKind::Qinglong));
    game.seat_mut(3).equipment.push(card(6, CardKind::Qinglong));
    game.grant_temporary(4, crate::qiaoshui::win_skill(4))
        .unwrap();
    let rows = observe(&mut game);
    submit(
        &mut game,
        0,
        Action::PlayCard {
            card: 1,
            targets: seats(&f["initial"]["targets"]),
        },
    );
    let mut added = false;
    let mut paired = false;
    let mut slash = false;
    let mut shan = false;
    let mut declined = false;
    for _ in 0..100 {
        let view = game.view(None).unwrap();
        let seat = view.waiting_for.unwrap();
        let private = game.view(Some(seat)).unwrap();
        let Some(choice) = private.choice else {
            assert_eq!(seat, 0);
            break;
        };
        if let Some(b) = choice.branches.iter().find(|b| b.label == "巧说：增加目标") {
            assert_eq!(seat, 0);
            assert!(!game.has_skill(0, "qiaoshui3"));
            assert!(b.targets.contains(&1));
            select(&mut game, seat, &choice, b.id, vec![], vec![1]);
            added = true;
            continue;
        }
        if let Some(b) = choice.branches.iter().find(|b| b.label == "选择被杀目标") {
            assert_eq!(seat, 0);
            assert_eq!(choice.target, Some(1));
            assert!(b.targets.contains(&2));
            select(&mut game, seat, &choice, b.id, vec![], vec![2]);
            paired = true;
            continue;
        }
        if choice.counter.is_none() && seat == 1 && choice.target == Some(4) {
            let b = choice
                .branches
                .iter()
                .find(|b| b.cards.contains(&2))
                .expect("B uses slash on E");
            select(&mut game, seat, &choice, b.id, vec![2], vec![]);
            slash = true;
            continue;
        }
        if seat == 4 && choice.kind == ChoiceKind::Shan {
            assert!(game.has_skill(4, "qiaoshui3"));
            submit(
                &mut game,
                seat,
                Action::Choose {
                    event: choice.event,
                    card: Some(4),
                },
            );
            shan = true;
            continue;
        }
        if seat == 4
            && choice.kind == ChoiceKind::Content
            && choice.branches.iter().any(|b| b.cards.contains(&4))
        {
            assert!(game.has_skill(4, "qiaoshui3"));
            let b = choice
                .branches
                .iter()
                .find(|b| b.cards.contains(&4))
                .unwrap();
            select(&mut game, seat, &choice, b.id, vec![4], vec![]);
            shan = true;
            continue;
        }
        if choice.counter.is_none() && seat == 3 && choice.target == Some(2) {
            select(&mut game, seat, &choice, 0, vec![], vec![]);
            declined = true;
            continue;
        }
        match choice.kind {
            ChoiceKind::Shan | ChoiceKind::Save => submit(
                &mut game,
                seat,
                Action::Choose {
                    event: choice.event,
                    card: None,
                },
            ),
            ChoiceKind::Discard => submit(
                &mut game,
                seat,
                Action::Discard {
                    event: choice.event,
                    cards: choice.cards[..choice.min].to_vec(),
                },
            ),
            ChoiceKind::Content => select(&mut game, seat, &choice, 0, vec![], vec![]),
        }
    }
    assert!((added, paired, slash, shan, declined) == (true, true, true, true, true));
    let rows = rows.lock().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].targets, vec![3]);
    assert_eq!(rows[1].targets, seats(&f["final"]["targets"]));
    assert_eq!(rows[0].subject, rows[1].subject);
    assert!(rows[0].bonus && !rows[1].bonus && rows.iter().all(|r| r.paid));
    assert_eq!(
        game.seat(1).unwrap().equipment,
        [card(5, CardKind::Qinglong)]
    );
    assert!(game
        .seat(0)
        .unwrap()
        .hand
        .contains(&card(6, CardKind::Qinglong)));
    assert!(game.seat(3).unwrap().hand.contains(&card(3, CardKind::Sha)));
    for id in [1, 2, 4] {
        assert_eq!(game.discard.iter().filter(|c| c.id == id).count(), 1);
    }
    assert_eq!(
        json!([game.seat(2).unwrap().hp, game.seat(4).unwrap().hp]),
        f["final"]["effects"]["hp"]
    );
    assert_eq!(
        game.has_skill(4, "qiaoshui3"),
        f["final"]["effects"]["shanBonus"]
    );
}

#[test]
fn target_edits_reject_duplicates_dead_and_ineligible_targets_and_last_removal() {
    for kind in [CardKind::Sha, CardKind::Shunshou] {
        let mut game = table(kind, &[(1, card(2, CardKind::Tao))]);
        game.remove_temporary(0, "qiaoshui3");
        game.seat_mut(3).alive = false;
        game.seat_mut(2).hero = crate::classic_data::roster()
            .iter()
            .find(|hero| hero.id == "zhugeliang");
        let checked = Arc::new(Mutex::new(false));
        let checked_now = checked.clone();
        let mut probe = registration(
            0,
            "invalid_target_probe",
            "useCard2",
            TriggerOrder::First,
            move |ctx, _, _| {
                assert_eq!(ctx.game().use_targets(ctx.event().subject), Some(&[1][..]));
                assert_eq!(ctx.add_use_target(1), Err(RuleError::InvalidTarget));
                assert_eq!(ctx.add_use_target(3), Err(RuleError::InvalidTarget));
                assert_eq!(ctx.add_use_target(2), Err(RuleError::InvalidTarget));
                assert_eq!(ctx.remove_use_target(1), Err(RuleError::InvalidTarget));
                assert_eq!(ctx.game().use_targets(ctx.event().subject), Some(&[1][..]));
                *checked_now.lock().unwrap() = true;
                TriggerStep::Done
            },
        );
        probe.role = TriggerRole::Player;
        game.register_trigger(probe).unwrap();
        submit(
            &mut game,
            0,
            Action::PlayCard {
                card: 1,
                targets: vec![1],
            },
        );
        assert!(*checked.lock().unwrap());
    }
}
