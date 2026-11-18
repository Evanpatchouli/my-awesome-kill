use super::{frame_name, next_decision, registration, submit};
use crate::classic_data::Suit;
use crate::{
    Action, Card, CardKind, CompareResult, EventKind, EventStatus, Game, RecastKey, RecastLoss,
    RecastRule, RuleError, SeatId, TriggerEvent, TriggerOrder, TriggerRole, TriggerStep,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const A: SeatId = 1;
const B: SeatId = 2;
const SPECTATOR: SeatId = 0;
const NAMES: [&str; 4] = ["aKnown", "aHidden", "bBid", "bOther"];
const OBSERVED: [&str; 9] = [
    "chooseCardAfter",
    "loseAfter",
    "compareCardShowBefore",
    "showCardsAfter",
    "compare",
    "compareFixing",
    "chooseToCompareAfter",
    "chooseToCompareCancelled",
    "cardsDiscardAfter",
];

#[derive(Clone)]
struct Sample {
    name: String,
    known_b: bool,
    cards: Arc<Mutex<Vec<Card>>>,
    rows: Arc<Mutex<Vec<Value>>>,
    returned: Arc<Mutex<Option<CompareResult>>>,
}

impl Sample {
    fn cards(&self) -> [Card; 4] {
        self.cards.lock().unwrap().as_slice().try_into().unwrap()
    }

    fn bids(&self) -> [Card; 2] {
        let [known, hidden, target, _] = self.cards();
        [
            if self.name == "bid-known" {
                known
            } else {
                hidden
            },
            target,
        ]
    }

    fn label(&self, card: Card) -> &'static str {
        let cards = self.cards();
        cards
            .iter()
            .position(|c| *c == card)
            .map_or("other", |i| NAMES[i])
    }

    fn zone(game: &Game, card: Card) -> &'static str {
        let locations: Vec<_> = [
            ("A:h", game.seat(A).unwrap().hand.as_slice()),
            ("B:h", game.seat(B).unwrap().hand.as_slice()),
            ("o", &game.resolving),
            ("d", &game.discard),
        ]
        .into_iter()
        .filter_map(|(name, cards)| cards.contains(&card).then_some(name))
        .collect();
        assert_eq!(
            locations.len(),
            1,
            "a locked bid has exactly one current zone"
        );
        locations[0]
    }

    fn knowledge(&self, game: &Game) -> [Vec<&'static str>; 2] {
        let view = game.view(Some(SPECTATOR)).unwrap();
        [A, B].map(|owner| {
            let mut cards: Vec<_> = view.seats[owner as usize]
                .known_hand
                .iter()
                .map(|&c| self.label(c))
                .collect();
            cards.sort_unstable();
            cards
        })
    }

    fn inside_compare(game: &Game, event: &TriggerEvent) -> bool {
        let mut frame = game.scheduler.active_frame(event.subject);
        while let Some((current, program)) = frame {
            if matches!(program.kind(), EventKind::Compare { .. }) {
                return true;
            }
            frame = current
                .parent
                .and_then(|parent| game.scheduler.active_frame(parent));
        }
        false
    }

    fn record(&self, game: &Game, event: &TriggerEvent, kind: &str) {
        if !OBSERVED.contains(&event.timing.as_str()) || !Self::inside_compare(game, event) {
            return;
        }
        let parent = event.parent.unwrap();
        if event.timing == "loseAfter"
            && !matches!(
                game.scheduler.active_frame(parent).unwrap().1.kind(),
                EventKind::LoseAsync
            )
        {
            return;
        }
        let [left, right] = self.bids();
        let [known_a, known_b] = self.knowledge(game);
        if matches!(
            event.timing.as_str(),
            "loseAfter" | "compareCardShowBefore" | "chooseToCompareCancelled"
        ) {
            let public = game.view(None).unwrap();
            assert!(!public.resolving.contains(&left) && !public.resolving.contains(&right));
            assert!(!public
                .events
                .iter()
                .any(|e| matches!(e.kind, EventKind::CompareReveal { .. })));
        }
        if event.timing == "showCardsAfter" {
            let public = game.view(None).unwrap();
            assert!(public.events.iter().any(|e| matches!(e.kind, EventKind::CompareReveal { cards, .. } if cards == [left,right])));
            assert!([left, right]
                .iter()
                .all(|c| public.card_faces.iter().any(|f| f.id == c.id)));
        }
        if event.timing == "chooseCardAfter" {
            let bid = if event.player == Some(A) { left } else { right };
            assert_eq!(game.selected_compare_card(event.subject), Some(bid));
        }
        if event.timing == "compareCardShowBefore" {
            assert_eq!(game.compare_materials(event.subject), Some([left, right]));
        }
        let seat = event.player.map(|s| {
            if s == A {
                "A"
            } else if s == B {
                "B"
            } else {
                "other"
            }
        });
        let event_cards: Vec<_> = event.cards.iter().map(|&c| self.label(c)).collect();
        self.rows.lock().unwrap().push(json!([
            self.name,
            kind,
            event.timing,
            frame_name(game, event.subject),
            frame_name(game, parent),
            seat,
            Self::zone(game, left),
            Self::zone(game, right),
            known_a,
            known_b,
            event_cards
        ]));
    }

    fn table(&self) -> Game {
        let mut game = Game::classic(5, SPECTATOR, "guanyu", 8).unwrap();
        // Face-up hand-to-hand gains supply the same prior public knowledge as the
        // fixture's gain2. Both this movement and the private created gains use real content.
        game.register_recast(
            RecastKey::Kind(CardKind::Tao),
            RecastRule {
                loss: Arc::new(|state, _, card| RecastLoss::Give {
                    to: if state.face(card.id).unwrap().rank == 11 {
                        A
                    } else {
                        B
                    },
                }),
                ..RecastRule::default()
            },
        );
        let setup = self.clone();
        game.register_trigger(registration(
            A,
            "reference_fixture_use",
            "phaseUseBegin",
            TriggerOrder::First,
            move |ctx, stage, _| match stage {
                0 => {
                    for seat in [A, B] {
                        let hand: Vec<_> = ctx.game().view(Some(seat)).unwrap().seats
                            [seat as usize]
                            .hand
                            .as_ref()
                            .unwrap()
                            .iter()
                            .map(|c| c.id)
                            .collect();
                        ctx.discard_hand(seat, &hand).unwrap();
                    }
                    let id = ctx
                        .gain_created_card(SPECTATOR, CardKind::Tao, Suit::Heart, 11, None)
                        .unwrap();
                    setup.cards.lock().unwrap().push(Card {
                        id,
                        kind: CardKind::Tao,
                    });
                    TriggerStep::Continue(1)
                }
                1 => {
                    ctx.recast(SPECTATOR, setup.cards.lock().unwrap()[0].id)
                        .unwrap();
                    for (to, kind, suit, rank) in [
                        (A, CardKind::Sha, Suit::Spade, 10),
                        (B, CardKind::Shan, Suit::Diamond, 5),
                        (
                            if setup.known_b { SPECTATOR } else { B },
                            CardKind::Tao,
                            Suit::Heart,
                            2,
                        ),
                    ] {
                        let id = ctx.gain_created_card(to, kind, suit, rank, None).unwrap();
                        setup.cards.lock().unwrap().push(Card { id, kind });
                    }
                    TriggerStep::Continue(2)
                }
                2 => {
                    if setup.known_b {
                        ctx.recast(SPECTATOR, setup.cards()[3].id).unwrap();
                    }
                    TriggerStep::Continue(3)
                }
                3 => {
                    let known_b = if setup.known_b {
                        vec!["bOther"]
                    } else {
                        vec![]
                    };
                    assert_eq!(setup.knowledge(ctx.game()), [vec!["aKnown"], known_b]);
                    ctx.compare(A, B).unwrap();
                    TriggerStep::Continue(4)
                }
                _ => {
                    *setup.returned.lock().unwrap() = ctx.compare_result();
                    TriggerStep::Done
                }
            },
        ))
        .unwrap();
        let request = self.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            request.record(game, event, "request")
        }));
        for timing in OBSERVED.into_iter().chain(["loseBefore"]) {
            let probe = self.clone();
            let mut reg = registration(
                A,
                "reference_compare_boundary",
                timing,
                TriggerOrder::First,
                move |ctx, _, _| {
                    probe.record(ctx.game(), ctx.event(), "effect");
                    if probe.name == "cancel-before-reveal" && timing == "compareCardShowBefore" {
                        return TriggerStep::CancelSubject;
                    }
                    if timing == "loseBefore"
                        && ctx.event().player == Some(B)
                        && matches!(
                            ctx.game()
                                .scheduler
                                .active_frame(ctx.event().parent.unwrap())
                                .unwrap()
                                .1
                                .kind(),
                            EventKind::LoseAsync
                        )
                    {
                        if probe.name == "cancel-second-loss" {
                            return TriggerStep::CancelSubject;
                        }
                        if probe.name == "discard-intercept" {
                            ctx.discard_hand(B, &[probe.bids()[1].id]).unwrap();
                        }
                    }
                    if timing == "chooseToCompareAfter" && probe.name == "cancel-second-loss" {
                        let id = probe.bids()[1].id;
                        assert!(!ctx.game().can_gain_compare_card(ctx.event().subject, A, id));
                        assert_eq!(ctx.gain_compare_card(A, id), Err(RuleError::InvalidCard));
                    }
                    TriggerStep::Done
                },
            );
            reg.role = TriggerRole::Global;
            reg.filter = Arc::new(|game, event, _| Self::inside_compare(game, event));
            game.register_trigger(reg).unwrap();
        }
        if self.name == "discard-intercept" {
            let award = self.clone();
            game.register_trigger(registration(
                A,
                "jyzongshi",
                "chooseToCompareAfter",
                TriggerOrder::Normal,
                move |ctx, stage, _| {
                    let bid = award.bids()[1];
                    assert!(ctx.game().comparison(ctx.event().subject).unwrap().won);
                    if stage == 0 {
                        assert!(ctx.game().discard.contains(&bid));
                        ctx.gain_compare_card(A, bid.id).unwrap();
                        return TriggerStep::Continue(1);
                    }
                    assert!(!ctx
                        .game()
                        .can_gain_compare_card(ctx.event().subject, A, bid.id));
                    TriggerStep::Done
                },
            ))
            .unwrap();
        }
        game
    }

    fn play(name: &str, known_b: bool) -> (Self, Game, bool) {
        let trial = Sample {
            name: name.into(),
            known_b,
            cards: Default::default(),
            rows: Default::default(),
            returned: Default::default(),
        };
        let mut game = trial.table();
        let mut cancelled = false;
        for _ in 0..50 {
            let public = game.view(None).unwrap();
            cancelled |= public.events.iter().any(|e| {
                e.status == EventStatus::Cancelled && matches!(e.kind, EventKind::Compare { .. })
            });
            let actor = public.waiting_for.unwrap();
            let view = game.view(Some(actor)).unwrap();
            if actor == A && view.actions.contains(&Action::EndPlay) {
                assert!(game.resolving.is_empty());
                return (trial, game, cancelled);
            }
            if let Some(choice) = view.choice.filter(|c| c.compare.is_some()) {
                let id = trial.bids()[usize::from(actor == B)].id;
                assert!(choice.branches[0].cards.contains(&id));
                submit(
                    &mut game,
                    actor,
                    Action::Select {
                        event: choice.event,
                        branch: choice.branches[0].id,
                        cards: vec![id],
                        targets: vec![],
                    },
                );
            } else {
                next_decision(&mut game, &[]);
            }
        }
        panic!("comparison did not settle");
    }
}

