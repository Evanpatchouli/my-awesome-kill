use duel_core::{
    Action, Card, ChoiceBranch, ChoiceKind, Command, EventKind, EventStatus, Game, Phase,
    RecastGain, RecastKey, RecastLoss, RecastRule, RuleError, TriggerContext, TriggerOrder,
    TriggerRegistration, TriggerRole, TriggerSelection, TriggerStep,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn table() -> Game {
    let game = Game::classic(0, 0, "guanyu", 2).unwrap();
    assert!(game
        .view(Some(0))
        .unwrap()
        .actions
        .contains(&Action::EndPlay));
    game
}

fn play(game: &mut Game, action: Action) {
    let version = game.view(None).unwrap().version;
    game.submit(Command {
        version,
        seat: 0,
        action,
    })
    .unwrap();
}

#[test]
fn state_dependent_registration_is_callable_from_public_view_and_submit() {
    let mut game = table();
    let initial = game.view(Some(0)).unwrap();
    let material = initial.seats[0].hand.as_ref().unwrap()[0];
    let received = Arc::new(Mutex::new(None));
    let gain_card = received.clone();
    game.register_recast(
        RecastKey::Card(material.id),
        RecastRule {
            filter: Some(Arc::new(move |state, seat, card| {
                let actor = state.seat(seat).unwrap();
                assert_eq!(state.seats().collect::<Vec<_>>(), [0, 1]);
                assert_eq!(state.turn().unwrap().phase, Phase::Play);
                actor.alive
                    && actor.hp == actor.max_hp
                    && actor.hand.contains(&card)
                    && card == material
                    && state.face(card.id).is_some()
                    && !state.draw_pile().contains(&card)
                    && !state.resolving().contains(&card)
                    && !state.seat(1).unwrap().hand.contains(&card)
            })),
            loss: Arc::new(|state, seat, card| {
                assert!(state.seat(seat).unwrap().hand.contains(&card));
                RecastLoss::Discard
            }),
            gain: Arc::new(move |state, seat, card, movement| {
                assert!(movement.moved && movement.card == card && movement.event > 0);
                assert!(state.discard().contains(&card));
                assert!(!state.seat(seat).unwrap().hand.contains(&card));
                let replacement = *state.draw_pile().last().unwrap();
                *gain_card.lock().unwrap() = Some(replacement);
                RecastGain::FromDeck {
                    card: replacement.id,
                }
            }),
        },
    );
    let action = Action::Recast { card: material.id };
    assert!(game.can_recast(0, material.id));
    assert!(game.view(Some(0)).unwrap().actions.contains(&action));
    assert!(game.view(Some(1)).unwrap().actions.is_empty());
    let command = Command {
        version: initial.version,
        seat: 0,
        action,
    };
    game.submit(command.clone()).unwrap();
    let replacement = received.lock().unwrap().unwrap();
    assert!(game.view(Some(0)).unwrap().seats[0]
        .hand
        .as_ref()
        .unwrap()
        .contains(&replacement));
    let observer = game.view(Some(1)).unwrap();
    assert!(observer.seats[0].hand.is_none());
    assert_eq!(observer.seats[0].known_hand, [replacement]);
    assert!(observer.card_faces.iter().any(|f| f.id == replacement.id));
    assert!(matches!(
        game.submit(command),
        Err(RuleError::WrongVersion { .. })
    ));
    assert_eq!(game.view(Some(1)).unwrap(), observer);
}

fn listen(
    game: &mut Game,
    timing: &str,
    effect: impl Fn(&mut TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) {
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: timing.to_owned(),
        timing: timing.to_owned(),
        role: TriggerRole::Player,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(effect),
    })
    .unwrap();
}

#[derive(Clone, Copy, PartialEq)]
enum Payment {
    Normal,
    Cancelled,
    AlreadyGiven,
}

