use super::*;
use serde_json::{json, Value};

fn letter(seat: SeatId) -> String {
    ((b'A' + seat as u8) as char).to_string()
}
fn event_name(kind: EventKind, skill: &str) -> &'static str {
    match kind {
        EventKind::CompareMany { .. } => "chooseToCompare",
        EventKind::GroupChoices { .. } => "chooseCardOL",
        EventKind::GroupBid { .. } => "chooseCard",
        EventKind::LoseAsync => "loseAsync",
        EventKind::Lose { .. } => "lose",
        EventKind::ShowBid { .. } | EventKind::ShowCards { .. } => "showCards",
        EventKind::OrderingDiscard => "orderingDiscard",
        EventKind::CardsDiscard { .. } => "cardsDiscard",
        EventKind::ActiveSkillEffect { .. } => {
            if skill == "gushe" {
                "gushe"
            } else {
                "twdanlie"
            }
        }
        other => panic!("unexpected comparison frame {other:?}"),
    }
}
fn capture(
    game: &Game,
    event: &TriggerEvent,
    point: &str,
    bids: &[Card],
    skill: &str,
) -> Option<Value> {
    let timing = event.timing.as_str();
    // The upstream fixture records only requests for these private/early hooks.
    if point == "callback"
        && [
            "chooseToCompareBefore",
            "chooseToCompareBegin",
            "chooseCardOLBegin",
            "chooseCardBegin",
            "chooseCardAfter",
        ]
        .contains(&timing)
    {
        return None;
    }
    if ![
        "chooseToCompareBefore",
        "chooseToCompareBegin",
        "chooseCardOLBegin",
        "chooseCardBegin",
        "chooseCardAfter",
        "chooseCardOLAfter",
        "loseAsyncBegin",
        "loseBefore",
        "loseAfter",
        "loseCancelled",
        "loseAsyncAfter",
        "compareCardShowBefore",
        "showCardsAfter",
        "compare",
        "compareFixing",
        "chooseToCompareEnd",
        "chooseToCompareAfter",
        "chooseToCompareCancelled",
        "cardsDiscardAfter",
    ]
    .contains(&timing)
    {
        return None;
    }
    if [
        "loseBefore",
        "loseAfter",
        "loseCancelled",
        "cardsDiscardAfter",
    ]
    .contains(&timing)
        && !event.cards.iter().any(|card| bids.contains(card))
    {
        return None;
    }
    let subject = event_name(game.event_kind(event.subject)?, skill);
    let parent = event_name(game.event_kind(event.parent?)?, skill);
    let zones: Vec<String> = bids
        .iter()
        .enumerate()
        .map(|(i, card)| {
            let seat = [0, 1, 3, 4][i];
            if game.seat(seat).unwrap().hand.contains(card) {
                format!("{}:h", letter(seat))
            } else if game.resolving.contains(card) {
                "o".into()
            } else if game.discard.contains(card) {
                "d".into()
            } else {
                "?".into()
            }
        })
        .collect();
    let frame = game.group_comparison(event.subject);
    let scores = frame
        .filter(|frame| frame.numbers.iter().any(Option::is_some))
        .map(|frame| json!(frame.numbers))
        .unwrap_or(Value::Null);
    let mut detail = json!({});
    if let Some(frame) = frame {
        let labels: Vec<_> = frame.group.targets.iter().copied().map(letter).collect();
        detail["targets"] = json!(labels);
        detail["selectedCards"] = json!(frame
            .cards
            .iter()
            .map(|card| card
                .and_then(|card| bids.iter().position(|bid| *bid == card))
                .map(|i| format!("{}-bid", letter([0, 1, 3, 4][i]))))
            .collect::<Vec<_>>());
        detail["index"] = json!(frame.index);
        detail["internalWinner"] = json!(frame.winner.map(letter));
        detail["result"] = if let Some(result) = &frame.result {
            json!({"player":"A-bid","targets":result.cards.iter().skip(1).map(|card|format!("{}-bid",letter([0,1,3,4][bids.iter().position(|bid|bid==card).unwrap()]))).collect::<Vec<_>>(),"num1":result.numbers.iter().map(|n|n[0]).collect::<Vec<_>>(),"num2":result.numbers.iter().map(|n|n[1]).collect::<Vec<_>>(),"winner":result.winner.map(letter)})
        } else {
            Value::Null
        };
    }
    Some(json!([
        point,
        timing,
        subject,
        parent,
        event.player.map(letter),
        event.target.map(letter),
        zones,
        scores,
        detail
    ]))
}
#[test]
fn five_real_content_trials_compare_common_request_callback_fields() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/compare-multiple.json"
    ))
    .unwrap();
    let timings = [
        "chooseToCompareBefore",
        "chooseToCompareBegin",
        "chooseCardOLBegin",
        "chooseCardBegin",
        "chooseCardAfter",
        "chooseCardOLAfter",
        "loseAsyncBegin",
        "loseBefore",
        "loseAfter",
        "loseCancelled",
        "loseAsyncAfter",
        "compareCardShowBefore",
        "showCardsAfter",
        "compare",
        "compareFixing",
        "chooseToCompareEnd",
        "chooseToCompareAfter",
        "chooseToCompareCancelled",
        "cardsDiscardAfter",
    ];
    let mut compared = 0usize;
    for trial in golden["trials"].as_array().unwrap() {
        let sample = trial["sample"].as_str().unwrap();
        let skill = trial["skill"].as_str().unwrap();
        let ranks: Vec<_> = trial["input"]["printedPoints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as u8)
            .collect();
        let (mut game, bids) = setup(ranks.try_into().unwrap());
        for seat in [0, 1, 3, 4] {
            let other = game
                .create_content_card(CardKind::Shan, Suit::Club, 2, None)
                .unwrap();
            game.seat_mut(seat).hand.push(other);
        }
        if skill == "twdanlie" {
            game.seat_mut(0).hp = 3;
        }
        let active = game
            .register_active_skill(if skill == "gushe" {
                crate::compare_content::gushe(0)
            } else {
                crate::compare_content::danlie(0)
            })
            .unwrap();
        let rows = Arc::new(Mutex::new(Vec::new()));
        let out = rows.clone();
        let bs = bids.clone();
        let name = skill.to_owned();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if let Some(row) = capture(game, event, "request", &bs, &name) {
                out.lock().unwrap().push(row);
            }
        }));
        for timing in timings {
            let out = rows.clone();
            let bs = bids.clone();
            let name = skill.to_owned();
            game.register_trigger(reg(
                &format!("observe_{timing}"),
                timing,
                move |ctx, _, _| {
                    if let Some(row) = capture(ctx.game(), ctx.event(), "callback", &bs, &name) {
                        out.lock().unwrap().push(row);
                    }
                    TriggerStep::Done
                },
            ))
            .unwrap();
        }
        if skill == "twdanlie" {
            for registration in crate::compare_content::danlie_number(0) {
                game.register_trigger(registration).unwrap();
            }
        }
        if sample == "pairs-fixing" || sample == "meanwhile-fixing" {
            let is_pair = sample == "pairs-fixing";
            let mut modify = reg("fixture_modify", "compareFixing", move |ctx, _, _| {
                let frame = ctx.game().group_comparison(ctx.event().subject).unwrap();
                if is_pair && frame.target == Some(1) {
                    ctx.set_compare_number(0, 10).unwrap();
                } else if !is_pair && frame.player == Some(0) {
                    ctx.set_compare_number(0, 1).unwrap();
                } else if !is_pair && frame.target == Some(3) {
                    ctx.set_compare_number(3, 13).unwrap();
                }
                TriggerStep::Done
            });
            modify.order = TriggerOrder::Normal;
            game.register_trigger(modify).unwrap();
        }
        if sample == "cancel-one-loss" {
            let mut cancel = reg("fixture_cancel_loss", "loseBefore", |_, _, _| {
                TriggerStep::CancelSubject
            });
            cancel.order = TriggerOrder::Normal;
            cancel.filter = Arc::new(|_, event, _| event.player == Some(3));
            game.register_trigger(cancel).unwrap();
        }
        if sample == "cancel-root" {
            let mut cancel = reg("fixture_cancel_root", "compareCardShowBefore", |_, _, _| {
                TriggerStep::CancelSubject
            });
            cancel.order = TriggerOrder::Normal;
            game.register_trigger(cancel).unwrap();
        }
        game.scheduler.start(Program::event(EventKind::Phase {
            seat: 0,
            phase: crate::Phase::Play,
        }));
        game.run(None);
        let view = game.view(None).unwrap();
        game.submit(Command {
            version: view.version,
            seat: 0,
            action: Action::ActivateSkill {
                registration: active,
                cards: vec![],
                targets: vec![4, 1, 3],
            },
        })
        .unwrap();
        for _ in 0..100 {
            let view = game.view(None).unwrap();
            let Some(seat) = view.waiting_for else { break };
            let Some(choice) = game.view(Some(seat)).unwrap().choice else {
                break;
            };
            let action = if choice.compare_group.is_some() {
                for card in &bids {
                    assert!(!game.is_known_by(card.id, Some(2)));
                }
                let bid = bids[[0, 1, 3, 4].iter().position(|&id| id == seat).unwrap()];
                Action::Select {
                    event: choice.event,
                    branch: 1,
                    cards: vec![bid.id],
                    targets: vec![],
                }
            } else if choice.kind == ChoiceKind::Content {
                Action::Select {
                    event: choice.event,
                    branch: 0,
                    cards: vec![],
                    targets: vec![],
                }
            } else if choice.kind == ChoiceKind::Discard {
                Action::Discard {
                    event: choice.event,
                    cards: choice.cards[..choice.min].to_vec(),
                }
            } else {
                Action::Choose {
                    event: choice.event,
                    card: None,
                }
            };
            game.submit(Command {
                version: view.version,
                seat,
                action,
            })
            .unwrap();
        }
        let expected: Vec<_> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                r[0] == sample
                    && ["request", "callback"].contains(&r[1].as_str().unwrap_or(""))
                    && timings.contains(&r[2].as_str().unwrap_or(""))
            })
            .collect();
        let rows = rows.lock().unwrap();
        assert_eq!(rows.len(), expected.len(), "{sample} count");
        for (i, (actual, reference)) in rows.iter().zip(expected).enumerate() {
            let base = json!([
                reference[1],
                reference[2],
                reference[3],
                reference[4],
                reference[5],
                reference[6],
                reference[7],
                reference[8]
            ]);
            assert_eq!(
                Value::Array(actual.as_array().unwrap()[..8].to_vec()),
                base,
                "{sample} row {i}"
            );
            let detail = &reference[9];
            for key in ["targets", "selectedCards", "index", "internalWinner"] {
                if detail.get(key).is_some() {
                    assert_eq!(actual[8][key], detail[key], "{sample} row {i} {key}");
                }
            }
            if let Some(result) = detail.get("result") {
                if result.is_null() {
                    assert!(actual[8]["result"].is_null(), "{sample} row {i}");
                } else {
                    for key in ["player", "targets", "num1", "num2", "winner"] {
                        if result.get(key).is_some() {
                            assert_eq!(
                                actual[8]["result"][key], result[key],
                                "{sample} row {i} result.{key}"
                            );
                        }
                    }
                }
            }
            compared += 1;
        }
    }
    eprintln!(
        "compared {compared} shared requests/callbacks of {} source rows",
        golden["events"].as_array().unwrap().len()
    );
}
