use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, ActiveSkillRegistration, ChoiceBranch, Command, Phase, TriggerOrder,
    TriggerRegistration, TriggerRole, TriggerStep,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Mutex;

// Compare only observation keys that the upstream harness actually sampled. The
// keys include category and both event types; filtering never changes the order
// of the remaining observations, and the resulting vectors must match exactly.
#[derive(Clone)]
struct Trace {
    keys: Arc<HashSet<(String, String, String, String)>>,
    rows: Arc<Mutex<Vec<Value>>>,
    cards: [Card; 7],
}

fn kind_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Recast { .. } => "recast",
        EventKind::LoseToDiscardpile { .. } => "loseToDiscardpile",
        EventKind::Lose { .. } => "lose",
        EventKind::Draw { .. } => "draw",
        EventKind::Gain { .. } => "gain",
        EventKind::ActivateSkill { .. } | EventKind::ActiveSkillEffect { .. } => "fixture-use",
        EventKind::RegisteredTrigger { .. } => "observer",
        _ => "other",
    }
}

fn ref_kind(name: &str) -> &str {
    if name == "reference_fixture_use" {
        "fixture-use"
    } else if name.starts_with("reference_recast_batch_") {
        "observer"
    } else {
        name
    }
}

impl Trace {
    fn names(&self, cards: &[Card]) -> Vec<&'static str> {
        const NAMES: [&str; 7] = [
            "shown", "hidden", "equipped", "control", "draw-1", "draw-2", "draw-3",
        ];
        cards
            .iter()
            .map(|card| {
                NAMES[self
                    .cards
                    .iter()
                    .position(|c| c == card)
                    .expect("fixture card")]
            })
            .collect()
    }
    fn snapshot(
        &self,
        game: &Game,
        category: &str,
        timing: &str,
        subject: &str,
        parent: &str,
        cards: &[Card],
        event: Option<EventId>,
    ) {
        let key = (
            category.to_owned(),
            timing.to_owned(),
            subject.to_owned(),
            parent.to_owned(),
        );
        if !self.keys.contains(&key) {
            return;
        }
        let zones: Vec<Option<&str>> = self
            .cards
            .iter()
            .map(|c| {
                if game.seat(0).unwrap().hand.contains(c) {
                    Some("A:h")
                } else if game.seat(0).unwrap().equipment.contains(c) {
                    Some("A:e")
                } else if game.seat(1).unwrap().hand.contains(c) {
                    Some("B:h")
                } else if game.discard.contains(c) {
                    Some("d")
                } else if game.transferring.contains(c) {
                    Some("s")
                } else if game.classic.as_ref().unwrap().deck.contains(c) {
                    Some("c")
                } else {
                    None
                }
            })
            .collect();
        let view = game.view(Some(2)).unwrap();
        let known = [
            self.names(&view.seats[0].known_hand),
            self.names(&view.seats[1].known_hand),
        ];
        let stock = event
            .and_then(|id| game.loss_record(id))
            .map(|r| self.names(&r.stockcards));
        let hands = [
            self.names(&game.seat(0).unwrap().hand),
            self.names(&game.seat(1).unwrap().hand),
        ];
        self.rows.lock().unwrap().push(json!({
            "key": [category, timing, subject, parent],
            "zones": zones,
            "cards": self.names(cards),
            "stock": stock,
            "known": known,
            "hands": hands,
            "discard": self.names(&game.discard),
        }));
    }
    fn event(&self, game: &Game, category: &str, e: &crate::TriggerEvent) {
        let Some((_, subject)) = game.scheduler.active_frame(e.subject) else {
            return;
        };
        let Some((_, parent)) = e.parent.and_then(|id| game.scheduler.active_frame(id)) else {
            return;
        };
        self.snapshot(
            game,
            category,
            &e.timing,
            kind_name(subject.kind()),
            kind_name(parent.kind()),
            &e.cards,
            Some(e.subject),
        );
    }
}

