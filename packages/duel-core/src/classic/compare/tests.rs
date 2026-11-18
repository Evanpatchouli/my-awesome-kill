use crate::classic_data::Suit;
use crate::{
    Action, Card, CardKind, ChoiceBranch, ChoiceKind, ChoiceView, Command, CompareParticipants,
    CompareResult, EventId, EventKind, EventStatus, Game, GameEvent, RuleError, SeatId,
    TriggerContext, TriggerOrder, TriggerRegistration, TriggerRole, TriggerSelection, TriggerStep,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

// Seventeen request rows per trial (51 of the fixture's 104). The other rows
// include public display, skill bodies, target costs and later use/phase timings.
const TIMINGS: &[&str] = &[
    "chooseToCompareBegin",
    "chooseCardOLBegin",
    "chooseCardBegin",
    "chooseCardAfter",
    "chooseCardOLAfter",
    "loseAfter",
    "compareCardShowBefore",
    "showCardsAfter",
    "compare",
    "compareFixing",
    "chooseToCompareAfter",
    "gainAfter",
    "cardsDiscardAfter",
];

#[derive(Default)]
struct Observations {
    hands: Option<[[Card; 2]; 2]>,
    subject: Option<EventId>,
    rows: Vec<Value>,
    returned: Option<CompareResult>,
    choices: Vec<SeatId>,
}

#[derive(Clone)]
struct Trial {
    a: SeatId,
    b: SeatId,
    turn: u64,
    sample: Value,
    log: Arc<Mutex<Observations>>,
}

fn registration(
    owner: SeatId,
    skill: &str,
    timing: &str,
    order: TriggerOrder,
    effect: impl Fn(&mut TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) -> TriggerRegistration {
    TriggerRegistration {
        owner,
        skill: skill.into(),
        timing: timing.into(),
        role: TriggerRole::Player,
        order,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(effect),
    }
}

fn branch(id: u32, label: &str) -> ChoiceBranch {
    ChoiceBranch {
        id,
        label: label.into(),
        cards: vec![],
        min_cards: 0,
        max_cards: 0,
        targets: vec![],
        min_targets: 0,
        max_targets: 0,
    }
}

fn submit(game: &mut Game, seat: SeatId, action: Action) {
    let version = game.view(None).unwrap().version;
    game.submit(Command {
        version,
        seat,
        action,
    })
    .unwrap();
}

fn frame_name(game: &Game, id: EventId) -> String {
    match game
        .scheduler
        .active_frame(id)
        .expect("live timing parent")
        .1
        .kind()
    {
        EventKind::Compare { .. } => "chooseToCompare",
        EventKind::CompareChoices { .. } => "chooseCardOL",
        EventKind::CompareBid { .. } => "chooseCard",
        EventKind::ShowCards { .. } => "showCards",
        EventKind::LoseAsync => "loseAsync",
        EventKind::Lose { .. } => "lose",
        EventKind::Gain { .. } => "gain",
        EventKind::OrderingDiscard => "orderingDiscard",
        EventKind::CardsDiscard { .. } => "cardsDiscard",
        EventKind::RegisteredTrigger { registration, .. } => {
            return game.trigger_registrations[&registration].skill.clone()
        }
        other => panic!("unexpected comparison subject/parent {other:?}"),
    }
    .into()
}

impl Trial {
    fn install(&self, game: &mut Game) {
        let trial = self.clone();
        game.register_trigger(registration(
            self.a,
            "reference_setup",
            "phaseUseBegin",
            TriggerOrder::First,
            move |ctx, _, _| {
                let mut hands = Vec::new();
                for (i, seat) in [trial.a, trial.b].into_iter().enumerate() {
                    let old: Vec<_> = ctx.game().view(Some(seat)).unwrap().seats[seat as usize]
                        .hand
                        .as_ref()
                        .unwrap()
                        .iter()
                        .map(|c| c.id)
                        .collect();
                    ctx.discard_hand(seat, &old).unwrap();
                    let mut given = Vec::new();
                    for physical in trial.sample["hands"][i].as_array().unwrap() {
                        let kind: CardKind = serde_json::from_value(physical[0].clone()).unwrap();
                        let suit: Suit = serde_json::from_value(physical[1].clone()).unwrap();
                        let id = ctx
                            .gain_created_card(
                                seat,
                                kind,
                                suit,
                                physical[2].as_u64().unwrap() as u8,
                                None,
                            )
                            .unwrap();
                        given.push(Card { id, kind });
                    }
                    hands.push(given.try_into().unwrap());
                }
                trial.log.lock().unwrap().hands = Some(hands.try_into().unwrap());
                TriggerStep::Done
            },
        ))
        .unwrap();

        let trial = self.clone();
        game.register_trigger(registration(
            self.a,
            "qiaoshui",
            "phaseUseBegin",
            TriggerOrder::Normal,
            move |ctx, stage, selected| match stage {
                0 => {
                    assert_eq!(ctx.compare_result(), None);
                    assert_eq!(ctx.compare(trial.a, trial.a), Err(RuleError::InvalidTarget));
                    let mut choose = branch(1, "选择拼点对手");
                    choose.targets = ctx
                        .game()
                        .seat_order_from(trial.a)
                        .unwrap()
                        .into_iter()
                        .filter(|&seat| ctx.game().can_compare(trial.a, seat))
                        .collect();
                    choose.min_targets = 1;
                    choose.max_targets = 1;
                    TriggerStep::Choose {
                        branches: vec![branch(0, "放弃"), choose],
                        then: 1,
                    }
                }
                1 => {
                    let selection = selected.unwrap();
                    if selection.branch == 0 {
                        return TriggerStep::Done;
                    }
                    ctx.compare(trial.a, selection.targets[0]).unwrap();
                    TriggerStep::Continue(2)
                }
                2 => {
                    let result = ctx.compare_result().unwrap();
                    assert_eq!(ctx.game().compare_history().last(), Some(&result));
                    assert!(result
                        .cards
                        .iter()
                        .all(|c| !ctx.game().resolving.contains(c)));
                    let gain = usize::from(result.won);
                    for viewer in (0..8).map(Some).chain([None]) {
                        let view = ctx.game().view(viewer).unwrap();
                        assert_eq!(
                            view.seats[trial.a as usize].known_hand,
                            [result.cards[gain]]
                        );
                        assert!(view.discard.contains(&result.cards[1 - gain]));
                        assert!(result
                            .cards
                            .iter()
                            .all(|c| view.card_faces.iter().any(|f| f.id == c.id)));
                    }
                    trial.log.lock().unwrap().returned = Some(result);
                    crate::qiaoshui::grant_result(ctx, result).unwrap();
                    TriggerStep::Done
                }
                _ => unreachable!(),
            },
        ))
        .unwrap();

        if self.sample["sample"] == "modified-win" {
            game.register_trigger(registration(
                self.a,
                "tianbian_number",
                "compare",
                TriggerOrder::Normal,
                |ctx, _, _| {
                    let result = *ctx.game().comparison(ctx.event().subject).unwrap();
                    assert_eq!(
                        ctx.game().face(result.cards[0].id).unwrap().suit,
                        Suit::Heart
                    );
                    assert_eq!(result.numbers, [7, 9]);
                    ctx.set_compare_number(ctx.owner(), 13).unwrap();
                    assert_eq!(
                        ctx.game().comparison(ctx.event().subject).unwrap().numbers,
                        [13, 9]
                    );
                    TriggerStep::Done
                },
            ))
            .unwrap();
        }
        let trial = self.clone();
        let mut regain = registration(
            self.a,
            "jyzongshi",
            "chooseToCompareAfter",
            TriggerOrder::Normal,
            move |ctx, stage, selected| {
                let result = *ctx.game().comparison(ctx.event().subject).unwrap();
                let gain = result.cards[usize::from(result.winner == Some(trial.a))];
                assert!(ctx
                    .game()
                    .can_gain_compare_card(result.event, trial.a, gain.id));
                assert_eq!(
                    ctx.set_compare_number(trial.a, 2),
                    Err(RuleError::WrongEvent)
                );
                if stage == 0 {
                    return TriggerStep::Choose {
                        branches: vec![branch(0, "放弃"), branch(1, "获得拼点牌")],
                        then: 1,
                    };
                }
                if selected.unwrap().branch == 1 {
                    assert_eq!(
                        ctx.gain_compare_card(trial.a, u32::MAX),
                        Err(RuleError::InvalidCard)
                    );
                    ctx.gain_compare_card(trial.a, gain.id).unwrap();
                    assert_eq!(
                        ctx.gain_compare_card(trial.a, gain.id),
                        Err(RuleError::InvalidSelection)
                    );
                }
                TriggerStep::Done
            },
        );
        regain.filter = Arc::new(|game, event, owner| {
            game.comparison(event.subject).is_some_and(|r| {
                let card = r.cards[usize::from(r.winner == Some(owner))];
                game.can_gain_compare_card(r.event, owner, card.id)
            })
        });
        game.register_trigger(regain).unwrap();

        for &timing in TIMINGS {
            let trial = self.clone();
            let mut observation = registration(
                0,
                "reference_observer",
                timing,
                TriggerOrder::First,
                move |ctx, _, _| {
                    trial.observe(ctx);
                    TriggerStep::Done
                },
            );
            let turn = self.turn;
            observation.role = TriggerRole::Global;
            observation.filter = Arc::new(move |game, _, _| {
                game.classic.as_ref().unwrap().turn.unwrap().number == turn
            });
            game.register_trigger(observation).unwrap();
        }
    }

    fn observe(&self, ctx: &TriggerContext<'_>) {
        let event = ctx.event();
        let game = ctx.game();
        let mut log = self.log.lock().unwrap();
        if log.returned.is_some() {
            return;
        }
        let Some(hands) = log.hands else { return };
        if event.timing == "chooseToCompareBegin" {
            log.subject = Some(event.subject);
        }
        let Some(subject) = log.subject else { return };
        let bids = [hands[0][0], hands[1][0]];
        if matches!(event.timing.as_str(), "loseAfter" | "gainAfter")
            && !event.cards.iter().any(|c| bids.contains(c))
        {
            return;
        }
        let zone = |card| {
            if game.seat(self.a).unwrap().hand.contains(&card) {
                "A:h"
            } else if game.seat(self.b).unwrap().hand.contains(&card) {
                "B:h"
            } else if game.resolving.contains(&card) {
                "o"
            } else if game.discard.contains(&card) {
                "d"
            } else {
                panic!("a compared physical card left all expected zones")
            }
        };
        if matches!(event.timing.as_str(), "chooseCardBegin" | "chooseCardAfter") {
            assert_eq!(
                game.compare_choice(event.subject),
                Some(CompareParticipants {
                    initiator: self.a,
                    target: self.b
                })
            );
        }
        if matches!(event.timing.as_str(), "compareCardShowBefore" | "loseAfter") {
            for observer in [None, Some(0)] {
                let view = game.view(observer).unwrap();
                assert!(bids.iter().all(|card| !view.resolving.contains(card)
                    && !view.card_faces.iter().any(|face| face.id == card.id)));
                assert!(!view
                    .events
                    .iter()
                    .any(|e| matches!(e.kind, EventKind::CompareReveal { .. })));
            }
        }
        let scores = game.comparison(subject).map(|r| r.numbers);
        let player = event.player.map(|seat| {
            if seat == self.a {
                "A"
            } else if seat == self.b {
                "B"
            } else {
                "other"
            }
        });
        log.rows.push(json!([
            event.timing,
            frame_name(game, event.subject),
            frame_name(game, event.parent.unwrap()),
            player,
            zone(bids[0]),
            zone(bids[1]),
            scores.map(|n| n[0]),
            scores.map(|n| n[1])
        ]));
    }

    fn bid(&self, game: &mut Game, choice: &ChoiceView) {
        let mut log = self.log.lock().unwrap();
        let hands = log.hands.unwrap();
        log.choices.push(choice.seat);
        drop(log);
        let index = usize::from(choice.seat == self.b);
        assert_eq!(choice.kind, ChoiceKind::Content);
        assert_eq!(
            choice.compare,
            Some(CompareParticipants {
                initiator: self.a,
                target: self.b
            })
        );
        assert_eq!(choice.branches[0].cards, hands[index].map(|c| c.id));
        for viewer in (0..8).map(Some).chain([None]) {
            let view = game.view(viewer).unwrap();
            assert_eq!(view.choice.is_some(), viewer == Some(choice.seat));
            assert!(view.private_choice_cards.is_empty() && view.actions.is_empty());
            for (i, owner) in [self.a, self.b].into_iter().enumerate() {
                let projected = &view.seats[owner as usize];
                assert_eq!(projected.hand_count, 2);
                assert_eq!(
                    projected.hand.as_deref(),
                    (viewer == Some(owner)).then_some(hands[i].as_slice())
                );
                assert!(projected.known_hand.is_empty());
                for card in hands[i] {
                    assert_eq!(
                        view.card_faces.iter().any(|face| face.id == card.id),
                        viewer == Some(owner)
                    );
                }
            }
            assert!(!view
                .events
                .iter()
                .any(|e| matches!(e.kind, EventKind::CompareReveal { .. })));
        }
        let before = game.view(Some(choice.seat)).unwrap();
        let ai = game
            .bot_command()
            .expect("both participants are ordinary AI seats");
        assert_eq!(
            ai.action,
            Action::Select {
                event: choice.event,
                branch: 1,
                cards: vec![hands[index][0].id],
                targets: vec![]
            }
        );
        assert_eq!(
            game.submit(Command {
                seat: 0,
                ..ai.clone()
            }),
            Err(RuleError::WrongActor {
                expected: choice.seat
            })
        );
        assert!(matches!(
            game.submit(Command {
                version: ai.version - 1,
                ..ai.clone()
            }),
            Err(RuleError::WrongVersion { .. })
        ));
        for (id, cards, error) in [
            (
                choice.event - 1,
                vec![hands[index][0].id],
                RuleError::WrongEvent,
            ),
            (choice.event, vec![], RuleError::InvalidSelection),
            (
                choice.event,
                vec![hands[1 - index][0].id],
                RuleError::InvalidSelection,
            ),
            (
                choice.event,
                vec![hands[index][0].id; 2],
                RuleError::InvalidSelection,
            ),
        ] {
            assert_eq!(
                game.submit(Command {
                    action: Action::Select {
                        event: id,
                        branch: 1,
                        cards,
                        targets: vec![]
                    },
                    ..ai.clone()
                }),
                Err(error)
            );
        }
        assert_eq!(game.view(Some(choice.seat)).unwrap(), before);
        game.submit(ai).unwrap();
    }
}

fn next_decision(game: &mut Game, trials: &[Trial]) {
    let public = game.view(None).unwrap();
    let actor = public.waiting_for.unwrap();
    let turn = public.turn.unwrap().number;
    let trial = trials.iter().find(|t| t.turn == turn);
    let Some(choice) = game.view(Some(actor)).unwrap().choice else {
        submit(game, actor, Action::EndPlay);
        return;
    };
    if choice.compare.is_some() {
        trial.unwrap().bid(game, &choice);
        return;
    }
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
            if let Some(cost) = choice.branches.iter().find(|b| b.label == "选择拼点对手") {
                let t = trial.unwrap();
                assert!(cost.targets.contains(&t.b) && !cost.targets.contains(&t.a));
                Action::Select {
                    event: choice.event,
                    branch: cost.id,
                    targets: vec![t.b],
                    cards: vec![],
                }
            } else if choice.branches.iter().any(|b| b.label == "获得拼点牌") {
                let trial = trial.unwrap();
                let bids = trial.log.lock().unwrap().hands.unwrap().map(|h| h[0]);
                for viewer in (0..8).map(Some).chain([None]) {
                    let view = game.view(viewer).unwrap();
                    assert!(bids.iter().all(|c| view.resolving.contains(c)
                        && view.card_faces.iter().any(|f| f.id == c.id)));
                    assert!(view.events.iter().any(|e| matches!(e.kind, EventKind::CompareReveal { cards, .. } if cards == bids)));
                }
                let ai = game.bot_command().unwrap();
                assert!(matches!(ai.action, Action::Select { branch: 1, .. }));
                ai.action
            } else {
                let branch = choice
                    .branches
                    .iter()
                    .find(|b| b.id == 0)
                    .unwrap_or(&choice.branches[0]);
                Action::Select {
                    event: choice.event,
                    branch: branch.id,
                    cards: branch.cards[..branch.min_cards].to_vec(),
                    targets: branch.targets[..branch.min_targets].to_vec(),
                }
            }
        }
    };
    submit(game, actor, action);
}