fn skill_recast(payment: Payment) {
    let mut game = table();
    let initial = game.view(Some(0)).unwrap();
    let hand = initial.seats[0].hand.as_ref().unwrap();
    let (material, cost) = (hand[0], hand[1]);
    assert!(!game.can_recast(0, material.id));
    let stages = Arc::new(AtomicUsize::new(0));
    let calls = stages.clone();
    listen(&mut game, "phaseJieshuBegin", move |ctx, stage, _| {
        assert_eq!(
            ctx.game().view(None).unwrap().turn.unwrap().phase,
            Phase::End
        );
        calls.fetch_add(1, Ordering::SeqCst);
        if stage == 1 {
            return TriggerStep::Done;
        }
        if payment == Payment::AlreadyGiven {
            ctx.discard_hand(0, &[cost.id]).unwrap();
        }
        ctx.recast(0, material.id).unwrap();
        assert_eq!(ctx.recast(0, material.id), Err(RuleError::InvalidSelection));
        assert_eq!(
            ctx.discard_hand(0, &[material.id]),
            Err(RuleError::InvalidSelection)
        );
        assert_eq!(
            ctx.give_hand(0, 1, material.id),
            Err(RuleError::InvalidSelection)
        );
        TriggerStep::Continue(1)
    });
    if payment == Payment::AlreadyGiven {
        listen(&mut game, "loseAfter", move |ctx, _, _| {
            if ctx.event().cards == [cost] {
                ctx.give_hand(0, 1, material.id).unwrap();
            }
            TriggerStep::Done
        });
    } else if payment == Payment::Cancelled {
        listen(&mut game, "recastBefore", |_, _, _| {
            TriggerStep::CancelSubject
        });
    }
    play(&mut game, Action::EndPlay);
    let discard = game.view(Some(0)).unwrap().choice.unwrap();
    assert_eq!(discard.kind, ChoiceKind::Discard);
    let cards = discard
        .cards
        .into_iter()
        .filter(|c| *c != material.id && *c != cost.id)
        .take(discard.min)
        .collect();
    play(
        &mut game,
        Action::Discard {
            event: discard.event,
            cards,
        },
    );
    let actor = game.view(Some(0)).unwrap();
    let other = game.view(Some(1)).unwrap();
    let recasts: Vec<_> = actor
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::Recast { seat: 0 }))
        .collect();
    assert_eq!(
        actor
            .events
            .iter()
            .filter(|e| e.status == EventStatus::Started
                && matches!(e.kind, EventKind::RecastReveal { seat: 0, card } if card == material))
            .count(),
        usize::from(payment == Payment::Normal)
    );
    if payment == Payment::AlreadyGiven {
        assert!(recasts.is_empty());
        assert!(other.seats[1].hand.as_ref().unwrap().contains(&material));
        assert_eq!(actor.seats[1].known_hand, [material]);
        assert!(game.view(None).unwrap().seats[1].known_hand.is_empty());
        assert!(actor.discard.contains(&cost));
    } else {
        assert_eq!(recasts.len(), 2);
        let parent = actor
            .events
            .iter()
            .find(|e| Some(e.id) == recasts[0].parent)
            .unwrap();
        assert!(matches!(
            parent.kind,
            EventKind::RegisteredTrigger { seat: 0, .. }
        ));
        assert_eq!(
            recasts[1].status,
            if payment == Payment::Normal {
                EventStatus::Finished
            } else {
                EventStatus::Cancelled
            }
        );
    }
    let held = actor.seats[0].hand.as_ref().unwrap().contains(&material);
    assert_eq!(held, payment == Payment::Cancelled);
    assert_eq!(
        actor.discard.contains(&material),
        payment == Payment::Normal
    );
    assert_eq!(
        stages.load(Ordering::SeqCst),
        if payment == Payment::Normal { 2 } else { 1 }
    );
    let gained: Vec<Card> = actor.seats[0]
        .hand
        .as_ref()
        .unwrap()
        .iter()
        .copied()
        .filter(|card| !hand.contains(card))
        .collect();
    assert_eq!(gained.len(), usize::from(payment == Payment::Normal));
}

#[test]
fn a_skill_can_recast_an_ordinary_card_at_end_phase() {
    skill_recast(Payment::Normal);
}

