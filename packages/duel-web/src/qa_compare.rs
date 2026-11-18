use super::Duel;
use duel_core::classic_data::Suit;
use duel_core::{
    ChoiceBranch, Game, SeatId, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// Open a comparison on the first play phase, using the ordinarily dealt hands.
#[wasm_bindgen(js_name = classicCompareFixture)]
pub fn classic_compare_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
    human_is_target: bool,
    pause_before_result: bool,
) -> Result<Duel, JsError> {
    if !(3..=10).contains(&count) || seat as usize >= count {
        return Err(JsError::new(
            "The comparison needs two players and a witness",
        ));
    }
    let other = (seat + 1) % count as SeatId;
    let (initiator, target) = if human_is_target {
        (other, seat)
    } else {
        (seat, other)
    };
    let opening = TriggerRegistration {
        owner: seat,
        skill: "compare_fixture".into(),
        timing: "phaseUseBegin".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|game, _, _| {
            game.view(None)
                .is_ok_and(|view| view.turn.is_some_and(|turn| turn.number == 1))
        }),
        effect: Arc::new(move |ctx, stage, _| {
            if stage == 0 {
                ctx.compare(initiator, target).unwrap();
                return TriggerStep::Continue(1);
            }
            let result = ctx
                .compare_result()
                .expect("the comparison resumed its content");
            assert_eq!(result.participants.initiator, initiator);
            assert_eq!(result.participants.target, target);
            TriggerStep::Done
        }),
    };
    let mut content = vec![opening];
    if !human_is_target {
        content.push(TriggerRegistration {
            owner: initiator,
            skill: "tianbian_number".into(),
            timing: "compare".into(),
            role: TriggerRole::Player,
            order: TriggerOrder::Normal,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|game, event, _| {
                let Some(result) = game.comparison(event.subject) else {
                    return false;
                };
                game.view(None).is_ok_and(|view| {
                    view.card_faces
                        .iter()
                        .any(|face| face.id == result.cards[0].id && face.suit == Suit::Heart)
                })
            }),
            effect: Arc::new(|ctx, _, _| {
                ctx.set_compare_number(ctx.owner(), 13).unwrap();
                TriggerStep::Done
            }),
        });
    }
    if pause_before_result {
        content.push(TriggerRegistration {
            owner: seat,
            skill: "compare_reveal_fixture".into(),
            timing: "compareFixing".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::Last,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(|_, stage, _| {
                if stage == 0 {
                    return TriggerStep::Choose {
                        branches: vec![ChoiceBranch {
                            id: 1,
                            label: "继续结算拼点".into(),
                            cards: vec![],
                            targets: vec![],
                            min_cards: 0,
                            max_cards: 0,
                            min_targets: 0,
                            max_targets: 0,
                        }],
                        then: 1,
                    };
                }
                TriggerStep::Done
            }),
        });
    }
    let game = Game::classic_with_triggers(seed, seat, hero, count, content)
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    Ok(Duel { game })
}