fn first_trial() -> (Game, Trial) {
    let golden: Value =
        serde_json::from_str(include_str!("../../../../../tests/reference/compare.json")).unwrap();
    let mut game = Game::classic(5, 0, "guanyu", 8).unwrap();
    let trial = Trial {
        a: 1,
        b: 2,
        turn: 2,
        sample: golden["trials"][0].clone(),
        log: Default::default(),
    };
    trial.install(&mut game);
    (game, trial)
}

fn settle_first_trial(game: &mut Game, trial: &Trial) -> Vec<GameEvent> {
    let mut events = Vec::new();
    for _ in 0..60 {
        events.extend(game.view(None).unwrap().events);
        if game
            .view(Some(trial.a))
            .unwrap()
            .actions
            .contains(&Action::EndPlay)
        {
            return events;
        }
        next_decision(game, std::slice::from_ref(trial));
    }
    panic!("the comparison did not return to its actual play phase");
}

mod boundaries;

#[test]
fn fixing_can_change_the_target_score_and_a_later_claim_cannot_take_the_same_bid() {
    let (mut game, trial) = first_trial();
    let mut fix = registration(
        trial.b,
        "fix_score",
        "compareFixing",
        TriggerOrder::Normal,
        |ctx, _, _| {
            assert_eq!(
                ctx.game().comparison(ctx.event().subject).unwrap().numbers,
                [13, 9]
            );
            assert_eq!(ctx.set_compare_number(7, 3), Err(RuleError::InvalidTarget));
            ctx.set_compare_number(ctx.owner(), 21).unwrap();
            TriggerStep::Done
        },
    );
    fix.role = TriggerRole::Target;
    game.register_trigger(fix).unwrap();
    let attempted = Arc::new(Mutex::new(false));
    let checked = attempted.clone();
    let mut late = registration(
        0,
        "second_claim",
        "chooseToCompareAfter",
        TriggerOrder::Last,
        move |ctx, _, _| {
            let result = *ctx.game().comparison(ctx.event().subject).unwrap();
            assert!(!result.won && !result.tie);
            assert!(!ctx.game().can_gain_compare_card(
                result.event,
                ctx.owner(),
                result.cards[0].id
            ));
            assert_eq!(
                ctx.gain_compare_card(ctx.owner(), result.cards[0].id),
                Err(RuleError::InvalidCard)
            );
            *checked.lock().unwrap() = true;
            TriggerStep::Done
        },
    );
    late.role = TriggerRole::Global;
    game.register_trigger(late).unwrap();
    settle_first_trial(&mut game, &trial);
    assert!(*attempted.lock().unwrap());
    let result = game.compare_history()[0];
    assert_eq!(result.numbers, [13, 21]);
    assert_eq!(result.winner, Some(trial.b));
    let (a_card, b_card) = (result.cards[0], result.cards[1]);
    assert_eq!(
        game.view(None).unwrap().seats[trial.a as usize].known_hand,
        [a_card]
    );
    assert_eq!(game.discard.iter().filter(|&&c| c == b_card).count(), 1);
}

