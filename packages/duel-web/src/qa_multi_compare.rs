use super::Duel;
use duel_core::classic_data::Suit;
use duel_core::{
    CardKind, ChoiceBranch, Game, SeatId, TriggerOrder, TriggerRegistration, TriggerRole,
    TriggerStep,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = classicMultiCompareFixture)]
pub fn classic_multi_compare_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
    scenario: &str,
) -> Result<Duel, JsError> {
    if count != 8 || seat != 0 || !["gushe", "danlie"].contains(&scenario) {
        return Err(JsError::new("Invalid comparison fixture"));
    }
    let danlie = scenario == "danlie";
    let setup = TriggerRegistration {
        owner: seat,
        skill: "multi_fixture".into(),
        timing: "phaseZhunbeiBegin".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|game, _, _| {
            game.view(None)
                .is_ok_and(|view| view.turn.is_some_and(|turn| turn.number == 1))
        }),
        effect: Arc::new(move |ctx, _, _| {
            for (i, target) in [0, 1, 3, 4].into_iter().enumerate() {
                let old: Vec<_> = ctx.game().view(Some(target)).unwrap().seats[target as usize]
                    .hand
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|c| c.id)
                    .collect();
                ctx.discard_hand(target, &old).unwrap();
                let rank = if danlie {
                    [10, 9, 10, 8][i]
                } else {
                    [7, 8, 10, 12][i]
                };
                ctx.gain_created_card(
                    target,
                    CardKind::Tao,
                    [Suit::Heart, Suit::Diamond, Suit::Club, Suit::Spade][i],
                    rank,
                    None,
                )
                .unwrap();
            }
            if danlie {
                ctx.lose_hp(seat, 1).unwrap();
            }
            TriggerStep::Done
        }),
    };
    let fixing = TriggerRegistration {
        owner: seat,
        skill: "multi_fixture_fixing".into(),
        timing: "compareFixing".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Last,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(move |game, event, _| {
            game.group_comparison(event.subject)
                .is_some_and(|frame| !danlie || frame.index == 0)
        }),
        effect: Arc::new(move |ctx, stage, _| {
            if stage == 0 {
                if !danlie
                    && ctx
                        .game()
                        .group_comparison(ctx.event().subject)
                        .is_some_and(|frame| frame.target == Some(1))
                {
                    ctx.set_compare_number(seat, 10).unwrap();
                }
                return TriggerStep::Choose {
                    branches: vec![ChoiceBranch {
                        id: 1,
                        label: "继续拼点结算".into(),
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
    };
    let mut content = vec![setup, fixing];
    if danlie {
        content.extend(duel_core::compare_content::danlie_number(seat));
    }
    let mut game = Game::classic_with_triggers(seed, seat, hero, count, content)
        .map_err(|e| JsError::new(&format!("{e:?}")))?;
    game.register_active_skill(if danlie {
        duel_core::compare_content::danlie(seat)
    } else {
        duel_core::compare_content::gushe(seat)
    })
    .map_err(|e| JsError::new(&format!("{e:?}")))?;
    Ok(Duel { game })
}
