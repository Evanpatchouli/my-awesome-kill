use super::*;
use std::sync::Arc;

fn veto(game: &mut Game, seat: SeatId, kinds: Vec<CardKind>) {
    let mut skill = crate::TemporarySkill::until_turn_end("test_veto");
    skill.card_enabled = Some(Arc::new(move |_, _, kind| !kinds.contains(&kind)));
    game.grant_temporary(seat, skill).unwrap();
}

#[test]
fn veto_blocks_physical_and_virtual_uses_but_not_recast_or_materials() {
    let mut game = assigned(&["guanyu", "zhouyu", "huanggai", "zhugeliang"], 0);
    let sha = card(1, CardKind::Sha);
    let red = card(2, CardKind::Tiesuo);
    game.seat_mut(0).hand = vec![sha, red];
    set_face(&mut game, red.id, Suit::Heart, 5);
    veto(&mut game, 0, vec![CardKind::Sha, CardKind::Tiesuo]);
    play_phase(&mut game, 0);
    let view = game.view(Some(0)).unwrap();
    assert!(!view.actions.iter().any(|action| matches!(action, Action::PlayCard { card, .. } if *card == sha.id || *card == red.id)));
    assert!(view.actions.contains(&Action::Recast { card: red.id }));
    assert!(!view
        .virtual_plays
        .iter()
        .any(|play| play.kind == CardKind::Sha));
    assert_eq!(
        game.submit(Command {
            version: view.version,
            seat: 0,
            action: Action::PlayCard {
                card: sha.id,
                targets: vec![1]
            }
        }),
        Err(RuleError::InvalidTarget)
    );
    assert_eq!(
        game.submit(Command {
            version: view.version,
            seat: 0,
            action: Action::PlayVirtual {
                kind: CardKind::Sha,
                cards: vec![red.id],
                targets: vec![1]
            }
        }),
        Err(RuleError::InvalidSelection)
    );

    let mut other = assigned(&["guanyu", "zhouyu", "huanggai", "zhugeliang"], 0);
    other.seat_mut(0).hand = vec![red];
    set_face(&mut other, red.id, Suit::Heart, 5);
    veto(&mut other, 0, vec![CardKind::Tiesuo]);
    play_phase(&mut other, 0);
    assert!(other
        .view(Some(0))
        .unwrap()
        .virtual_plays
        .iter()
        .any(|play| play.kind == CardKind::Sha && play.branch.cards.contains(&red.id)));
}

#[test]
fn active_defense_and_counter_are_vetoed_but_passive_responses_are_not() {
    let mut game = assigned(&["guanyu", "zhouyu", "huanggai", "zhugeliang"], 0);
    game.seat_mut(1).hand = vec![
        card(1, CardKind::Shan),
        card(2, CardKind::Sha),
        card(3, CardKind::Wuxie),
    ];
    game.seat_mut(1).equipment.push(card(4, CardKind::Bagua));
    veto(
        &mut game,
        1,
        vec![CardKind::Shan, CardKind::Sha, CardKind::Wuxie],
    );
    game.classic = None;
    start(
        &mut game,
        Program::Equipment(crate::classic::equipment::Flow::response(
            1,
            CardKind::Shan,
            false,
        )),
    );
    assert_ne!(game.view(None).unwrap().waiting_for, Some(1));
    assert!(!game
        .events
        .iter()
        .any(|event| matches!(event.kind, EventKind::Judge { seat: 1 })));

    let mut game = assigned(&["guanyu", "zhouyu", "huanggai", "zhugeliang"], 0);
    game.seat_mut(1).hand = vec![card(2, CardKind::Sha)];
    veto(&mut game, 1, vec![CardKind::Sha]);
    assert_eq!(game.sha_use_branches(1, &[0]).len(), 1);
    game.classic = None;
    start(&mut game, Program::respond(1, CardKind::Sha));
    let choice = game.view(Some(1)).unwrap().choice.unwrap();
    assert!(choice
        .branches
        .iter()
        .any(|branch| branch.cards.contains(&2)));

    let mut game = assigned(&["guanyu", "zhouyu", "huanggai", "zhugeliang"], 0);
    game.seat_mut(1).hand = vec![card(3, CardKind::Wuxie)];
    veto(&mut game, 1, vec![CardKind::Wuxie]);
    let context = crate::CounterContext {
        card_kind: CardKind::Juedou,
        source: Some(0),
        target: 1,
        negated: false,
    };
    game.classic = None;
    start(
        &mut game,
        Program::Equipment(crate::classic::equipment::Flow::counter_response(
            1, context,
        )),
    );
    assert_ne!(game.view(None).unwrap().waiting_for, Some(1));
}

#[test]
fn bot_never_plays_a_vetoed_card() {
    let mut game = assigned(&["guanyu", "zhouyu", "huanggai", "zhugeliang"], 0);
    game.seat_mut(0).hand = vec![card(1, CardKind::Sha)];
    veto(&mut game, 0, vec![CardKind::Sha]);
    assert!(!matches!(
        game.bot_play(game.seat(0).unwrap()),
        Action::PlayCard { .. }
    ));
}
