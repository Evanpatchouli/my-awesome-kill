use super::*;
use crate::{
    Action, CardKind, Command, GameSetup, RuleError, SeatSetup, TriggerContext, TriggerEvent,
    TriggerOrder, TriggerRegistration, TriggerRole, TriggerSelection, TriggerStep,
};
use std::sync::Mutex;

#[test]
fn four_sealed_bids_are_private_validated_and_collected_in_order() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/compare-multiple.json"
    ))
    .unwrap();
    let seats = [0, 1, 3, 4];
    let mut game = Game::new(GameSetup {
        active_seat: 0,
        seats: (0..8)
            .map(|id| SeatSetup {
                id,
                hp: 4,
                hand: if seats.contains(&id) {
                    vec![
                        Card {
                            id: 100 + id,
                            kind: CardKind::Tao,
                        },
                        Card {
                            id: 200 + id,
                            kind: CardKind::Shan,
                        },
                    ]
                } else {
                    vec![]
                },
            })
            .collect(),
    })
    .unwrap();
    let observations = Arc::new(Mutex::new(Vec::new()));
    let out = observations.clone();
    let registration = TriggerRegistration {
        owner: 0,
        skill: "private_probe".into(),
        timing: "chooseCardAfter".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(
            move |ctx: &mut TriggerContext<'_>, _, _: Option<&TriggerSelection>| {
                let card = ctx
                    .game()
                    .selected_compare_card(ctx.event().subject)
                    .unwrap();
                out.lock()
                    .unwrap()
                    .push((ctx.event().player.unwrap(), card.id));
                TriggerStep::Done
            },
        ),
    };
    game.register_trigger(registration).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let out = requests.clone();
    game.timing_observer = Some(Arc::new(move |game, event: &TriggerEvent| {
        if [
            "chooseCardOLBegin",
            "chooseCardBegin",
            "chooseCardAfter",
            "chooseCardOLAfter",
        ]
        .contains(&event.timing.as_str())
        {
            assert!(game.resolving.is_empty());
            out.lock()
                .unwrap()
                .push((event.timing.clone(), event.player));
        }
    }));
    let group = BidGroup::Group {
        initiator: 0,
        targets: Arc::from([1, 3, 4]),
    };
    let flow = game.collect_bids(group);
    game.scheduler.start(flow);
    game.run(None);
    for seat in seats {
        let public = game.view(None).unwrap();
        assert_eq!(public.waiting_for, Some(seat));
        assert!(public.choice.is_none());
        assert!(game.view(Some((seat + 1) % 8)).unwrap().choice.is_none());
        let own = game.view(Some(seat)).unwrap();
        let choice = own.choice.unwrap();
        assert_eq!(choice.compare, None);
        assert_eq!(
            choice.compare_group,
            Some(CompareGroup {
                initiator: 0,
                targets: vec![1, 3, 4]
            })
        );
        assert_eq!(choice.branches[0].cards, [100 + seat, 200 + seat]);
        let wrong = Command {
            version: own.version,
            seat,
            action: Action::Select {
                event: choice.event,
                branch: 1,
                cards: vec![100 + ((seat + 1) % 8)],
                targets: vec![],
            },
        };
        assert_eq!(game.submit(wrong), Err(RuleError::InvalidSelection));
        assert_eq!(game.view(None).unwrap(), public);
        game.submit(Command {
            version: own.version,
            seat,
            action: Action::Select {
                event: choice.event,
                branch: 1,
                cards: vec![100 + seat],
                targets: vec![],
            },
        })
        .unwrap();
        assert!(game
            .seat(seat)
            .unwrap()
            .hand
            .iter()
            .any(|card| card.id == 100 + seat));
    }
    assert!(game.view(None).unwrap().waiting_for.is_none());
    assert_eq!(*observations.lock().unwrap(), seats.map(|s| (s, 100 + s)));
    let names: Vec<_> = requests
        .lock()
        .unwrap()
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    let fixture_names: Vec<_> = golden["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            row[0] == "pairs-fixing"
                && row[1] == "request"
                && [
                    "chooseCardOLBegin",
                    "chooseCardBegin",
                    "chooseCardAfter",
                    "chooseCardOLAfter",
                ]
                .contains(&row[2].as_str().unwrap_or(""))
        })
        .map(|row| row[2].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, fixture_names);
}
