use super::*;
use crate::{CardFace, EventKind};

type TraceRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<bool>,
    Option<String>,
    Vec<(String, Option<String>)>,
);

fn pump_content(
    game: &mut Game,
    mut answer: Option<Answer>,
    rows: &Arc<Mutex<Vec<TraceRow>>>,
    name: &str,
    delayed: crate::Card,
    revealed: crate::Card,
) {
    while let Some(work) = game.scheduler.poll() {
        match work {
            Work::Run(event, program, child) => game.resolve(event, program, child, &mut answer),
            Work::Started(event, Program::Trick(flow)) => {
                record_content(game, event, &flow, "begin", rows, name, delayed, revealed)
            }
            Work::Ended(event, Program::Trick(flow), _) => {
                record_content(game, event, &flow, "end", rows, name, delayed, revealed)
            }
            _ => {}
        }
        if game.decision.is_some() {
            break;
        }
    }
}

fn record_content(
    game: &Game,
    event: crate::scheduler::EventRef,
    flow: &crate::classic::tricks::Trick,
    timing: &str,
    rows: &Arc<Mutex<Vec<TraceRow>>>,
    name: &str,
    delayed: crate::Card,
    revealed: crate::Card,
) {
    let Some(effect) = flow.judgment_effect_name() else {
        return;
    };
    let material = flow.judgment_effect_material().expect("effect material");
    assert_eq!(material, delayed);
    let EventKind::TrickEffect { seat, .. } = flow.kind() else {
        unreachable!()
    };
    let parent = event
        .parent
        .and_then(|id| game.scheduler.active_frame(id))
        .map(|(_, p)| frame_name(p.kind()))
        .unwrap_or("reference_fixture_use");
    let owner = game
        .card_location(revealed.id)
        .and_then(|location| location.owner)
        .map(|s| ((b'A' + s as u8) as char).to_string());
    rows.lock().unwrap().push((
        timing.into(),
        "content".into(),
        effect.into(),
        parent.into(),
        Some(((b'A' + seat as u8) as char).to_string()),
        None,
        zone(game, delayed),
        zone(game, revealed),
        owner,
        None,
        None,
        vec![(format!("{name}-delayed"), zone(game, material))],
    ));
}

fn zone(game: &Game, card: crate::Card) -> Option<String> {
    use crate::CardZone::*;
    let location = game.card_location(card.id)?;
    let suffix = match location.zone {
        Some(Hand) => "h",
        Some(Equipment) => "e",
        Some(Judgment) => "j",
        Some(Expansion) => "x",
        Some(DrawPile) => "c",
        Some(Ordering) => "o",
        Some(Special) => "s",
        Some(DiscardPile) => "d",
        None => return None,
    };
    Some(match location.owner {
        Some(owner) => format!("{}:{suffix}", (b'A' + owner as u8) as char),
        None => suffix.into(),
    })
}

fn frame_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Phase {
            phase: Phase::Judgment,
            ..
        } => "phaseJudge",
        EventKind::Lose { .. } => "lose",
        EventKind::Judge { .. } => "judge",
        EventKind::CardsDiscard { .. } => "cardsDiscard",
        EventKind::OrderingDiscard => "orderingDiscard",
        EventKind::Damage { .. } => "damage",
        EventKind::ChooseToUse { .. } => "chooseToUse",
        EventKind::CounterWindow { .. } => "_wuxie",
        EventKind::Gain { .. } => "gain",
        EventKind::UseResponseCard { .. }
        | EventKind::UseVirtual {
            kind: CardKind::Wuxie,
            ..
        } => "useCard",
        EventKind::SkillTrigger {
            skill: crate::SkillId::Tiandu,
            ..
        } => "tiandu",
        EventKind::TrickEffect {
            kind: CardKind::Shandian,
            ..
        } => "shandian",
        _ => "other",
    }
}

