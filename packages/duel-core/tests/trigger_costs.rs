use duel_core::{
    Action, Card, CardKind, Command, Game, GameSetup, RuleError, SeatSetup, TriggerContext,
    TriggerOrder, TriggerRegistration, TriggerRole, TriggerSelection, TriggerStep,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const SOURCE: u32 = 9;
const TARGET: u32 = 17;

fn game() -> Game {
    Game::new(GameSetup {
        active_seat: SOURCE,
        seats: vec![
            SeatSetup {
                id: SOURCE,
                hp: 4,
                hand: vec![Card {
                    id: 0,
                    kind: CardKind::Sha,
                }],
            },
            SeatSetup {
                id: TARGET,
                hp: 4,
                hand: (1..=3)
                    .map(|id| Card {
                        id,
                        kind: CardKind::Tao,
                    })
                    .collect(),
            },
        ],
    })
    .unwrap()
}

fn listen(
    game: &mut Game,
    name: &str,
    timing: &str,
    effect: impl Fn(&mut TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) {
    game.register_trigger(TriggerRegistration {
        owner: TARGET,
        skill: name.into(),
        timing: timing.into(),
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

fn resolve_sha(game: &mut Game) {
    game.submit(Command {
        version: 0,
        seat: SOURCE,
        action: Action::PlaySha {
            card: 0,
            target: TARGET,
        },
    })
    .unwrap();
    let waiting = game.view(Some(TARGET)).unwrap();
    game.submit(Command {
        version: waiting.version,
        seat: TARGET,
        action: Action::Choose {
            event: waiting.choice.unwrap().event,
            card: None,
        },
    })
    .unwrap();
}

#[test]
fn same_stage_cannot_spend_a_card_twice_or_partially_claim_an_invalid_batch() {
    let mut game = game();
    let completed = Arc::new(AtomicBool::new(false));
    let stage_completed = completed.clone();
    listen(&mut game, "pay", "damageBegin4", move |ctx, stage, _| {
        if stage == 0 {
            ctx.discard_hand(TARGET, &[1]).unwrap();
            assert_eq!(
                ctx.discard_hand(TARGET, &[2, 1]),
                Err(RuleError::InvalidSelection)
            );
            ctx.discard_hand(TARGET, &[2]).unwrap();
            TriggerStep::Continue(1)
        } else {
            assert_eq!(
                ctx.game().view(Some(TARGET)).unwrap().seats[1].hand_count,
                1
            );
            stage_completed.store(true, Ordering::Relaxed);
            TriggerStep::Done
        }
    });
    resolve_sha(&mut game);
    assert!(completed.load(Ordering::Relaxed));
    let after = game.view(Some(TARGET)).unwrap();
    assert_eq!(
        after.discard.iter().map(|card| card.id).collect::<Vec<_>>(),
        [1, 2, 0]
    );
    assert_eq!(after.seats[1].hp, 3);
}

#[test]
fn nested_loss_can_invalidate_a_later_payment_without_consuming_its_remaining_cards() {
    let mut game = game();
    let body_ran = Arc::new(AtomicBool::new(false));
    let record_body = body_ran.clone();
    listen(
        &mut game,
        "two payments",
        "damageBegin4",
        move |ctx, stage, _| {
            if stage == 0 {
                ctx.discard_hand(TARGET, &[1]).unwrap();
                ctx.discard_hand(TARGET, &[2, 3]).unwrap();
                TriggerStep::Continue(1)
            } else {
                record_body.store(true, Ordering::Relaxed);
                TriggerStep::CancelSubject
            }
        },
    );
    let claimed = AtomicBool::new(false);
    listen(
        &mut game,
        "intervening loss",
        "loseAfter",
        move |ctx, _, _| {
            if !claimed.swap(true, Ordering::Relaxed) {
                ctx.discard_hand(TARGET, &[2]).unwrap();
            }
            TriggerStep::Done
        },
    );
    resolve_sha(&mut game);
    assert!(!body_ran.load(Ordering::Relaxed));
    let after = game.view(Some(TARGET)).unwrap();
    assert_eq!(
        after.discard.iter().map(|card| card.id).collect::<Vec<_>>(),
        [1, 2, 0]
    );
    assert_eq!(
        after.seats[1]
            .hand
            .as_ref()
            .unwrap()
            .iter()
            .map(|card| card.id)
            .collect::<Vec<_>>(),
        [3]
    );
    assert_eq!(after.seats[1].hp, 3);
}
