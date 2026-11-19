use super::{Flow, RestClock};
use crate::classic::tests::{card, scene};
use crate::classic_data::Role;
use crate::{
    Action, Card, CardKind, Command, CounterContext, EventId, EventKind, Game, Program, SeatId,
    TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::Arc;

fn table() -> Game {
    scene(&[Role::Lord, Role::Rebel, Role::Loyalist, Role::Rebel], 0)
}

fn run(game: &mut Game, program: Program) {
    game.scheduler.start(program);
    game.run(None);
}

fn trick_targets(game: &Game, kind: CardKind) -> Vec<Vec<SeatId>> {
    game.trick_uses(game.seat(0).unwrap(), card(30, kind))
        .unwrap()
        .into_iter()
        .map(|play| play.targets)
        .collect()
}

fn rest_during_use(game: &mut Game, timing: &str, material: Card, target: SeatId) {
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: format!("rest_during_{timing}"),
        timing: timing.into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(move |_, event, _| event.cards.contains(&material)),
        effect: Arc::new(move |ctx, _, _| {
            assert!(ctx.game().resolving.contains(&material));
            ctx.rest(target, RestClock::default()).unwrap();
            TriggerStep::Done
        }),
    })
    .unwrap();
}

fn pass(game: &mut Game, seat: SeatId, event: EventId) {
    game.submit(Command {
        version: game.version,
        seat,
        action: Action::Select {
            event,
            branch: 0,
            cards: vec![],
            targets: vec![],
        },
    })
    .unwrap();
}

#[test]
fn resting_seat_leaves_card_and_compare_choices_until_it_returns() {
    let mut game = table();
    let sha = card(1, CardKind::Sha);
    game.seat_mut(0).hand.push(sha);
    game.seat_mut(1).hand.push(card(2, CardKind::Shan));

    run(
        &mut game,
        Program::Rest(Flow::rest(1, RestClock::default())),
    );
    assert!(game.is_alive(1));
    assert!(game.living_seats().contains(&1));
    assert!(!game.is_in(1));
    assert!(!game.can_compare(0, 1));
    assert!(!game.accepts_sha_target(0, 1));
    assert!(game
        .physical_sha_uses(game.seat(0).unwrap(), sha)
        .iter()
        .all(|play| !play.targets.contains(&1)));
    assert_eq!(trick_targets(&game, CardKind::Juedou), [vec![2], vec![3]]);
    assert_eq!(trick_targets(&game, CardKind::Nanman), [vec![2, 3]]);
    assert_eq!(trick_targets(&game, CardKind::Wugu), [vec![0, 2, 3]]);

    game.decision = None;
    game.scheduler.next(Program::Rest(Flow::end(1, 4)));
    game.run(None);
    assert!(game.is_in(1));
    assert!(game.can_compare(0, 1));
    assert!(game.accepts_sha_target(0, 1));
    assert_eq!(
        trick_targets(&game, CardKind::Juedou),
        [vec![1], vec![2], vec![3]]
    );
    assert_eq!(trick_targets(&game, CardKind::Nanman), [vec![1, 2, 3]]);
}

#[test]
fn sha_target_resting_during_use_does_not_respond_or_take_damage() {
    let mut game = table();
    let sha = card(1, CardKind::Sha);
    let shan = card(2, CardKind::Shan);
    game.seat_mut(0).hand.push(sha);
    game.seat_mut(1).hand.push(shan);
    rest_during_use(&mut game, "useCard", sha, 1);
    let program = game.attack_program(0, vec![sha], vec![1]);
    run(&mut game, program);

    assert!(game.is_resting(1));
    assert_eq!(game.seat(1).unwrap().hp, 4);
    assert!(game.seat(1).unwrap().hand.contains(&shan));
    assert!(!matches!(&game.decision, Some(crate::Decision::Cards(choice)) if choice.seat == 1));
    assert!(!game
        .events
        .iter()
        .any(|record| { matches!(record.kind, EventKind::ShaEffect { target: 1, .. }) }));
    assert_eq!(game.discard.iter().filter(|&&card| card == sha).count(), 1);
    assert!(!game.resolving.contains(&sha));
}

#[test]
fn group_trick_skips_newly_resting_target_and_its_counter_choices() {
    let mut game = table();
    let nanman = card(1, CardKind::Nanman);
    let wuxie = card(2, CardKind::Wuxie);
    let sha = card(3, CardKind::Sha);
    game.seat_mut(0).hand.push(nanman);
    game.seat_mut(1).hand.extend([wuxie, sha]);
    rest_during_use(&mut game, "useCard2", nanman, 1);
    let program = game
        .trick_uses(game.seat(0).unwrap(), nanman)
        .unwrap()
        .pop()
        .unwrap()
        .program;
    run(&mut game, program);
    while let Some(crate::Decision::Cards(choice)) = game.decision.as_ref() {
        assert_ne!(choice.seat, 1);
        let (seat, event) = (choice.seat, choice.event);
        pass(&mut game, seat, event);
    }

    assert_eq!(game.seat(1).unwrap().hp, 4);
    assert_eq!(game.seat(1).unwrap().hand, [wuxie, sha]);
    assert_eq!(game.seat(2).unwrap().hp, 3);
    assert_eq!(game.seat(3).unwrap().hp, 3);
    assert!(!game
        .events
        .iter()
        .any(|record| { matches!(record.kind, EventKind::CounterWindow { seat: 1, .. }) }));
    assert_eq!(
        game.discard.iter().filter(|&&card| card == nanman).count(),
        1
    );
    assert!(!game.resolving.contains(&nanman));
}

#[test]
fn counter_window_resumes_at_the_next_seat_if_a_responder_rests() {
    let mut game = table();
    let wuxie = card(2, CardKind::Wuxie);
    game.seat_mut(1).hand.push(wuxie);
    game.register_trigger(TriggerRegistration {
        owner: 1,
        skill: "rest_before_counter".into(),
        timing: "chooseToUseBegin".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|ctx, _, _| {
            ctx.rest(ctx.owner(), RestClock::default()).unwrap();
            TriggerStep::Done
        }),
    })
    .unwrap();
    let program = game.counter_window(CounterContext {
        card_kind: CardKind::Juedou,
        source: Some(0),
        target: 3,
        negated: false,
    });
    run(&mut game, program);
    let mut responders = Vec::new();
    while let Some(crate::Decision::Cards(choice)) = game.decision.as_ref() {
        assert!(choice.counter.is_some());
        let (seat, event) = (choice.seat, choice.event);
        responders.push(seat);
        pass(&mut game, seat, event);
    }

    assert_eq!(responders, [0, 2, 3]);
    assert!(game.is_resting(1));
    assert_eq!(game.seat(1).unwrap().hand, [wuxie]);
}