#[test]
fn nested_comparisons_return_only_their_own_result_and_cleanup_only_their_own_cards() {
    let resumed = Arc::new(Mutex::new(Vec::new()));
    let outer_return = resumed.clone();
    let outer = registration(
        0,
        "outer_author",
        "phaseUseBegin",
        TriggerOrder::Normal,
        move |ctx, stage, _| {
            if stage == 0 {
                ctx.compare(0, 1).unwrap();
                return TriggerStep::Continue(1);
            }
            outer_return
                .lock()
                .unwrap()
                .push(ctx.compare_result().unwrap());
            TriggerStep::Done
        },
    );
    let inner_return = resumed.clone();
    let mut inner = registration(
        1,
        "nested_author",
        "compare",
        TriggerOrder::Normal,
        move |ctx, stage, _| {
            let outer = *ctx.game().comparison(ctx.event().subject).unwrap();
            if stage == 0 {
                assert!(ctx.compare_result().is_none());
                ctx.compare(1, 0).unwrap();
                return TriggerStep::Continue(1);
            }
            let inner = ctx.compare_result().unwrap();
            assert_ne!(inner.event, outer.event);
            assert!(inner.cards.iter().all(|c| ctx.game().discard.contains(c)));
            assert!(outer
                .cards
                .iter()
                .all(|c| ctx.game().resolving.contains(c) && !inner.cards.contains(c)));
            // Resuming the nested operation still edits the enclosing trigger's own subject.
            ctx.set_compare_number(0, 19).unwrap();
            inner_return.lock().unwrap().push(inner);
            TriggerStep::Done
        },
    );
    inner.role = TriggerRole::Target;
    let mut game = Game::classic_with_triggers(0, 0, "guanyu", 2, vec![outer, inner]).unwrap();
    let mut events = Vec::new();
    let mut actors = Vec::new();
    loop {
        let view = game.view(None).unwrap();
        events.extend(view.events);
        if resumed.lock().unwrap().len() == 2 {
            break;
        }
        let seat = view.waiting_for.unwrap();
        let choice = game.view(Some(seat)).unwrap().choice.unwrap();
        assert!(choice.compare.is_some() && actors.len() < 4);
        if seat == 0 {
            // Offline human input remains pending; an observer cannot answer for it.
            assert!(game.bot_command().is_none());
            assert_eq!(game.view(None).unwrap().waiting_for, Some(seat));
        }
        actors.push(seat);
        let offered = &choice.branches[0];
        submit(
            &mut game,
            seat,
            Action::Select {
                event: choice.event,
                branch: offered.id,
                cards: vec![offered.cards[0]],
                targets: vec![],
            },
        );
    }
    assert_eq!(actors, [0, 1, 1, 0]);
    let results = resumed.lock().unwrap();
    let (inner, outer) = (results[0], results[1]);
    assert_eq!(game.compare_history(), results.as_slice());
    assert_eq!(
        outer.participants,
        CompareParticipants {
            initiator: 0,
            target: 1
        }
    );
    assert_eq!(
        inner.participants,
        CompareParticipants {
            initiator: 1,
            target: 0
        }
    );
    assert_eq!(outer.numbers[0], 19);
    let mut ancestor = inner.parent;
    while ancestor != Some(outer.event) {
        let parent = events
            .iter()
            .find(|e| Some(e.id) == ancestor)
            .expect("nested comparison has an enclosing comparison frame");
        ancestor = parent.parent;
    }
    assert!(game.resolving.is_empty());
    for card in inner.cards.into_iter().chain(outer.cards) {
        assert_eq!(game.discard.iter().filter(|&&c| c == card).count(), 1);
    }
}