#[test]
fn six_eight_seat_judgments_match_reference_requests_and_callbacks() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/judgment-zones.json"
    ))
    .unwrap();
    let watched = [
        "phaseJudgeBefore",
        "phaseJudgeBegin",
        "loseBefore",
        "loseAfter",
        "phaseJudge",
        "judge",
        "judgeFixing",
        "judgeEnd",
        "phaseJudgeEnd",
        "phaseJudgeAfter",
        "cardsDiscardAfter",
        "damageBegin1",
        "damageEnd",
        "gainAfter",
        "chooseToUseBegin",
        "chooseToUseAfter",
    ];
    for (name, kind, suit, counter, tiandu) in [
        (
            "lebu-fail",
            CardKind::Lebu,
            crate::classic_data::Suit::Club,
            false,
            false,
        ),
        (
            "lebu-pass-tiandu",
            CardKind::Lebu,
            crate::classic_data::Suit::Heart,
            false,
            true,
        ),
        (
            "lightning-hit",
            CardKind::Shandian,
            crate::classic_data::Suit::Spade,
            false,
            false,
        ),
        (
            "lightning-pass",
            CardKind::Shandian,
            crate::classic_data::Suit::Heart,
            false,
            false,
        ),
        (
            "lightning-counter",
            CardKind::Shandian,
            crate::classic_data::Suit::Spade,
            true,
            false,
        ),
        (
            "lebu-counter",
            CardKind::Lebu,
            crate::classic_data::Suit::Club,
            true,
            false,
        ),
    ] {
        let mut game = scene(
            &[
                Role::Lord,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Rebel,
                Role::Rebel,
                Role::Rebel,
                Role::Renegade,
            ],
            0,
        );
        game.seat_mut(0).hp = 8;
        game.seat_mut(0).max_hp = 8;
        if tiandu {
            game.seat_mut(0).hero = crate::classic_data::roster()
                .iter()
                .find(|h| h.id == "guojia");
        }
        let delayed = card(500, kind);
        let revealed = card(501, CardKind::Sha);
        let wuxie = card(502, CardKind::Wuxie);
        game.seat_mut(0).judges.push(delayed);
        if counter {
            game.seat_mut(7).hand.push(wuxie);
        }
        let state = game.classic.as_mut().unwrap();
        state.faces.push(CardFace {
            id: delayed.id,
            suit: crate::classic_data::Suit::Club,
            rank: 6,
        });
        state.faces.push(CardFace {
            id: revealed.id,
            suit,
            rank: 8,
        });
        state.deck.push(revealed);
        let rows: Arc<Mutex<Vec<TraceRow>>> = Arc::new(Mutex::new(Vec::new()));
        let capture = move |game: &Game, event: &crate::TriggerEvent, point: &'static str| {
            if !watched.contains(&event.timing.as_str()) {
                return None;
            }
            if ["loseBefore", "loseAfter"].contains(&event.timing.as_str())
                && !event.cards.contains(&delayed)
                && !event.cards.contains(&wuxie)
            {
                return None;
            }
            if event.timing == "cardsDiscardAfter"
                && !event.cards.contains(&delayed)
                && !event.cards.contains(&revealed)
                && !event.cards.contains(&wuxie)
            {
                return None;
            }
            if event.timing == "gainAfter" && !event.cards.contains(&revealed) {
                return None;
            }
            let kind = game.scheduler.active_frame(event.subject)?.1.kind();
            let parent = event
                .parent
                .and_then(|id| game.scheduler.active_frame(id))
                .map(|(_, p)| frame_name(p.kind()))
                .unwrap_or("reference_fixture_use");
            let label = |seat: Option<u32>| seat.map(|s| ((b'A' + s as u8) as char).to_string());
            let card_label = |card: crate::Card| {
                if card == delayed {
                    format!("{name}-delayed")
                } else if card == revealed {
                    format!("{name}-revealed")
                } else {
                    format!("{name}-wuxie")
                }
            };
            let judged = game
                .judgment_card(event.subject)
                .or_else(|| event.player.and_then(|seat| game.judging_card(seat)))
                .map(card_label);
            let cards = event
                .cards
                .iter()
                .copied()
                .map(|card| (card_label(card), zone(game, card)))
                .collect();
            Some((
                event.timing.clone(),
                point.to_owned(),
                frame_name(kind).to_owned(),
                parent.to_owned(),
                label(event.player),
                label(event.source),
                zone(game, delayed),
                zone(game, revealed),
                label(
                    game.card_location(revealed.id)
                        .and_then(|location| location.owner),
                ),
                game.judgment_result(event.subject)
                    .or_else(|| game.response_result(event.subject)),
                judged,
                cards,
            ))
        };
        for timing in watched {
            let out = rows.clone();
            let mut probe = registration(timing);
            probe.role = TriggerRole::Global;
            probe.effect = Arc::new(move |ctx, _, _| {
                if let Some(row) = capture(ctx.game(), ctx.event(), "callback") {
                    out.lock().unwrap().push(row);
                }
                TriggerStep::Done
            });
            game.register_trigger(probe).unwrap();
        }
        let out = rows.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if let Some(row) = capture(game, event, "request") {
                out.lock().unwrap().push(row);
            }
        }));
        game.scheduler
            .start(Program::Phase(Flow::new(0, Phase::Judgment, None, 1)));
        pump_content(&mut game, None, &rows, name, delayed, revealed);
        let mut countered = false;
        while let Some(Decision::Cards(choice)) = game.decision.take() {
            let (branch, cards) = if choice.counter.is_some() && counter && !countered {
                if let Some(b) = choice.branches.iter().find(|b| b.cards.contains(&wuxie.id)) {
                    countered = true;
                    (b.id, vec![wuxie.id])
                } else {
                    (0, vec![])
                }
            } else if choice.counter.is_none() && tiandu {
                (1, vec![])
            } else {
                (0, vec![])
            };
            pump_content(
                &mut game,
                Some(Answer::Select(crate::Selection {
                    branch,
                    cards,
                    targets: vec![],
                })),
                &rows,
                name,
                delayed,
                revealed,
            );
        }
        assert_eq!(countered, counter, "{name}");
        let expected: Vec<TraceRow> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                r["sample"] == name
                    && (r["point"] == "content"
                        || ((r["point"] == "request" || r["point"] == "callback")
                            && watched.contains(&r["timing"].as_str().unwrap())))
            })
            .map(|r| {
                (
                    r["timing"].as_str().unwrap().to_owned(),
                    r["point"].as_str().unwrap().to_owned(),
                    r["event"].as_str().unwrap().to_owned(),
                    r["parent"].as_str().unwrap().to_owned(),
                    r["player"].as_str().map(str::to_owned),
                    r["source"].as_str().map(str::to_owned),
                    r["delayed"][1].as_str().map(str::to_owned),
                    r["revealed"][1].as_str().map(str::to_owned),
                    r["revealedOwner"].as_str().map(str::to_owned),
                    r["accepted"].as_bool(),
                    r["judgmentCard"].as_str().map(str::to_owned),
                    r["cards"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|card| {
                            (
                                card[0].as_str().unwrap().to_owned(),
                                card[1].as_str().map(str::to_owned),
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        let all_rows: Vec<_> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["sample"] == name)
            .collect();
        assert_eq!(
            expected.len(),
            all_rows.len(),
            "no reference rows may be omitted"
        );
        assert!(all_rows
            .iter()
            .all(|row| row["delayed"][0] == format!("{name}-delayed")
                && row["revealed"][0] == format!("{name}-revealed")));
        assert_eq!(*rows.lock().unwrap(), expected, "{name}");
        let trial = golden["trials"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .unwrap();
        assert_eq!(zone(&game, delayed).as_deref(), trial["delayed"].as_str());
        assert_eq!(zone(&game, revealed).as_deref(), trial["revealed"].as_str());
        assert_eq!(
            8 - game.seat(0).unwrap().hp,
            if name == "lightning-hit" { 3 } else { 0 }
        );
    }
}

#[test]
fn delayed_and_judgment_cards_follow_independent_reference_lifecycles() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/judgment-zones.json"
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
            Role::Renegade,
        ],
        0,
    );
    let delayed = card(500, CardKind::Lebu);
    let revealed = card(501, CardKind::Sha);
    game.seat_mut(0).judges.push(delayed);
    let state = game.classic.as_mut().unwrap();
    state.faces.push(CardFace {
        id: delayed.id,
        suit: crate::classic_data::Suit::Club,
        rank: 6,
    });
    state.faces.push(CardFace {
        id: revealed.id,
        suit: crate::classic_data::Suit::Club,
        rank: 8,
    });
    state.deck.push(revealed);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let out = observed.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if ![
            "loseAfter",
            "phaseJudge",
            "judge",
            "judgeFixing",
            "judgeEnd",
            "phaseJudgeEnd",
            "phaseJudgeAfter",
            "cardsDiscardAfter",
        ]
        .contains(&event.timing.as_str())
        {
            return;
        }
        let zone = |card| {
            if game.seat(0).unwrap().judges.contains(&card) {
                "A:j"
            } else if game.resolving.contains(&card) {
                "o"
            } else if game.discard.contains(&card) {
                "d"
            } else if game.classic.as_ref().unwrap().deck.contains(&card) {
                "c"
            } else {
                "?"
            }
        };
        let subject = game.scheduler.active_frame(event.subject).unwrap().1.kind();
        let parent = event
            .parent
            .and_then(|id| game.scheduler.active_frame(id))
            .map(|(_, p)| p.kind());
        out.lock().unwrap().push((
            event.timing.clone(),
            subject,
            parent,
            zone(delayed),
            zone(revealed),
            event.cards.clone(),
        ));
    }));
    start(&mut game, Phase::Judgment, None, 1);
    while let Some(Decision::Cards(choice)) = game.decision.take() {
        assert!(choice.counter.is_some());
        pump(
            &mut game,
            Some(Answer::Select(crate::Selection {
                branch: 0,
                cards: vec![],
                targets: vec![],
            })),
        );
    }
    assert!(game.decision.is_none());
    let rows = observed.lock().unwrap();
    let lose = rows.iter().find(|r| r.0 == "loseAfter").expect("real loss");
    assert!(matches!(lose.1, EventKind::Lose { .. }));
    assert!(matches!(
        lose.2,
        Some(EventKind::Phase {
            phase: Phase::Judgment,
            ..
        })
    ));
    assert_eq!(lose.3, "o");
    let bare = rows.iter().find(|r| r.0 == "phaseJudge").unwrap();
    assert_eq!(bare.3, "o");
    let judge = rows.iter().find(|r| r.0 == "judge").unwrap();
    assert!(matches!(judge.1, EventKind::Judge { .. }));
    assert!(matches!(
        judge.2,
        Some(EventKind::Phase {
            phase: Phase::Judgment,
            ..
        })
    ));
    let fixing = rows.iter().find(|r| r.0 == "judgeFixing").unwrap();
    assert_eq!((fixing.3, fixing.4), ("o", "o"));
    let stage_after = rows.iter().position(|r| r.0 == "phaseJudgeAfter").unwrap();
    let revealed_discard = rows
        .iter()
        .position(|r| r.0 == "cardsDiscardAfter" && r.5 == [revealed])
        .unwrap();
    let delayed_discard = rows
        .iter()
        .position(|r| r.0 == "cardsDiscardAfter" && r.5 == [delayed])
        .unwrap();
    assert!(revealed_discard < stage_after && stage_after < delayed_discard);
    assert_eq!(game.discard.iter().filter(|&&c| c == delayed).count(), 1);
    assert_eq!(game.discard.iter().filter(|&&c| c == revealed).count(), 1);
    let trial = golden["trials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "lebu-fail")
        .unwrap();
    assert_eq!(trial["delayed"], "d");
    assert_eq!(trial["revealed"], "d");
    assert_eq!(trial["skipped"], serde_json::json!(["phaseUse"]));
}

