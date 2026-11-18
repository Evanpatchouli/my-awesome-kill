//! Mingdian's play-phase and damage-end entry points share one round's card names.

use crate::{
    ActiveSkillRegistration, Card, CardKind, CardType, CardZone, ChoiceBranch, Game,
    RecastBatchRule, RuleError, SeatId, TriggerContext, TriggerEffect, TriggerOrder,
    TriggerRegistration, TriggerRole, TriggerSelection, TriggerStep,
};
use std::sync::{Arc, Mutex};

const SKILL: &str = "clanmingdian";

#[cfg(test)]
mod tests;

#[derive(Default)]
struct Usage {
    round: u64,
    recast: Vec<CardKind>,
    turn: u64,
    played: Vec<CardKind>,
}

pub fn install(game: &mut Game, owner: SeatId) -> Result<(), RuleError> {
    let usage = Arc::new(Mutex::new(Usage::default()));
    let choice_usage = usage.clone();
    let choice = Arc::new(move |game: &Game, owner: SeatId| {
        let already_recast = {
            let usage = choice_usage.lock().expect("Mingdian usage");
            if usage.round == game.round_number() {
                usage.recast.clone()
            } else {
                Vec::new()
            }
        };
        let cards: Vec<_> = game
            .hand_and_equipment(owner)
            .ok()?
            .filter(|card| {
                card.kind.card_type() == CardType::Basic
                    && !already_recast.contains(&card.kind)
                    && game.can_recast_material(owner, card.id)
            })
            .map(|card| card.id)
            .collect();
        (!cards.is_empty()).then(|| ChoiceBranch {
            id: 1,
            label: "明典".into(),
            min_cards: 1,
            max_cards: cards.len(),
            cards,
            targets: Vec::new(),
            min_targets: 0,
            max_targets: 0,
        })
    });
    let use_state = usage.clone();
    game.register_trigger(TriggerRegistration {
        owner,
        skill: "clanmingdian_card_uses".into(),
        timing: "useCard2".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::First,
        priority: i32::MAX,
        label: None,
        silent: true,
        filter: Arc::new(|_, event, _| {
            event
                .card_kind
                .is_some_and(|kind| kind.card_type() == CardType::Basic)
        }),
        effect: Arc::new(move |ctx, _, _| {
            let turn = ctx.game().turn_number();
            let kind = ctx.event().card_kind.unwrap();
            let mut usage = use_state.lock().expect("Mingdian usage");
            if usage.turn != turn {
                usage.turn = turn;
                usage.played.clear();
            }
            if !usage.played.contains(&kind) {
                usage.played.push(kind);
            }
            TriggerStep::Done
        }),
    })?;

    let effect_usage = usage.clone();
    let effect: TriggerEffect = Arc::new(move |ctx, _, selected| {
        selected
            .filter(|selection| selection.branch == 1)
            .map_or(TriggerStep::Done, |selected| {
                recast(ctx, selected, &effect_usage)
            })
    });
    game.register_active_skill(ActiveSkillRegistration {
        owner,
        skill: SKILL.into(),
        limited: false,
        choice: choice.clone(),
        effect: effect.clone(),
    })?;
    let damage_choice = choice.clone();
    game.register_trigger(TriggerRegistration {
        owner,
        skill: SKILL.into(),
        timing: "damageEnd".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::Normal,
        priority: 0,
        label: Some("明典".into()),
        silent: false,
        filter: Arc::new(move |game, _, owner| choice(game, owner).is_some()),
        effect: Arc::new(move |ctx, stage, selected| {
            if stage == 0 {
                return damage_choice(ctx.game(), ctx.owner()).map_or(
                    TriggerStep::Done,
                    |branch| TriggerStep::Choose {
                        branches: vec![
                            branch,
                            ChoiceBranch {
                                id: 0,
                                label: "放弃".into(),
                                cards: Vec::new(),
                                min_cards: 0,
                                max_cards: 0,
                                targets: Vec::new(),
                                min_targets: 0,
                                max_targets: 0,
                            },
                        ],
                        then: 1,
                    },
                );
            }
            effect(ctx, stage, selected)
        }),
    })?;
    Ok(())
}

fn recast(
    ctx: &mut TriggerContext<'_>,
    selected: &TriggerSelection,
    usage: &Arc<Mutex<Usage>>,
) -> TriggerStep {
    let owner = ctx.owner();
    let Ok(materials) = ctx.game().recast_materials(owner, &selected.cards) else {
        return TriggerStep::Done;
    };
    let round = ctx.game().round_number();
    let mut state = usage.lock().expect("Mingdian usage");
    if state.round != round {
        state.round = round;
        state.recast.clear();
    }
    if materials
        .iter()
        .any(|card| card.kind.card_type() != CardType::Basic || state.recast.contains(&card.kind))
    {
        return TriggerStep::Done;
    }
    let hand = ctx.game().seat(owner).unwrap().hand.clone();
    if ctx
        .recast_batch(owner, &selected.cards, RecastBatchRule::default())
        .is_err()
    {
        return TriggerStep::Done;
    }
    let names = distinct_basics(&materials);
    state.recast.extend(&names);
    drop(state);

    let usage = usage.clone();
    TriggerStep::ContinueWith(Arc::new(move |ctx, _, _| {
        let Some((pile, card)) = bonus(ctx, &usage, &names, &hand) else {
            return TriggerStep::Done;
        };
        let _ = ctx.gain_public_pile_card(owner, pile, card.id);
        TriggerStep::Done
    }))
}

fn distinct_basics(cards: &[Card]) -> Vec<CardKind> {
    let mut kinds = Vec::new();
    for card in cards {
        if card.kind.card_type() == CardType::Basic && !kinds.contains(&card.kind) {
            kinds.push(card.kind);
        }
    }
    kinds
}

fn bonus(
    ctx: &mut TriggerContext<'_>,
    usage: &Mutex<Usage>,
    names: &[CardKind],
    before: &[Card],
) -> Option<(CardZone, Card)> {
    let owner = ctx.owner();
    if !ctx.game().is_alive(owner) {
        return None;
    }
    let formerly_held = distinct_basics(before);
    let now_held = distinct_basics(&ctx.game().seat(owner).ok()?.hand);
    let recast_card = ctx.random_pile_card(|card| names.contains(&card.kind));
    let turn = ctx.game().turn_number();
    let used = {
        let usage = usage.lock().expect("Mingdian usage");
        usage.turn == turn && usage.played.iter().any(|kind| names.contains(kind))
    };
    if !used {
        return None;
    }
    let peach = ctx.random_pile_card(|card| card.kind == CardKind::Tao);
    if peach.is_some() && !formerly_held.contains(&CardKind::Tao) {
        return peach;
    }
    ctx.random_pile_card(|card| {
        card.kind.card_type() == CardType::Basic && !formerly_held.contains(&card.kind)
    })
    .or_else(|| {
        (formerly_held.len() > 3 && now_held.len() < 4)
            .then_some(recast_card)
            .flatten()
    })
}