#[test]
fn a_private_transfer_during_second_payment_cannot_be_undone_or_discarded_by_compare_cleanup() {
    let author = registration(
        0,
        "author",
        "phaseUseBegin",
        TriggerOrder::Normal,
        |ctx, stage, _| {
            if stage == 0 {
                ctx.compare(0, 1).unwrap();
                return TriggerStep::Continue(1);
            }
            assert!(ctx.compare_result().is_some());
            TriggerStep::Done
        },
    );
    let mut intercept = registration(
        1,
        "intercept",
        "loseBefore",
        TriggerOrder::First,
        |ctx, _, _| {
            let card = ctx.event().cards[0];
            ctx.give_hand(1, 0, card.id).unwrap();
            TriggerStep::Done
        },
    );
    intercept.filter = Arc::new(|game, event, _| {
        matches!(
            game.scheduler
                .active_frame(event.parent.unwrap())
                .map(|(_, p)| p.kind()),
            Some(EventKind::LoseAsync)
        )
    });
    let mut game = Game::classic_with_triggers(0, 0, "guanyu", 2, vec![author, intercept]).unwrap();
    let bids = [0, 1].map(|seat| {
        let view = game.view(Some(seat)).unwrap();
        let choice = view.choice.unwrap();
        let card = view.seats[seat as usize].hand.as_ref().unwrap()[0];
        assert!(choice.compare.is_some());
        submit(
            &mut game,
            seat,
            Action::Select {
                event: choice.event,
                branch: choice.branches[0].id,
                cards: vec![card.id],
                targets: vec![],
            },
        );
        card
    });
    assert!(game
        .view(Some(0))
        .unwrap()
        .actions
        .contains(&Action::EndPlay));
    assert_eq!(game.compare_history()[0].cards, bids);
    let actual_hand = game.view(Some(0)).unwrap().seats[0].hand.clone().unwrap();
    let public = game.view(None).unwrap();
    assert_eq!(actual_hand.iter().filter(|&&c| c == bids[1]).count(), 1);
    assert!(!actual_hand.contains(&bids[0]) && !game.seat(1).unwrap().hand.contains(&bids[1]));
    assert_eq!(public.discard.iter().filter(|&&c| c == bids[0]).count(), 1);
    assert!(!public.discard.contains(&bids[1]) && public.resolving.is_empty());
    assert!(public.seats[0].known_hand.is_empty());
    assert!(public
        .events
        .iter()
        .any(|e| matches!(e.kind, EventKind::CompareReveal { .. })));
}