#[test]
fn multiple_delayed_cards_share_one_phase_cleanup_without_duplicates() {
    let mut game = table();
    game.seat_mut(0).hp = 8;
    game.seat_mut(0).max_hp = 8;
    let lebu = card(500, CardKind::Lebu);
    let lightning = card(501, CardKind::Shandian);
    let first_judge = card(502, CardKind::Sha);
    let second_judge = card(503, CardKind::Sha);
    game.seat_mut(0).judges.extend([lebu, lightning]);
    let state = game.classic.as_mut().unwrap();
    for (card, suit, rank) in [
        (lebu, crate::classic_data::Suit::Club, 6),
        (lightning, crate::classic_data::Suit::Spade, 1),
        (first_judge, crate::classic_data::Suit::Spade, 8),
        (second_judge, crate::classic_data::Suit::Club, 8),
    ] {
        state.faces.push(CardFace {
            id: card.id,
            suit,
            rank,
        });
    }
    state.deck.extend([second_judge, first_judge]);
    let batches = Arc::new(Mutex::new(Vec::new()));
    let out = batches.clone();
    game.timing_observer = Some(Arc::new(move |_, event| {
        if event.timing == "cardsDiscardAfter"
            && event
                .cards
                .iter()
                .any(|card| [lebu, lightning].contains(card))
        {
            out.lock().unwrap().push(event.cards.clone());
        }
    }));
    start(&mut game, Phase::Judgment, None, 1);
    while let Some(Decision::Cards(choice)) = game.decision.take() {
        assert!(choice.counter.is_some());
        pump(
            &mut game,
            Some(Answer::Select(crate::Selection {
                branch: 0,
                cards: vec![],
                targets: vec![],
            })),
        );
    }
    assert_eq!(*batches.lock().unwrap(), [vec![lightning, lebu]]);
    for card in [lebu, lightning, first_judge, second_judge] {
        assert_eq!(game.discard.iter().filter(|&&item| item == card).count(), 1);
        assert!(!game.resolving.contains(&card));
    }
    assert_eq!(game.seat(0).unwrap().hp, 5);
}

