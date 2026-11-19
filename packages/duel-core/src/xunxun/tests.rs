use super::*;
use crate::classic::tests::scene;
use crate::classic_data::{Role, Suit};
use crate::{Action, CardFace, CardZone, Command, EventKind, MoveStep, Program};
use std::sync::{Arc, Mutex};

const TIMINGS: &[&str] = &[
    "phaseZhunbeiBefore",
    "phaseZhunbeiBegin",
    "phaseZhunbeiEnd",
    "phaseZhunbeiAfter",
    "phaseDrawBefore",
    "phaseDrawBegin",
    "phaseDrawBegin1",
    "cardsGotoOrderingBefore",
    "cardsGotoOrderingBegin",
    "cardsGotoOrderingEnd",
    "cardsGotoOrderingAfter",
    "chooseToMoveBefore",
    "chooseToMoveBegin",
    "chooseToMoveEnd",
    "chooseToMoveAfter",
    "cardsGotoPileBefore",
    "cardsGotoPileBegin",
    "cardsGotoPileEnd",
    "cardsGotoPileAfter",
    "phaseDrawEnd",
    "phaseDrawAfter",
];

#[test]
fn natural_draw_matches_ordered_move_reference() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/reference/ordered-move.json"
    ))
    .unwrap();
    let trial = &golden["trials"][1];
    assert_eq!(trial["name"], "xunxun");
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
    let table = game.classic.as_mut().unwrap();
    table.turn = None;
    let printed = [
        (CardKind::Sha, Suit::Spade),
        (CardKind::Shan, Suit::Heart),
        (CardKind::Tao, Suit::Diamond),
        (CardKind::Juedou, Suit::Club),
        (CardKind::Wuxie, Suit::Spade),
    ];
    let mut cards = Vec::new();
    for (index, (card, (kind, suit))) in table.deck.iter_mut().rev().zip(printed).enumerate() {
        card.kind = kind;
        table.faces.push(CardFace {
            id: card.id,
            suit,
            rank: 3 + index as u8,
        });
        cards.push(*card);
    }
    let ids: Vec<_> = cards.iter().map(|c| c.id).collect();
    let observations = Arc::new(Mutex::new(Vec::new()));
    let out = observations.clone();
    let watched = ids.clone();
    game.timing_observer = Some(Arc::new(move |game, event| {
        if event.player == Some(0) && TIMINGS.contains(&event.timing.as_str()) {
            let zones: Vec<_> = watched
                .iter()
                .map(|id| match game.card_location(*id).and_then(|l| l.zone) {
                    Some(CardZone::Ordering) => "o",
                    Some(CardZone::DrawPile) => "c",
                    Some(CardZone::Hand) => "h",
                    _ => "?",
                })
                .collect();
            if !event.timing.starts_with("phase") {
                assert!(
                    matches!(
                        event.parent.and_then(|p| game.event_kind(p)),
                        Some(EventKind::RegisteredTrigger { seat: 0, .. })
                    ),
                    "the choice belongs to the registered skill frame"
                );
            }
            out.lock().unwrap().push((event.timing.clone(), zones));
        }
    }));
    game.register_trigger(registration(0)).unwrap();
    game.scheduler.start(Program::event(EventKind::PhaseLoop));
    game.run(None);
    let first = game.view(Some(0)).unwrap();
    let choice = first.choice.unwrap();
    assert!(choice
        .branches
        .iter()
        .any(|b| b.id == 1 && b.label == "发动恂恂"));
    game.submit(Command {
        version: first.version,
        seat: 0,
        action: Action::Select {
            event: choice.event,
            branch: 1,
            cards: vec![],
            targets: vec![],
        },
    })
    .unwrap();
    let private = game.view(Some(0)).unwrap();
    assert_eq!(private.private_choice_cards, cards[..4]);
    assert!(ids[..4]
        .iter()
        .all(|id| private.card_faces.iter().any(|face| face.id == *id)));
    let choice = private.choice.as_ref().unwrap();
    let request = choice.move_cards.as_ref().unwrap();
    assert_eq!(request.prompt, "恂恂：将两张牌置于牌堆底");
    assert!(!request.allow_decline);
    assert_eq!(request.rows[0].label, "牌堆顶");
    assert_eq!(request.rows[0].cards, ids[..4]);
    assert_eq!(request.rows[1].label, "牌堆底");
    assert_eq!((request.rows[1].min, request.rows[1].max), (2, 2));
    for seat in 1..8 {
        let view = game.view(Some(seat)).unwrap();
        assert!(view.choice.is_none() && view.private_choice_cards.is_empty());
        assert!(view
            .card_faces
            .iter()
            .all(|face| !ids[..4].contains(&face.id)));
    }
    let step = |i, row, index| MoveStep {
        card: ids[i],
        row,
        index,
    };
    let good = vec![step(3, 1, 0), step(1, 1, 1), step(2, 0, 0)];
    for moves in [
        None,
        Some(vec![step(3, 1, 0)]),
        Some(vec![
            step(3, 1, 0),
            step(1, 1, 1),
            step(2, 1, 2),
            step(2, 0, 0),
        ]),
    ] {
        assert!(game
            .submit(Command {
                version: private.version,
                seat: 0,
                action: Action::MoveCards {
                    event: choice.event,
                    moves
                }
            })
            .is_err());
        assert_eq!(game.view(Some(0)).unwrap(), private);
    }
    game.submit(Command {
        version: private.version,
        seat: 0,
        action: Action::MoveCards {
            event: choice.event,
            moves: Some(good),
        },
    })
    .unwrap();
    assert_eq!(game.seat(0).unwrap().hand, [cards[2], cards[0]]);
    let deck = &game.classic.as_ref().unwrap().deck;
    assert_eq!(deck.last(), Some(&cards[4]));
    assert_eq!(&deck[..2], &[cards[1], cards[3]]);
    for (i, card) in cards.iter().enumerate() {
        assert_eq!(game.physical_cards().filter(|c| *c == card).count(), 1);
        assert_eq!(game.is_known_by(card.id, Some(0)), i == 0 || i == 2);
        for seat in 1..8 {
            assert!(!game.is_known_by(card.id, Some(seat)));
        }
    }
    let expected: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r[0] == "xunxun" && TIMINGS.contains(&r[1].as_str().unwrap()))
        .map(|r| {
            let zones = r[6]
                .as_array()
                .unwrap()
                .iter()
                .map(|zone| zone.as_str().unwrap())
                .collect::<Vec<_>>();
            (r[1].as_str().unwrap().to_owned(), zones)
        })
        .collect();
    assert_eq!(expected.len(), 21);
    assert_eq!(observations.lock().unwrap().as_slice(), expected);
}