#[test]
fn cancelling_the_play_begin_subject_skips_its_play_decision() {
    let skip = registration(
        0,
        "skip",
        "phaseUseBegin",
        TriggerOrder::First,
        |_, _, _| TriggerStep::CancelSubject,
    );
    let game = Game::classic_with_triggers(0, 0, "guanyu", 2, vec![skip]).unwrap();
    let view = game.view(Some(0)).unwrap();
    assert!(view.events.iter().any(|e| matches!(
        e.kind,
        EventKind::Phase {
            seat: 0,
            phase: crate::Phase::Play
        }
    )));
    assert!(view.actions.is_empty());
    assert_eq!(view.turn.unwrap().phase, crate::Phase::Discard);
}

#[test]
fn a_match_ending_after_the_first_bid_does_not_leave_an_unanswerable_second_bid() {
    let author = registration(
        0,
        "author",
        "phaseUseBegin",
        TriggerOrder::Normal,
        |ctx, _, _| {
            ctx.compare(0, 1).unwrap();
            TriggerStep::Done
        },
    );
    let interrupt = registration(
        0,
        "fatal_loss",
        "chooseCardAfter",
        TriggerOrder::First,
        |ctx, _, _| {
            ctx.lose_hp(1, 10).unwrap();
            TriggerStep::Done
        },
    );
    let mut game = Game::classic_with_triggers(0, 0, "guanyu", 2, vec![author, interrupt]).unwrap();
    let first = game.view(Some(0)).unwrap().choice.unwrap();
    submit(
        &mut game,
        0,
        Action::Select {
            event: first.event,
            branch: first.branches[0].id,
            cards: vec![first.branches[0].cards[0]],
            targets: vec![],
        },
    );
    for _ in 0..10 {
        if game.view(None).unwrap().winner.is_some() {
            break;
        }
        next_decision(&mut game, &[]);
    }
    let finished = game.view(None).unwrap();
    assert!(finished.winner.is_some());
    assert!(finished.waiting_for.is_none() && game.compare_history().is_empty());
}