#[test]
fn known_and_hidden_bids_and_both_cancellation_boundaries_match_all_78_reference_observations() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/reference/compare-boundaries.json"
    ))
    .unwrap();
    let mut actual = Vec::new();
    for expected in golden["trials"].as_array().unwrap() {
        let name = expected["sample"].as_str().unwrap();
        let (trial, game, cancelled) = Sample::play(name, false);
        let bids = trial.bids();
        let result = *trial.returned.lock().unwrap();
        assert_eq!(game.compare_history().last().copied(), result);
        assert_eq!(
            json!({ "sample":name,"initialKnowledge":[["aKnown"],[]], "chosen":trial.label(bids[0]),
            "finalZones":bids.map(|c|Sample::zone(&game,c)), "finalKnowledge":trial.knowledge(&game), "cancelled":cancelled,
            "hasResult":result.is_some(), "won":result.map(|r|r.won), "points":result.map(|r|r.numbers) }),
            *expected
        );
        actual.extend(trial.rows.lock().unwrap().iter().cloned());
    }
    assert_eq!(actual.len(), 78);
    assert_eq!(
        actual.as_slice(),
        golden["events"].as_array().unwrap().as_slice()
    );
}

#[test]
fn another_payers_known_card_survives_until_that_seat_pays() {
    for name in ["bid-hidden", "cancel-second-loss"] {
        let (trial, _, _) = Sample::play(name, true);
        let rows = trial.rows.lock().unwrap();
        assert!(rows.iter().any(|r| r[2] == "loseAfter"
            && r[5] == "A"
            && r[8] == json!([])
            && r[9] == json!(["bOther"])));
        let final_b = if name == "cancel-second-loss" {
            json!(["bOther"])
        } else {
            json!([])
        };
        assert_eq!(rows.last().unwrap()[9], final_b);
    }
}

#[test]
fn a_locked_bid_intercepted_into_discard_can_be_awarded_once_after_compare() {
    let (trial, game, _) = Sample::play("discard-intercept", false);
    let [left, right] = trial.bids();
    assert_eq!(Sample::zone(&game, left), "d");
    assert_eq!(Sample::zone(&game, right), "A:h");
    assert_eq!(trial.knowledge(&game), [vec!["bBid"], vec![]]);
    assert_eq!(game.compare_history()[0].cards, [left, right]);
}
