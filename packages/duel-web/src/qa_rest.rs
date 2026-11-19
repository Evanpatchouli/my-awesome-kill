use super::Duel;
use duel_core::{
    ActiveSkillRegistration, ChoiceBranch, Game, RestClock, RestUnit, SeatId, TriggerStep,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy)]
enum Operation {
    RestForever,
    RestRound,
    End,
    Die,
    GivePublic,
}

#[wasm_bindgen(js_name = classicRestFixture)]
pub fn classic_rest_fixture(
    seed: u32,
    seat: SeatId,
    hero: &str,
    count: usize,
) -> Result<Duel, JsError> {
    let mut game = Game::classic(seed, seat, hero, count)
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    // The physical next seat carries the independently registered Mowang rule.
    let mowang = (seat + 1) % count as SeatId;
    duel_core::rest_content::install_last_pair(&mut game, mowang)
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    for (operation, skill, label) in [
        (Operation::RestForever, "fixture_rest_forever", "永久休整"),
        (Operation::RestRound, "fixture_rest_round", "休整一轮"),
        (Operation::End, "fixture_rest_end", "结束休整"),
        (Operation::Die, "fixture_die", "令其阵亡"),
        (Operation::GivePublic, "fixture_give_public", "公开赠牌"),
    ] {
        game.register_active_skill(ActiveSkillRegistration {
            owner: seat,
            skill: skill.into(),
            limited: false,
            choice: Arc::new(move |game, owner| {
                let targets: Vec<_> = game
                    .living_seats()
                    .iter()
                    .copied()
                    .filter(|&id| {
                        id != owner
                            && match operation {
                                Operation::End => game.is_resting(id),
                                _ => !game.is_resting(id),
                            }
                    })
                    .collect();
                (!targets.is_empty()).then(|| ChoiceBranch {
                    id: 0,
                    label: label.into(),
                    cards: vec![],
                    min_cards: 0,
                    max_cards: 0,
                    targets,
                    min_targets: 1,
                    max_targets: 1,
                })
            }),
            effect: Arc::new(move |ctx, _, selection| {
                if let Some(&target) = selection.and_then(|s| s.targets.first()) {
                    match operation {
                        Operation::RestForever => ctx.rest(target, RestClock::default()),
                        Operation::RestRound => ctx.rest(
                            target,
                            RestClock {
                                unit: RestUnit::Round,
                                count: 1,
                            },
                        ),
                        Operation::End => ctx.rest_end(target, None),
                        Operation::Die => ctx.die(target, None),
                        Operation::GivePublic => {
                            let card = ctx
                                .game()
                                .hand_and_equipment(ctx.owner())
                                .unwrap()
                                .next()
                                .map(|c| c.id);
                            if let Some(card) = card {
                                ctx.give_cards(ctx.owner(), target, &[card], true)
                            } else {
                                Ok(())
                            }
                        }
                    }
                    .expect("the selected seat belongs to this fixture");
                }
                TriggerStep::Done
            }),
        })
        .map_err(|error| JsError::new(&format!("{error:?}")))?;
    }
    Ok(Duel { game })
}