#[test]
fn public_compare_matches_choices_physical_movements_scores_and_cleanup_on_three_natural_turns() {
    let golden: Value =
        serde_json::from_str(include_str!("../../../../../tests/reference/compare.json")).unwrap();
    assert_eq!(golden["events"].as_array().unwrap().len(), 104);
    let mut game = Game::classic(5, 0, "guanyu", 8).unwrap();
    let trials: Vec<_> = golden["trials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| {
            let turn = sample["turn"].as_u64().unwrap();
            let a = turn as u32 - 1;
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
    let mut events = Vec::<GameEvent>::new();
    for _ in 0..100 {
        events.extend(game.view(None).unwrap().events);
        if trials
            .last()
            .unwrap()
            .log
            .lock()
            .unwrap()
            .returned
            .is_some()
        {
            break;
        }
        next_decision(&mut game, &trials);
    }
    assert_eq!(game.compare_history().len(), 3);
    for trial in &trials {
        let log = trial.log.lock().unwrap();
        let result = log
            .returned
            .expect("calling content resumed after ordering cleanup");
        let hands = log.hands.unwrap();
        let expected = &trial.sample["result"];
        assert_eq!(result.turn, Some(trial.turn));
        assert_eq!(log.choices, [trial.a, trial.b]);
        assert_eq!(result.won, expected["bool"]);
        assert_eq!(result.tie, expected["tie"]);
        assert_eq!(json!(result.numbers), expected["numbers"]);
        assert_eq!(
            result.winner,
            expected["winner"]
                .as_str()
                .map(|s| if s == "A" { trial.a } else { trial.b })
        );
        assert_eq!(result.cards, [hands[0][0], hands[1][0]]);
        assert_eq!(game.comparison(result.event), Some(&result));
        assert!(!game.can_gain_compare_card(result.event, trial.a, result.cards[0].id));
        let parent = events.iter().find(|e| Some(e.id) == result.parent).unwrap();
        assert!(
            matches!(parent.kind, EventKind::RegisteredTrigger { registration, .. } if game.trigger_registrations[&registration].skill == "qiaoshui")
        );
        let cost_position = events
            .iter()
            .position(|e| {
                e.parent == result.parent
                    && e.status == EventStatus::Finished
                    && matches!(e.kind, EventKind::ChooseContent { seat, .. } if seat == trial.a)
            })
            .unwrap();
        let compare_position = events
            .iter()
            .position(|e| e.id == result.event && e.status == EventStatus::Started)
            .unwrap();
        assert!(cost_position < compare_position);
        let children: Vec<_> = events
            .iter()
            .filter(|e| e.parent == Some(result.event) && e.status == EventStatus::Started)
            .collect();
        assert_eq!(children.iter().filter(|e| matches!(e.kind, EventKind::CompareReveal { cards, .. } if cards == result.cards)).count(), 1);
        assert_eq!(children.iter().filter(|e| matches!(e.kind, EventKind::CompareResult { result: value } if value == result)).count(), 1);
        let reference: Vec<_> = golden["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                r[0] == trial.sample["sample"]
                    && r[1] == "request"
                    && TIMINGS.contains(&r[2].as_str().unwrap())
            })
            .map(|r| Value::Array(r.as_array().unwrap()[2..10].to_vec()))
            .collect();
        assert_eq!(log.rows, reference, "{}", trial.sample["sample"]);
        assert_eq!(reference.len(), 17);
    }
}
mod qiaoshui_targets;
mod turn_expiry;
mod use_boundaries;
mod use_targets;
