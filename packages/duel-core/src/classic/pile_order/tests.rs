use super::*;
use crate::classic::tests::scene;
use crate::classic_data::Role;
use crate::{CardZone, EventStatus, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep};
use std::sync::Mutex;

#[test]
fn cancelling_the_child_keeps_preview_private_until_the_root_cleans_it_up() {
    let mut game = scene(
        &[
            Role::Lord,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Rebel,
            Role::Loyalist,
            Role::Renegade,
        ],
        0,
    );
    let cards: Vec<_> = game
        .classic
        .as_ref()
        .unwrap()
        .deck
        .iter()
        .rev()
        .take(5)
        .copied()
        .collect();
    let timings = Arc::new(Mutex::new(Vec::<String>::new()));
    let observations = timings.clone();
    let observed_cards = cards.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        let zone = match event.timing.as_str() {
            "cardsGotoOrderingBefore" => None,
            "cardsGotoOrderingAfter"
            | "chooseToMoveBegin"
            | "chooseToMoveCancelled"
            | "choosePileOrderAfter"
            | "cardsDiscardBefore" => Some(CardZone::Ordering),
            "cardsDiscardAfter" => Some(CardZone::DiscardPile),
            _ => return,
        };
        if event.timing == "choosePileOrderAfter" {
            let result = game.pile_order_result(event.subject).unwrap();
            assert_eq!(result.rows, [vec![], vec![]]);
            assert!(!result.cancelled);
        }
        if event.timing == "cardsDiscardAfter" {
            assert_eq!(
                game.event_kind(event.parent.unwrap()),
                Some(EventKind::OrderingDiscard)
            );
        }
        for card in &observed_cards {
            assert_eq!(
                game.card_location(card.id)
                    .and_then(|location| location.zone),
                zone
            );
        }
        if zone == Some(CardZone::Ordering) {
            let spectator = game.view(Some(2)).unwrap();
            assert!(spectator.resolving.is_empty());
            assert!(spectator.private_choice_cards.is_empty());
        }
        observations.lock().unwrap().push(event.timing.clone());
    }));
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "cancel_pile_choice".into(),
        timing: "chooseToMoveBegin".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|_, _, _| TriggerStep::CancelSubject),
    })
    .unwrap();
    game.scheduler
        .start(game.pile_order_program(0, 5, "整理牌堆"));
    game.run(None);
    assert_eq!(
        *timings.lock().unwrap(),
        [
            "cardsGotoOrderingBefore",
            "cardsGotoOrderingAfter",
            "chooseToMoveBegin",
            "chooseToMoveCancelled",
            "choosePileOrderAfter",
            "cardsDiscardBefore",
            "cardsDiscardAfter"
        ]
    );
    assert!(game.events.iter().any(|event| {
        matches!(event.kind, EventKind::ChooseToMove { seat: 0 })
            && event.status == EventStatus::Cancelled
    }));
    assert!(game.events.iter().any(|event| {
        matches!(event.kind, EventKind::ChoosePileOrder { seat: 0, .. })
            && event.status == EventStatus::Finished
    }));
    assert_eq!(game.discard, cards);
    assert!(game.reserved_draws.is_empty());
    assert!(game.private_ordering.is_empty());
}

#[test]
fn cancelling_cleanup_releases_the_same_cards_back_to_the_deck() {
    let mut game = scene(
        &[Role::Lord, Role::Rebel, Role::Loyalist, Role::Renegade],
        0,
    );
    let deck = game.classic.as_ref().unwrap().deck.clone();
    let preview = &deck[deck.len() - 4..];
    for timing in ["chooseToMoveBegin", "cardsDiscardBefore"] {
        game.register_trigger(TriggerRegistration {
            owner: 0,
            skill: format!("cancel_{timing}"),
            timing: timing.into(),
            role: TriggerRole::Player,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(|_, _, _| TriggerStep::CancelSubject),
        })
        .unwrap();
    }
    game.scheduler
        .start(game.pile_order_program(0, 4, "整理牌堆"));
    game.run(None);
    assert!(game.private_ordering.is_empty());
    assert!(game.reserved_draws.is_empty());
    assert!(game.resolving.is_empty());
    assert!(game.discard.is_empty());
    for card in preview {
        assert_eq!(
            game.physical_cards()
                .filter(|&actual| actual == card)
                .count(),
            1
        );
        assert!(!game.ordering_owners.contains_key(&card.id));
    }
}
