use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{Action, Command, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep};
use std::sync::{Arc, Mutex};

#[test]
fn eight_seat_sha_miss_and_use_end_preserve_subject_material_and_order() {
    for virtual_sha in [false, true] {
        for cancel_miss in [false, true] {
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
            let sha = card(10, CardKind::Sha);
            let shan = card(11, CardKind::Shan);
            if !virtual_sha {
                game.seat_mut(0).hand.push(sha);
            }
            game.seat_mut(1).hand.push(shan);
            let cards = if virtual_sha { vec![] } else { vec![sha] };
            let seen = Arc::new(Mutex::new(Vec::new()));
            for timing in ["shaMiss", "useCardEnd", "useCardAfter"] {
                let seen = seen.clone();
                let expected = cards.clone();
                game.register_trigger(TriggerRegistration {
                    owner: 0,
                    skill: timing.into(),
                    timing: timing.into(),
                    role: TriggerRole::Global,
                    order: TriggerOrder::Normal,
                    priority: 0,
                    label: None,
                    silent: true,
                    filter: Arc::new(|_, _, _| true),
                    effect: Arc::new(move |ctx, _, _| {
                        let event = ctx.event();
                        assert_eq!(event.player, Some(0));
                        assert_eq!(event.cards, expected);
                        assert_eq!(event.card_kind, Some(CardKind::Sha));
                        if timing == "shaMiss" {
                            assert_eq!(event.target, Some(1));
                            assert!(matches!(
                                ctx.game().event_kind(event.subject),
                                Some(EventKind::ShaEffect { seat: 0, target: 1 })
                            ));
                            assert!(matches!(
                                event.parent.and_then(|id| ctx.game().event_kind(id)),
                                Some(
                                    EventKind::UseShaTargets { .. }
                                        | EventKind::UseVirtual {
                                            kind: CardKind::Sha,
                                            ..
                                        }
                                )
                            ));
                        } else {
                            assert!(matches!(
                                ctx.game().event_kind(event.subject),
                                Some(
                                    EventKind::UseShaTargets { .. }
                                        | EventKind::UseVirtual {
                                            kind: CardKind::Sha,
                                            ..
                                        }
                                )
                            ));
                        }
                        seen.lock().unwrap().push(timing);
                        if cancel_miss && timing == "shaMiss" {
                            TriggerStep::CancelSubject
                        } else {
                            TriggerStep::Done
                        }
                    }),
                })
                .unwrap();
            }
            game.scheduler
                .start(game.attack_program(0, cards, vec![1, 2]));
            game.run(None);
            for (seat, response) in [(1, Some(shan.id)), (2, None)] {
                let choice = game.view(Some(seat)).unwrap().choice.unwrap();
                game.submit(Command {
                    version: game.version,
                    seat,
                    action: Action::Choose {
                        event: choice.event,
                        card: response,
                    },
                })
                .unwrap();
            }
            assert_eq!(
                *seen.lock().unwrap(),
                ["shaMiss", "useCardEnd", "useCardAfter"]
            );
            assert_eq!(game.seat(2).unwrap().hp, 3);
        }
    }
}
