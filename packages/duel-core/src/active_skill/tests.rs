mod qinggang_revocation;

use super::*;
use crate::{
    Action, Card, CardKind, Command, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::Mutex;

fn table() -> Game {
    let mut game = Game::classic(5, 0, "guanyu", 8).unwrap();
    game.seat_mut(0).hand = vec![Card {
        id: 10001,
        kind: CardKind::Sha,
    }];
    game.seat_mut(1).hand = vec![Card {
        id: 10002,
        kind: CardKind::Tao,
    }];
    game
}

fn skill(ran: Arc<Mutex<Vec<Vec<CardId>>>>) -> ActiveSkillRegistration {
    ActiveSkillRegistration {
        owner: 0,
        skill: "limited_test".into(),
        limited: true,
        choice: Arc::new(|game, seat| {
            Some(ChoiceBranch {
                id: 4,
                label: "test".into(),
                cards: game.seat(seat).unwrap().hand.iter().map(|c| c.id).collect(),
                min_cards: 1,
                max_cards: 1,
                targets: vec![1],
                min_targets: 1,
                max_targets: 1,
            })
        }),
        effect: Arc::new(move |_, stage, selected| {
            assert_eq!(stage, 0);
            let selected = selected.unwrap();
            assert_eq!(selected.targets, [1]);
            ran.lock().unwrap().push(selected.cards.clone());
            TriggerStep::Done
        }),
    }
}
fn command(
    game: &mut Game,
    seat: SeatId,
    registration: u64,
    cards: Vec<CardId>,
    targets: Vec<SeatId>,
) -> Result<(), RuleError> {
    game.submit(Command {
        version: game.version,
        seat,
        action: Action::ActivateSkill {
            registration,
            cards,
            targets,
        },
    })
}
fn trigger(
    effect: impl Fn(&mut crate::TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) -> TriggerRegistration {
    TriggerRegistration {
        owner: 0,
        skill: "before".into(),
        timing: "useSkillBegin".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(effect),
    }
}
#[test]
fn limited_success_is_private_validated_and_once_per_seat_and_name() {
    let mut game = table();
    let ran = Arc::new(Mutex::new(Vec::new()));
    let id = game.register_active_skill(skill(ran.clone())).unwrap();
    let other = game.register_active_skill(skill(ran.clone())).unwrap();
    assert!(game.has_active_skill(0, "limited_test"));
    assert_eq!(game.active_plays(0).len(), 2);
    let own = game.view(Some(0)).unwrap();
    assert_eq!(own.active_plays[0].branch.cards, [10001]);
    assert!(game.view(Some(1)).unwrap().active_plays.is_empty());
    assert!(game.view(None).unwrap().active_plays.is_empty());
    for (seat, registration, cards, targets) in [
        (0, id, vec![10002], vec![1]),
        (0, id, vec![10001, 10001], vec![1]),
        (0, id, vec![10001], vec![0]),
        (1, id, vec![10001], vec![1]),
        (0, id + 100, vec![10001], vec![1]),
    ] {
        let before = game.view(Some(0)).unwrap();
        assert!(command(&mut game, seat, registration, cards, targets).is_err());
        assert_eq!(game.view(Some(0)).unwrap(), before);
        assert!(!game.limited_skill_used(0, "limited_test"));
    }
    command(&mut game, 0, id, vec![10001], vec![1]).unwrap();
    assert_eq!(*ran.lock().unwrap(), [vec![10001]]);
    assert_eq!(game.limited_spent(0), ["limited_test"]);
    assert!(game.active_plays(0).is_empty());
    assert!(command(&mut game, 0, other, vec![10001], vec![1]).is_err());
    assert!(game.unregister_active_skill(id));
    assert!(game.active_skill(id).is_none());
    assert!(game.has_active_skill(0, "limited_test"));
}
#[test]
fn cancelled_begin_does_not_consume_but_moved_material_does() {
    for cancel in [true, false] {
        let mut game = table();
        let ran = Arc::new(Mutex::new(Vec::new()));
        let id = game.register_active_skill(skill(ran.clone())).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = requests.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if ["useSkillBegin", "useSkillAfter"].contains(&event.timing.as_str()) {
                assert!(event.cards.is_empty());
                assert!(matches!(
                    game.event_kind(event.subject),
                    Some(EventKind::ActivateSkill { seat: 0, .. })
                ));
                observed.lock().unwrap().push(event.timing.clone());
            }
        }));
        game.register_trigger(trigger(move |ctx, _, _| {
            if cancel {
                TriggerStep::CancelSubject
            } else {
                ctx.discard_hand(0, &[10001]).unwrap();
                TriggerStep::Done
            }
        }))
        .unwrap();
        command(&mut game, 0, id, vec![10001], vec![1]).unwrap();
        assert_eq!(game.limited_skill_used(0, "limited_test"), !cancel);
        assert_eq!(ran.lock().unwrap().len(), usize::from(!cancel));
        let expected = if cancel {
            vec!["useSkillBegin"]
        } else {
            vec!["useSkillBegin", "useSkillAfter"]
        };
        assert_eq!(*requests.lock().unwrap(), expected);
        if !cancel {
            assert!(!game.seat(0).unwrap().hand.iter().any(|c| c.id == 10001));
        }
    }
}
