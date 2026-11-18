use super::*;

#[derive(Clone, Debug)]
struct Snapshot {
    turn: u64,
    timing: String,
    point: &'static str,
    temporary: bool,
    duel_usable: bool,
    permanent: bool,
}

#[test]
fn qiaoshui_loss_expires_after_phase_after_request() {
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
    let cards = Arc::new(Mutex::new([None; 2]));
    let permanent: Vec<_> = trials[1..]
        .iter()
        .map(|trial| {
            game.seat(trial.a).unwrap().hero.unwrap().skills[0]
                .id
                .clone()
        })
        .collect();
    let snapshots = Arc::new(Mutex::new(Vec::<Snapshot>::new()));
    for (index, trial) in trials[1..].iter().enumerate() {
        let saved = cards.clone();
        let mut reg = registration(
            trial.a,
            "expiry_duel_setup",
            "phaseUseBegin",
            TriggerOrder::Last,
            move |ctx, _, _| {
                let id = ctx
                    .gain_created_card(ctx.owner(), CardKind::Juedou, Suit::Spade, 1, None)
                    .unwrap();
                saved.lock().unwrap()[index] = Some(Card {
                    id,
                    kind: CardKind::Juedou,
                });
                TriggerStep::Done
            },
        );
        let turn = trial.turn;
        reg.filter =
            Arc::new(move |game, _, _| game.classic.as_ref().unwrap().turn.unwrap().number == turn);
        game.register_trigger(reg).unwrap();
    }
    let record = {
        let cards = cards.clone();
        let snapshots = snapshots.clone();
        let permanent = permanent.clone();
        move |game: &Game, event: &crate::TriggerEvent, point: &'static str| {
            let turn = game.classic.as_ref().unwrap().turn.unwrap().number;
            if !(3..=4).contains(&turn)
                || !["phaseUseEnd", "phaseEnd", "phaseAfter"].contains(&event.timing.as_str())
            {
                return;
            }
            let index = (turn - 3) as usize;
            let a = (turn - 1) as SeatId;
            if event.player != Some(a) {
                return;
            }
            let card = cards.lock().unwrap()[index].expect("duel created after comparison");
            let player = game.seat(a).unwrap();
            let usable = player.hand.contains(&card)
                && game
                    .card_uses(player, card)
                    .is_some_and(|uses| !uses.is_empty());
            snapshots.lock().unwrap().push(Snapshot {
                turn,
                timing: event.timing.clone(),
                point,
                temporary: game.has_skill(a, "qiaoshui2"),
                duel_usable: usable,
                permanent: game.has_skill(a, &permanent[index]),
            });
        }
    };
    let observer = record.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        observer(game, event, "request")
    }));
    for timing in ["phaseUseEnd", "phaseEnd", "phaseAfter"] {
        for (order, point) in [(TriggerOrder::First, "first"), (TriggerOrder::Last, "last")] {
            let observer = record.clone();
            let mut reg = registration(
                0,
                if point == "first" {
                    "expiry_probe_first"
                } else {
                    "expiry_probe_last"
                },
                timing,
                order,
                move |ctx, _, _| {
                    observer(ctx.game(), ctx.event(), point);
                    TriggerStep::Done
                },
            );
            reg.role = TriggerRole::Global;
            game.register_trigger(reg).unwrap();
        }
    }
    let mut rejected = [false; 2];
    for _ in 0..160 {
        if snapshots
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.turn == 4 && s.timing == "phaseAfter" && s.point == "last")
        {
            break;
        }
        if game.view(None).unwrap().turn.unwrap().number > 4 {
            break;
        }
        let public = game.view(None).unwrap();
        let turn = public.turn.unwrap().number;
        if (3..=4).contains(&turn) {
            let index = (turn - 3) as usize;
            let a = (turn - 1) as SeatId;
            let view = game.view(Some(a)).unwrap();
            if !rejected[index]
                && view.waiting_for == Some(a)
                && view.choice.is_none()
                && view.actions.contains(&Action::EndPlay)
            {
                let card = cards.lock().unwrap()[index].expect("physical duel created");
                assert!(game.seat(a).unwrap().hand.contains(&card));
                assert!(!view.actions.iter().any(
                    |action| matches!(action, Action::PlayCard { card: id, .. } if *id == card.id)
                ));
                let before_public = game.view(None).unwrap();
                assert_eq!(
                    game.submit(Command {
                        version: view.version,
                        seat: a,
                        action: Action::PlayCard {
                            card: card.id,
                            targets: vec![trials[index + 1].b]
                        }
                    }),
                    Err(RuleError::InvalidTarget)
                );
                assert_eq!(game.view(Some(a)).unwrap(), view);
                assert_eq!(game.view(None).unwrap(), before_public);
                rejected[index] = true;
            }
        }
        next_decision(&mut game, &trials);
    }
    for trial in &trials[1..] {
        let result = trial
            .log
            .lock()
            .unwrap()
            .returned
            .expect("comparison completed");
        assert_eq!(result.won, trial.sample["result"]["bool"]);
        assert_eq!(result.tie, trial.sample["result"]["tie"]);
    }
    assert_eq!(rejected, [true, true]);
    let observed = snapshots.lock().unwrap();
    for turn in 3..=4 {
        for timing in ["phaseUseEnd", "phaseEnd", "phaseAfter"] {
            for point in ["request", "first", "last"] {
                let row = observed
                    .iter()
                    .find(|s| s.turn == turn && s.timing == timing && s.point == point)
                    .expect("timing observation");
                let sample = if turn == 3 { "tie" } else { "loss" };
                let fixture_point = if point == "request" {
                    "request"
                } else if timing == "phaseAfter" {
                    "callback"
                } else {
                    "request"
                };
                let fixture_row = golden["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|entry| {
                        entry[0] == sample && entry[1] == fixture_point && entry[2] == timing
                    })
                    .expect("phase fixture row");
                let temporary = fixture_row[10] == "qiaoshui2";
                assert_eq!(row.temporary, temporary, "{row:?}");
                assert_eq!(row.duel_usable, !temporary, "{row:?}");
                assert!(row.permanent, "{row:?}");
            }
        }
    }
}