#[test]
fn upstream_batch_observations_match_real_eight_seat_submission() {
    let fixtures: [(Value, &[&str]); 2] = [
        (
            serde_json::from_str(include_str!(
                "../../../../../tests/reference/recast-batch.json"
            ))
            .unwrap(),
            &[
                "default",
                "intercept-one",
                "cancel-loss",
                "cancel-draw",
                "public-gain",
            ],
        ),
        (
            serde_json::from_str(include_str!(
                "../../../../../tests/reference/recast-batch-boundaries.json"
            ))
            .unwrap(),
            &["cancel-loss-wrapper", "intercept-all"],
        ),
    ];
    for (fixture, cases) in fixtures {
        let columns = fixture["eventColumns"].as_array().unwrap();
        let column = |name: &str| {
            columns
                .iter()
                .position(|column| column.as_str() == Some(name))
        };
        let zones_column = column("zones").unwrap();
        let detail_column = column("detail").unwrap();
        let observer_hands_column = column("spectatorHands");
        for &case in cases {
            let raw: Vec<&Value> = fixture["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| {
                    row[0] == case
                        && ["request", "callback", "factory"].contains(&row[1].as_str().unwrap())
                })
                .collect();
            let expected_counts = match case {
                "default" => 30,
                "intercept-one" => 37,
                "cancel-loss" => 30,
                "cancel-draw" => 27,
                "public-gain" => 26,
                "cancel-loss-wrapper" => 21,
                "intercept-all" => 40,
                _ => unreachable!(),
            };
            assert_eq!(raw.len(), expected_counts, "{case}: fixture range changed");
            let keys: HashSet<_> = raw
                .iter()
                .map(|r| {
                    (
                        r[1].as_str().unwrap().to_owned(),
                        r[2].as_str().unwrap().to_owned(),
                        ref_kind(r[3].as_str().unwrap()).to_owned(),
                        ref_kind(r[4].as_str().unwrap()).to_owned(),
                    )
                })
                .collect();
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
            let cards = [
                card(1, CardKind::Tiesuo),
                card(2, CardKind::Tiesuo),
                card(3, CardKind::Qinggang),
                card(4, CardKind::Sha),
                card(109, CardKind::Tao),
                card(108, CardKind::Shan),
                card(107, CardKind::Sha),
            ];
            game.seat_mut(0).hand.extend([cards[0], cards[3], cards[1]]);
            game.seat_mut(0).equipment.push(cards[2]);
            game.known_to_all.extend([cards[0].id, cards[3].id]);
            for c in &cards[4..] {
                let slot = game
                    .classic
                    .as_mut()
                    .unwrap()
                    .deck
                    .iter_mut()
                    .find(|x| x.id == c.id)
                    .unwrap();
                *slot = *c;
            }
            let trace = Trace {
                keys: Arc::new(keys),
                rows: Arc::default(),
                cards,
            };
            let loss = trace.clone();
            let gain = trace.clone();
            let rule = RecastBatchRule {
                loss: Arc::new(move |state, _, selected| {
                    loss.snapshot(
                        state.0,
                        "factory",
                        "recastingLose",
                        "recast",
                        "fixture-use",
                        selected,
                        None,
                    );
                    RecastLoss::Discard
                }),
                gain: Arc::new(move |state, _, selected, _| {
                    gain.snapshot(
                        state.0,
                        "factory",
                        "recastingGain",
                        "recast",
                        "fixture-use",
                        selected,
                        None,
                    );
                    if case == "public-gain" {
                        RecastBatchGain::FromDeck {
                            cards: gain.cards[4..].iter().map(|c| c.id).collect(),
                        }
                    } else {
                        RecastBatchGain::Draw
                    }
                }),
            };
            let active = game
                .register_active_skill(ActiveSkillRegistration {
                    owner: 0,
                    skill: "reference_batch_trace".into(),
                    limited: false,
                    choice: Arc::new(move |_, _| {
                        Some(ChoiceBranch {
                            id: 0,
                            label: "recast".into(),
                            cards: cards[..3].iter().map(|c| c.id).collect(),
                            min_cards: 3,
                            max_cards: 3,
                            targets: vec![],
                            min_targets: 0,
                            max_targets: 0,
                        })
                    }),
                    effect: Arc::new(move |ctx, _, selection| {
                        ctx.recast_batch(0, &selection.unwrap().cards, rule.clone())
                            .unwrap();
                        TriggerStep::Done
                    }),
                })
                .unwrap();
            let requests = trace.clone();
            game.timing_observer = Some(Arc::new(move |game, event| {
                requests.event(game, "request", event)
            }));
            let timings: HashSet<_> = raw
                .iter()
                .filter(|r| r[1] == "callback")
                .map(|r| r[2].as_str().unwrap().to_owned())
                .chain([
                    "loseBefore".into(),
                    "drawBefore".into(),
                    "loseToDiscardpileBegin".into(),
                ])
                .collect();
            for timing in timings {
                let callbacks = trace.clone();
                let timing_copy = timing.clone();
                game.register_trigger(TriggerRegistration {
                    owner: 2,
                    skill: format!("reference_batch_{timing}"),
                    timing,
                    role: TriggerRole::Global,
                    order: TriggerOrder::Normal,
                    priority: 0,
                    label: None,
                    silent: true,
                    filter: Arc::new(|_, _, _| true),
                    effect: Arc::new(move |ctx, _, _| {
                        callbacks.event(ctx.game(), "callback", ctx.event());
                        let event = ctx.event();
                        let subject = ctx
                            .game()
                            .scheduler
                            .active_frame(event.subject)
                            .unwrap()
                            .1
                            .kind();
                        let parent = event
                            .parent
                            .and_then(|id| ctx.game().scheduler.active_frame(id))
                            .map(|(_, p)| p.kind());
                        let outer = matches!(subject, EventKind::Lose { seat: 0, .. })
                            && matches!(parent, Some(EventKind::LoseToDiscardpile { .. }));
                        if timing_copy == "loseBefore" && outer {
                            match case {
                                "intercept-one" => ctx.give_hand(0, 1, cards[1].id).unwrap(),
                                "intercept-all" => ctx
                                    .give_cards(
                                        0,
                                        1,
                                        &cards[..3].iter().map(|c| c.id).collect::<Vec<_>>(),
                                        false,
                                    )
                                    .unwrap(),
                                "cancel-loss" => return TriggerStep::CancelSubject,
                                _ => {}
                            }
                        }
                        if case == "cancel-loss-wrapper" && timing_copy == "loseToDiscardpileBegin"
                        {
                            return TriggerStep::CancelSubject;
                        }
                        if case == "cancel-draw"
                            && timing_copy == "drawBefore"
                            && matches!(subject, EventKind::Draw { seat: 0, .. })
                        {
                            return TriggerStep::CancelSubject;
                        }
                        TriggerStep::Done
                    }),
                })
                .unwrap();
            }
            game.scheduler.start(Program::event(EventKind::Phase {
                seat: 0,
                phase: Phase::Play,
            }));
            game.run(None);
            game.submit(Command {
                version: game.version,
                seat: 0,
                action: Action::ActivateSkill {
                    registration: active,
                    cards: cards[..3].iter().map(|c| c.id).collect(),
                    targets: vec![],
                },
            })
            .unwrap();
            let actual = trace.rows.lock().unwrap();
            assert_eq!(
                actual.len(),
                raw.len(),
                "{case}: different sampled observation count\n{actual:#?}"
            );
            for (i, (a, r)) in actual.iter().zip(raw.iter()).enumerate() {
                let detail = &r[detail_column];
                let key = json!([
                    r[1],
                    r[2],
                    ref_kind(r[3].as_str().unwrap()),
                    ref_kind(r[4].as_str().unwrap())
                ]);
                assert_eq!(a["key"], key, "{case} row {i}: sequence");
                assert_eq!(a["zones"], r[zones_column], "{case} row {i}: zones ({key})");
                if let Some(expected) = detail.get("cards") {
                    assert_eq!(&a["cards"], expected, "{case} row {i}: cards ({key})");
                }
                if let Some(expected) = detail
                    .get("stockcards")
                    .or_else(|| detail.get("stockCards"))
                {
                    assert_eq!(&a["stock"], expected, "{case} row {i}: stockcards ({key})");
                }
                let known = observer_hands_column
                    .map(|column| &r[column])
                    .or_else(|| detail.get("spectatorKnownHands"));
                if let Some(expected) = known {
                    assert_eq!(
                        &a["known"], expected,
                        "{case} row {i}: spectator knowledge ({key})"
                    );
                }
                if let Some(expected) = detail.get("hands") {
                    assert_eq!(&a["hands"], expected, "{case} row {i}: hands ({key})");
                }
                if let Some(expected) = detail.get("discardOrder") {
                    assert_eq!(&a["discard"], expected, "{case} row {i}: discard ({key})");
                }
            }
        }
    }
}