#[test]
fn cancelling_a_skill_recast_stops_its_paid_continuation() {
    skill_recast(Payment::Cancelled);
}

#[test]
fn earlier_nested_transfer_invalidates_later_promised_recast() {
    skill_recast(Payment::AlreadyGiven);
}

fn assert_unannounced(game: &Game, material: Card, status: EventStatus) {
    for viewer in [None, Some(1)] {
        let view = game.view(viewer).unwrap();
        assert!(view.seats[0].hand.is_none());
        assert!(!view.seats[0].known_hand.contains(&material));
        assert!(!view.card_faces.iter().any(|face| face.id == material.id));
        assert!(!view.discard.contains(&material) && !view.resolving.contains(&material));
        assert!(!view.private_choice_cards.contains(&material));
        assert!(view.choice.is_none() && view.actions.is_empty());
        assert!(view
            .events
            .iter()
            .any(|e| e.status == status && matches!(e.kind, EventKind::Recast { seat: 0 })));
        assert!(!view
            .events
            .iter()
            .any(|e| matches!(e.kind, EventKind::RecastReveal { .. })));
        let encoded = serde_json::to_value(&view.events).unwrap();
        assert!(encoded
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e.get("card").is_none()));
    }
    assert!(game.view(Some(0)).unwrap().seats[0]
        .hand
        .as_ref()
        .unwrap()
        .contains(&material));
}

fn pause_before_recast_announcement(timing: &str, allow: bool) {
    let mut game = table();
    let material = game.view(Some(0)).unwrap().seats[0].hand.as_ref().unwrap()[0];
    game.register_recast(RecastKey::Card(material.id), Default::default());
    listen(&mut game, timing, move |ctx, stage, choice| {
        assert_eq!(ctx.event().cards, [material]);
        if stage == 0 {
            return TriggerStep::Choose {
                branches: ["cancel", "continue"]
                    .into_iter()
                    .enumerate()
                    .map(|(id, label)| ChoiceBranch {
                        id: id as u32,
                        label: label.into(),
                        cards: vec![],
                        min_cards: 0,
                        max_cards: 0,
                        targets: vec![],
                        min_targets: 0,
                        max_targets: 0,
                    })
                    .collect(),
                then: 1,
            };
        }
        if choice.unwrap().branch == 0 {
            TriggerStep::CancelSubject
        } else {
            TriggerStep::Done
        }
    });
    let announcements = Arc::new(AtomicUsize::new(0));
    let count = announcements.clone();
    listen(&mut game, "recastingLose", move |ctx, _, _| {
        count.fetch_add(1, Ordering::SeqCst);
        for viewer in [None, Some(1)] {
            let view = ctx.game().view(viewer).unwrap();
            assert_eq!(view.seats[0].known_hand, [material]);
            assert!(view.card_faces.iter().any(|face| face.id == material.id));
            let reveal: Vec<_> = view.events.iter().filter(|e| e.status == EventStatus::Started &&
                matches!(e.kind, EventKind::RecastReveal { seat: 0, card } if card == material)).collect();
            assert_eq!(reveal.len(), 1);
            assert_eq!(reveal[0].parent, Some(ctx.event().subject));
        }
        TriggerStep::Done
    });
    play(&mut game, Action::Recast { card: material.id });
    assert_unannounced(&game, material, EventStatus::Started);
    let choice = game.view(Some(0)).unwrap().choice.unwrap();
    play(
        &mut game,
        Action::Select {
            event: choice.event,
            branch: u32::from(allow),
            cards: vec![],
            targets: vec![],
        },
    );
    assert_eq!(announcements.load(Ordering::SeqCst), usize::from(allow));
    if allow {
        assert!(game.view(None).unwrap().discard.contains(&material));
    } else {
        assert_unannounced(&game, material, EventStatus::Cancelled);
    }
}

#[test]
fn paused_and_cancelled_pre_recast_hooks_keep_material_private() {
    for timing in ["recastBefore", "recastBegin"] {
        pause_before_recast_announcement(timing, false);
        pause_before_recast_announcement(timing, true);
    }
}
