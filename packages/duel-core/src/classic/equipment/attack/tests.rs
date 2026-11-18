use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{Action, Command, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep};
use std::sync::{Arc, Mutex};

fn registration(
    timing: &str,
    effect: impl Fn(&mut crate::TriggerContext<'_>) -> TriggerStep + Send + Sync + 'static,
) -> TriggerRegistration {
    TriggerRegistration {
        owner: 0,
        skill: format!("probe_{timing}"),
        timing: timing.into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |ctx, _, _| effect(ctx)),
    }
}

#[test]
fn registered_virtual_fire_sha_propagates_paid_damage_without_materials() {
    let mut game = scene(
        &[Role::Lord, Role::Rebel, Role::Loyalist, Role::Renegade],
        0,
    );
    game.classic = None;
    game.seat_mut(0).hp = 3;
    game.seat_mut(1).linked = true;
    game.seat_mut(2).linked = true;
    let tao = card(1, CardKind::Tao);
    game.seat_mut(0).hand.push(tao);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_use = seen.clone();
    game.register_trigger(registration("useCard2", move |ctx| {
        if ctx.event().card_kind == Some(CardKind::Tao) {
            ctx.use_virtual_sha(0, 1, Some(DamageNature::Fire)).unwrap();
        } else if ctx.event().card_kind == Some(CardKind::Sha) {
            seen_use.lock().unwrap().push(ctx.event().cards.clone());
        }
        TriggerStep::Done
    }))
    .unwrap();
    game.register_trigger(registration("damageBegin1", |ctx| {
        if matches!(
            ctx.event().parent.and_then(|id| ctx.game().event_kind(id)),
            Some(EventKind::ShaEffect { .. })
        ) {
            assert!(ctx.event().cards.is_empty());
            ctx.add_damage(1).unwrap();
        }
        TriggerStep::Done
    }))
    .unwrap();
    game.scheduler.start(Program::event(EventKind::UseTao {
        seat: 0,
        target: 0,
        card: tao,
    }));
    game.run(None);
    let choice = game.view(Some(1)).unwrap().choice.unwrap();
    game.submit(Command {
        version: game.version,
        seat: 1,
        action: Action::Choose {
            event: choice.event,
            card: None,
        },
    })
    .unwrap();
    let hits = game.damage_history();
    assert_eq!(hits.len(), 2);
    for (index, hit) in hits.iter().enumerate() {
        assert_eq!(
            (hit.target, hit.amount, hit.nature, hit.propagated),
            (
                (index + 1) as SeatId,
                2,
                Some(DamageNature::Fire),
                index != 0
            )
        );
        assert!(hit.cards.is_empty());
        assert_eq!(hit.card_kind, Some(CardKind::Sha));
    }
    assert_eq!(*seen.lock().unwrap(), vec![Vec::<Card>::new()]);
}

#[test]
fn virtual_sha_rechecks_veto_and_counts_during_own_play_phase() {
    let mut game = scene(
        &[Role::Lord, Role::Rebel, Role::Loyalist, Role::Renegade],
        0,
    );
    assert!(game.content_sha_program(0, 0, None).is_err());
    assert!(game.content_sha_program(0, 2, None).is_err());
    let program = game.content_sha_program(0, 1, None).unwrap();
    let mut veto = crate::TemporarySkill::until_turn_end("forbid_sha");
    veto.card_enabled = Some(Arc::new(|_, _, kind| kind != CardKind::Sha));
    game.grant_temporary(0, veto).unwrap();
    assert!(game.content_sha_program(0, 1, None).is_err());
    game.scheduler.start(program);
    game.run(None);
    assert!(!game.classic.as_ref().unwrap().sha_played);

    let mut game = scene(
        &[Role::Lord, Role::Rebel, Role::Loyalist, Role::Renegade],
        0,
    );
    let program = game.content_sha_program(0, 1, None).unwrap();
    game.scheduler.start(program);
    game.run(None);
    assert!(game.classic.as_ref().unwrap().sha_played);

    let mut game = scene(
        &[Role::Lord, Role::Rebel, Role::Loyalist, Role::Renegade],
        1,
    );
    let program = game.content_sha_program(0, 1, None).unwrap();
    game.scheduler.start(program);
    game.run(None);
    assert!(!game.classic.as_ref().unwrap().sha_played);

    let mut game = scene(
        &[Role::Lord, Role::Rebel, Role::Loyalist, Role::Renegade],
        0,
    );
    let program = game.content_sha_program(0, 1, None).unwrap();
    game.seat_mut(1).alive = false;
    assert!(game.content_sha_program(0, 1, None).is_err());
    game.scheduler.start(program);
    game.run(None);
    assert!(!game.classic.as_ref().unwrap().sha_played);
}
