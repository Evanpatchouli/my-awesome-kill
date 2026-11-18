use super::*;

#[derive(Clone, Debug)]
struct UseRow {
    turn: u64,
    timing: String,
    subject: EventId,
    targets: Vec<SeatId>,
    candidates: Vec<SeatId>,
    temporary: bool,
    paid: bool,
}

#[test]
fn qiaoshui_real_turns_change_one_paid_card_target_only_after_win() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/reference/compare.json"
    ))
    .unwrap();
    let mut game = Game::classic(5, 0, "guanyu", 8).unwrap();
    let trials: Vec<_> = golden["trials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| {
            let turn = sample["turn"].as_u64().unwrap();
            let a = turn as SeatId - 1;
            let trial = Trial {
                a,
                b: a + 1,
                turn,
                sample: sample.clone(),
                log: Default::default(),
            };
            trial.install(&mut game);
            trial
        })
        .collect();
    let cards = Arc::new(Mutex::new([[None; 2]; 3]));
    for (i, trial) in trials.iter().enumerate() {
        let saved = cards.clone();
        let a = trial.a;
        let b = trial.b;
        let d = (a + 3) % 8;
        let mut setup = registration(
            a,
            "use_target_setup",
            "phaseUseBegin",
            TriggerOrder::Last,
            move |ctx, _, _| {
                for seat in [b, d] {
                    let old: Vec<_> = ctx
                        .game()
                        .seat(seat)
                        .unwrap()
                        .hand
                        .iter()
                        .map(|c| c.id)
                        .collect();
                    ctx.discard_hand(seat, &old).unwrap();
                }
                let sha = ctx
                    .gain_created_card(a, CardKind::Sha, Suit::Spade, 2, None)
                    .unwrap();
                let duel = ctx
                    .gain_created_card(a, CardKind::Juedou, Suit::Spade, 3, None)
                    .unwrap();
                saved.lock().unwrap()[i] = [Some(sha), Some(duel)];
                TriggerStep::Done
            },
        );
        let turn = trial.turn;
        setup.filter =
            Arc::new(move |game, _, _| game.classic.as_ref().unwrap().turn.unwrap().number == turn);
        game.register_trigger(setup).unwrap();
    }
    let rows = Arc::new(Mutex::new(Vec::<UseRow>::new()));
    let seen = rows.clone();
    let saved = cards.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if !["useCard2", "useCardAfter"].contains(&event.timing.as_str()) {
            return;
        }
        let turn = game.classic.as_ref().unwrap().turn.unwrap().number;
        if !(2..=4).contains(&turn) || event.player != Some((turn - 1) as SeatId) {
            return;
        }
        let index = (turn - 2) as usize;
        let ids = saved.lock().unwrap()[index];
        let Some(card) = event.cards.iter().find(|c| ids.contains(&Some(c.id))) else {
            return;
        };
        let a = (turn - 1) as SeatId;
        seen.lock().unwrap().push(UseRow {
            turn,
            timing: event.timing.clone(),
            subject: event.subject,
            targets: game.use_targets(event.subject).unwrap().to_vec(),
            candidates: game.additional_use_targets(event.subject),
            temporary: game.has_skill(a, "qiaoshui3"),
            paid: !game.seat(a).unwrap().hand.contains(card) && game.resolving.contains(card),
        });
    }));
    let mut played = [0usize; 3];
    let mut selected = false;
    let mut hp_before = [None; 3];
    for _ in 0..250 {
        let view = game.view(None).unwrap();
        let turn = view.turn.unwrap().number;
        if turn > 4 {
            break;
        }
        if (2..=4).contains(&turn) {
            let i = (turn - 2) as usize;
            let a = trials[i].a;
            let b = trials[i].b;
            let d = (a + 3) % 8;
            if view.waiting_for == Some(a) {
                let private = game.view(Some(a)).unwrap();
                if let Some(choice) = &private.choice {
                    if let Some(add) = choice.branches.iter().find(|branch| {
                        branch.label == "巧说：增加目标" || branch.label == "巧说：选择额外目标"
                    }) {
                        assert!(!game.has_skill(a, "qiaoshui3"));
                        assert!(add.targets.contains(&d));
                        submit(
                            &mut game,
                            a,
                            Action::Select {
                                event: choice.event,
                                branch: add.id,
                                cards: vec![],
                                targets: vec![d],
                            },
                        );
                        selected = true;
                        continue;
                    }
                } else if private.actions.contains(&Action::EndPlay)
                    && played[i] < if i == 0 { 2 } else { 1 }
                {
                    let ids = cards.lock().unwrap()[i];
                    let card = ids[played[i]].expect("physical card created");
                    if played[i] == 0 {
                        hp_before[i] = Some((game.seat(b).unwrap().hp, game.seat(d).unwrap().hp));
                        assert!(!private.actions.iter().any(|action| matches!(action, Action::PlayCard { card: id, targets } if *id == card && targets.contains(&d))));
                    }
                    if i == 0 && played[i] == 1 {
                        let (before_b, before_d) = hp_before[i].unwrap();
                        assert_eq!(game.seat(b).unwrap().hp, before_b - 1);
                        assert_eq!(game.seat(d).unwrap().hp, before_d - 1);
                    }
                    assert!(private.actions.contains(&Action::PlayCard {
                        card,
                        targets: vec![b]
                    }));
                    played[i] += 1;
                    submit(
                        &mut game,
                        a,
                        Action::PlayCard {
                            card,
                            targets: vec![b],
                        },
                    );
                    continue;
                } else if played[i] > 0 {
                    let (before_b, before_d) = hp_before[i].unwrap();
                    assert!(game.seat(b).unwrap().hp < before_b);
                    assert_eq!(game.seat(d).unwrap().hp, before_d - i32::from(i == 0));
                    if i != 0 {
                        let duel = cards.lock().unwrap()[i][1].unwrap();
                        assert!(!private.actions.iter().any(|action| matches!(action, Action::PlayCard { card, .. } if *card == duel)));
                    }
                }
            }
        }
        next_decision(&mut game, &trials);
    }
    assert!(selected);
    assert_eq!(played, [2, 1, 1]);
    let rows = rows.lock().unwrap();
    for (i, trial) in trials.iter().enumerate() {
        let own: Vec<_> = rows.iter().filter(|row| row.turn == trial.turn).collect();
        let expected: Vec<_> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                row[0] == trial.sample["sample"]
                    && row[1] == "request"
                    && (row[2] == "useCard2" || row[2] == "useCardAfter")
            })
            .collect();
        assert_eq!(own.len(), expected.len());
        for (row, reference) in own.iter().zip(expected) {
            assert_eq!(row.timing, reference[2]);
            assert_eq!(row.temporary, reference[10] == "qiaoshui3");
            assert!(row.paid, "{row:?}");
        }
        let fixture_targets = trial.sample["cardTargets"].as_array().unwrap();
        assert_eq!(own.len(), fixture_targets.len());
        for (row, expected) in own.iter().zip(fixture_targets) {
            let targets: Vec<_> = expected["targets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|target| match target.as_str().unwrap() {
                    "B" => trial.b,
                    "D" => (trial.a + 3) % 8,
                    _ => panic!("unexpected fixture target"),
                })
                .collect();
            assert_eq!(row.targets, targets);
        }
        if i == 0 {
            assert!(own[0].candidates.contains(&((trial.a + 3) % 8)));
        }
        for pair in own.chunks_exact(2) {
            assert_eq!(pair[0].subject, pair[1].subject);
            assert_eq!(pair[0].targets, [trial.b]);
            let mut target = vec![trial.b];
            if i == 0 && pair[0].subject == own[0].subject {
                target.push((trial.a + 3) % 8);
            }
            assert_eq!(pair[1].targets, target);
        }
        let ids = cards.lock().unwrap()[i];
        for id in ids.iter().take(played[i]).flatten() {
            assert_eq!(game.discard.iter().filter(|card| card.id == *id).count(), 1);
            assert!(!game.resolving.iter().any(|card| card.id == *id));
        }
    }
}
