use duel_core::{
    content::{CardKey, ContentPack},
    Action, Card, CardKind, Command, EventKind, Game, GameSetup, RecordedGame, ReplayLog,
    ReplaySetup, RuleError, SeatSetup,
};

fn example() -> ContentPack {
    let mut pack = ContentPack::parse(include_str!("../data/example-content-pack.json")).unwrap();
    pack.deck[0].copies = 32;
    pack
}

fn seeded(pack: &ContentPack, has_card: bool) -> RecordedGame {
    (0..256)
        .find_map(|seed| {
            let session = RecordedGame::new(ReplaySetup::SeededContent {
                seed,
                human: 0,
                hero: "demo:traveler".into(),
                seats: 2,
                pack: pack.clone(),
            })
            .unwrap();
            let view = session.game().view(Some(0)).unwrap();
            (view.waiting_for == Some(0)
                && !view.active_plays.is_empty()
                && (!has_card
                    || view.seats[0]
                        .hand
                        .as_ref()
                        .unwrap()
                        .iter()
                        .any(|card| matches!(card.kind, CardKind::Content(_)))))
            .then_some(session)
        })
        .expect("a deterministic opening has a custom hero and card")
}

#[test]
fn validates_ids_and_effects_and_keeps_wire_kinds_distinct() {
    let pack = example();
    assert_eq!(pack.preview().unwrap().generals[0].skills.len(), 2);
    let kind = CardKind::Content(CardKey::new("demo:more").unwrap());
    assert_eq!(serde_json::to_string(&kind).unwrap(), r#""demo:more""#);
    assert_eq!(
        serde_json::from_str::<CardKind>(r#""demo:more""#).unwrap(),
        kind
    );
    assert_eq!(
        serde_json::from_str::<CardKind>(r#""wuzhong""#).unwrap(),
        CardKind::Wuzhong
    );
    assert!(serde_json::from_str::<CardKind>(r#""unregistered""#).is_err());
    let prescribed = GameSetup {
        seats: vec![
            SeatSetup {
                id: 0,
                hp: 4,
                hand: vec![Card { id: 1, kind }],
            },
            SeatSetup {
                id: 1,
                hp: 4,
                hand: vec![],
            },
        ],
        active_seat: 0,
    };
    assert!(matches!(
        Game::new(prescribed),
        Err(RuleError::UnsupportedCard)
    ));

    let mut duplicate = pack.clone();
    duplicate.cards.push(duplicate.cards[0].clone());
    assert!(duplicate.preview().is_err());
    let mut bad_reference = pack.clone();
    bad_reference.generals[0].skills.push("demo:nope".into());
    assert!(bad_reference.preview().is_err());
    let mut invalid_deck = pack.clone();
    invalid_deck.deck[0].rank = 14;
    assert!(matches!(
        Game::classic_with_content(0, 0, "demo:traveler", 2, &invalid_deck),
        Err(RuleError::InvalidContent { .. })
    ));
    assert!(ContentPack::parse(&" ".repeat(128 * 1024 + 1)).is_err());
    let unknown_op = include_str!("../data/example-content-pack.json")
        .replace(r#""op": "draw""#, r#""op": "arbitrary""#);
    assert!(ContentPack::parse(&unknown_op).is_err());
}

#[test]
fn custom_general_skill_and_card_share_existing_rules_and_replay() {
    let pack = example();
    let mut session = seeded(&pack, true);
    let opening = session.game().view(Some(0)).unwrap();
    let spectator = session.game().view(None).unwrap();
    let custom_kind = CardKind::Content(pack.cards[0].id);
    let card = *opening.seats[0]
        .hand
        .as_ref()
        .unwrap()
        .iter()
        .find(|card| card.kind == custom_kind)
        .unwrap();
    assert_eq!(opening.seats[0].hero.as_deref(), Some("demo:traveler"));
    assert_eq!(
        opening.seats[0].hand_count, 7,
        "opening four, phase two, trigger one"
    );
    assert!(spectator.seats[0].hand.is_none());
    assert!(!spectator.card_faces.iter().any(|face| face.id == card.id));

    let active = &opening.active_plays[0];
    session
        .submit(Command {
            version: opening.version,
            seat: 0,
            action: Action::ActivateSkill {
                registration: active.registration,
                cards: vec![],
                targets: vec![],
            },
        })
        .unwrap();
    let gained = session.game().view(Some(0)).unwrap();
    assert_eq!(gained.seats[0].hand_count, 8);
    assert!(gained.active_plays.is_empty());
    assert_eq!(
        session.submit(Command {
            version: gained.version,
            seat: 0,
            action: Action::PlayCard {
                card: card.id,
                targets: vec![1]
            },
        }),
        Err(RuleError::InvalidTarget)
    );
    session
        .submit(Command {
            version: gained.version,
            seat: 0,
            action: Action::PlayCard {
                card: card.id,
                targets: vec![0],
            },
        })
        .unwrap();
    for _ in 0..8 {
        let waiting = session.game().view(None).unwrap().waiting_for;
        let Some(seat) = waiting else { break };
        let view = session.game().view(Some(seat)).unwrap();
        let Some(choice) = view.choice else { break };
        let Some(context) = choice.counter else { break };
        assert_eq!(context.card_kind, custom_kind);
        session
            .submit(Command {
                version: view.version,
                seat,
                action: Action::Select {
                    event: choice.event,
                    branch: 0,
                    cards: vec![],
                    targets: vec![],
                },
            })
            .unwrap();
    }
    let played = session.game().view(Some(0)).unwrap();
    assert_eq!(played.seats[0].hand_count, 10);
    assert!(played.discard.contains(&card));
    assert!(played
        .events
        .iter()
        .any(|event| matches!(event.kind, EventKind::Draw { amount: 3, .. })));

    let raw = serde_json::to_string(session.log()).unwrap();
    let restored: ReplayLog = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        restored.restore_at(0).unwrap().view(Some(0)).unwrap(),
        opening
    );
    assert_eq!(restored.restore().unwrap().view(Some(0)).unwrap(), played);
}