#[test]
fn terminal_death_during_judgment_loss_still_cleans_the_delayed_material() {
    let mut game = table();
    game.seat_mut(0).hp = 1;
    let delayed = card(500, CardKind::Lebu);
    game.seat_mut(0).judges.push(delayed);
    let mut kill = registration("loseAfter");
    kill.role = TriggerRole::Global;
    kill.filter = Arc::new(move |_, event, _| event.cards.contains(&delayed));
    kill.effect = Arc::new(|ctx, _, _| {
        ctx.lose_hp(0, 1).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(kill).unwrap();
    start(&mut game, Phase::Judgment, None, 1);
    for _ in 0..16 {
        let Some(Decision::Cards(choice)) = game.decision.take() else {
            break;
        };
        assert_eq!(choice.kind, crate::ChoiceKind::Save);
        pump(&mut game, Some(Answer::Pass));
    }
    assert!(game.match_result().is_some());
    assert_eq!(
        game.discard.iter().filter(|&&card| card == delayed).count(),
        1
    );
    assert!(!game.resolving.contains(&delayed));
    assert!(!game.ordering_owners.contains_key(&delayed.id));
}

#[test]
fn jianxiong_takes_lightning_material_before_phase_cleanup() {
    let mut game = table();
    game.seat_mut(0).hp = 8;
    game.seat_mut(0).max_hp = 8;
    game.seat_mut(0).hero = crate::classic_data::roster()
        .iter()
        .find(|hero| hero.id == "caocao");
    let lightning = card(500, CardKind::Shandian);
    let revealed = card(501, CardKind::Sha);
    game.seat_mut(0).judges.push(lightning);
    let state = game.classic.as_mut().unwrap();
    state.faces.push(CardFace {
        id: revealed.id,
        suit: crate::classic_data::Suit::Spade,
        rank: 8,
    });
    state.deck.push(revealed);
    start(&mut game, Phase::Judgment, None, 1);
    let mut accepted = false;
    while let Some(Decision::Cards(choice)) = game.decision.take() {
        let branch = if choice.counter.is_none() {
            accepted = true;
            choice
                .branches
                .iter()
                .find(|branch| branch.id == 1)
                .unwrap()
                .id
        } else {
            0
        };
        pump(
            &mut game,
            Some(Answer::Select(crate::Selection {
                branch,
                cards: vec![],
                targets: vec![],
            })),
        );
    }
    assert!(accepted);
    assert!(game.seat(0).unwrap().hand.contains(&lightning));
    assert!(!game.discard.contains(&lightning));
    assert_eq!(
        game.discard
            .iter()
            .filter(|&&card| card == revealed)
            .count(),
        1
    );
    assert!(!game.ordering_owners.contains_key(&lightning.id));
}

#[test]
fn guicai_replacement_updates_judging_owner_and_tiandu_gain() {
    let mut game = table();
    game.seat_mut(0).hero = crate::classic_data::roster()
        .iter()
        .find(|h| h.id == "guojia");
    game.seat_mut(1).hero = crate::classic_data::roster()
        .iter()
        .find(|h| h.id == "simayi");
    let delayed = card(500, CardKind::Lebu);
    let revealed = card(501, CardKind::Sha);
    let replacement = card(502, CardKind::Tao);
    game.seat_mut(0).judges.push(delayed);
    game.seat_mut(1).hand.push(replacement);
    let state = game.classic.as_mut().unwrap();
    state.faces.push(CardFace {
        id: revealed.id,
        suit: crate::classic_data::Suit::Spade,
        rank: 8,
    });
    state.faces.push(CardFace {
        id: replacement.id,
        suit: crate::classic_data::Suit::Heart,
        rank: 2,
    });
    state.deck.push(revealed);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let out = seen.clone();
    let mut observer = registration("judge");
    observer.role = TriggerRole::Global;
    observer.order = TriggerOrder::Last;
    observer.effect = Arc::new(move |ctx, _, _| {
        out.lock()
            .unwrap()
            .push((ctx.game().judging_card(0), ctx.event().cards.clone()));
        TriggerStep::Done
    });
    game.register_trigger(observer).unwrap();
    start(&mut game, Phase::Judgment, None, 1);
    let mut replaced = false;
    let mut kept = false;
    while let Some(Decision::Cards(choice)) = game.decision.take() {
        let (branch, cards) = if choice.counter.is_some() {
            (0, vec![])
        } else if choice.seat == 1
            && choice
                .branches
                .iter()
                .any(|b| b.cards.contains(&replacement.id))
        {
            replaced = true;
            (1, vec![replacement.id])
        } else {
            kept = true;
            (1, vec![])
        };
        pump(
            &mut game,
            Some(Answer::Select(crate::Selection {
                branch,
                cards,
                targets: vec![],
            })),
        );
    }
    assert!(replaced && kept);
    assert_eq!(*seen.lock().unwrap(), [(Some(replacement), vec![])]);
    assert!(game.seat(0).unwrap().hand.contains(&replacement));
    assert_eq!(game.discard.iter().filter(|&&c| c == revealed).count(), 1);
    assert_eq!(game.discard.iter().filter(|&&c| c == delayed).count(), 1);
    assert!(!game.resolving.contains(&replacement));
    assert!(!game.ordering_owners.contains_key(&replacement.id));
}

#[test]
fn virtual_delayed_trick_preserves_its_effective_kind_after_leaving_judgment_zone() {
    let mut game = table();
    let material = card(500, CardKind::Guohe);
    let revealed = card(501, CardKind::Sha);
    game.put_in_judgment(0, material, CardKind::Lebu);
    let state = game.classic.as_mut().unwrap();
    state.faces.push(CardFace {
        id: revealed.id,
        suit: crate::classic_data::Suit::Club,
        rank: 8,
    });
    state.deck.push(revealed);
    start(&mut game, Phase::Judgment, None, 1);
    while let Some(Decision::Cards(choice)) = game.decision.take() {
        assert!(choice.counter.is_some());
        pump(
            &mut game,
            Some(Answer::Select(crate::Selection {
                branch: 0,
                cards: vec![],
                targets: vec![],
            })),
        );
    }
    assert_eq!(
        game.discard
            .iter()
            .filter(|&&card| card == material)
            .count(),
        1
    );
    start(&mut game, Phase::Play, None, 3);
    assert!(game.decision.is_none());
}

#[test]
fn stale_ordering_cleanup_cannot_discard_a_card_reowned_by_another_event() {
    let mut game = table();
    let material = card(500, CardKind::Sha);
    game.resolving.push(material);
    game.ordering_owners.insert(material.id, 1);
    assert_eq!(game.take_resolving(material.id), material);
    game.resolving.push(material);
    game.ordering_owners.insert(material.id, 2);
    game.scheduler.start(Program::JudgmentCleanup(
        crate::classic::hero_integration::Cleanup::Ordering {
            owner: 1,
            cards: vec![material],
        },
    ));
    pump(&mut game, None);
    assert!(game.resolving.contains(&material));
    assert!(!game.discard.contains(&material));
    assert_eq!(game.ordering_owners.get(&material.id), Some(&2));
    game.scheduler.start(Program::JudgmentCleanup(
        crate::classic::hero_integration::Cleanup::Ordering {
            owner: 2,
            cards: vec![material],
        },
    ));
    pump(&mut game, None);
    assert_eq!(
        game.discard
            .iter()
            .filter(|&&card| card == material)
            .count(),
        1
    );
}
