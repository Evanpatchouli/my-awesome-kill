//! Optional multiplayer comparison skills, installed explicitly by trusted content.
use crate::{
    ActiveSkillRegistration, ChoiceBranch, CompareMode, Game, SeatId, TriggerContext,
    TriggerEffect, TriggerOrder, TriggerRegistration, TriggerRole, TriggerSelection, TriggerStep,
};
use std::sync::Arc;

fn option(id: u32, label: &str, cards: Vec<crate::CardId>, count: usize) -> ChoiceBranch {
    ChoiceBranch {
        id,
        label: label.into(),
        cards,
        min_cards: count,
        max_cards: count,
        targets: vec![],
        min_targets: 0,
        max_targets: 0,
    }
}
fn active(
    owner: SeatId,
    name: &'static str,
    label: &'static str,
    effect: TriggerEffect,
) -> ActiveSkillRegistration {
    ActiveSkillRegistration {
        owner,
        skill: name.into(),
        limited: false,
        choice: Arc::new(move |game, seat| {
            if game.active_skill_uses_this_phase(seat, name) > 0 {
                return None;
            }
            let targets: Vec<_> = game
                .seat_order_from(seat)
                .ok()?
                .into_iter()
                .filter(|&target| game.can_compare(seat, target))
                .collect();
            if targets.is_empty() {
                return None;
            }
            Some(ChoiceBranch {
                id: 1,
                label: label.into(),
                cards: vec![],
                min_cards: 0,
                max_cards: 0,
                max_targets: targets.len().min(3),
                targets,
                min_targets: 1,
            })
        }),
        effect,
    }
}

/// 鼓舌 performs sequential comparisons with a private choice after each pair.
pub fn gushe(owner: SeatId) -> ActiveSkillRegistration {
    active(
        owner,
        "gushe",
        "鼓舌",
        Arc::new(|ctx, stage, selection| {
            if stage == 0 {
                if let Some(selection) = selection {
                    if ctx
                        .compare_many(
                            ctx.owner(),
                            &selection.targets,
                            CompareMode::Pairwise,
                            Some(Arc::new(gushe_pair)),
                        )
                        .is_ok()
                    {
                        return TriggerStep::Continue(1);
                    }
                }
            }
            TriggerStep::Done
        }),
    )
}

fn loser_choice(ctx: &mut TriggerContext<'_>, loser: SeatId, then: u16) -> TriggerStep {
    if !ctx.game().is_in(loser) {
        return if then == 2 {
            TriggerStep::Continue(3)
        } else {
            TriggerStep::Done
        };
    }
    let cards: Vec<_> = ctx
        .game()
        .hand_and_equipment(loser)
        .map(|cards| cards.map(|card| card.id).collect())
        .unwrap_or_default();
    if cards.is_empty() {
        let _ = ctx.draw(ctx.owner());
        return if then == 2 {
            TriggerStep::Continue(3)
        } else {
            TriggerStep::Done
        };
    }
    TriggerStep::ChooseFor {
        seat: loser,
        branches: vec![
            option(0, "不弃置，令鼓舌者摸牌", vec![], 0),
            option(1, "弃置一张牌", cards, 1),
        ],
        then,
    }
}

fn settle_loser(ctx: &mut TriggerContext<'_>, seat: SeatId, selected: Option<&TriggerSelection>) {
    if selected.is_some_and(|choice| choice.branch == 1) {
        if let Some(id) = selected.and_then(|choice| choice.cards.first()) {
            let _ = ctx.discard_held(seat, &[*id]);
        }
    } else {
        let _ = ctx.draw(ctx.owner());
    }
}

fn gushe_pair(
    ctx: &mut TriggerContext<'_>,
    stage: u16,
    selected: Option<&TriggerSelection>,
) -> TriggerStep {
    let owner = ctx.owner();
    let Some(frame) = ctx.game().group_comparison(ctx.event().subject) else {
        return TriggerStep::Done;
    };
    let Some(target) = frame.target else {
        return TriggerStep::Done;
    };
    let winner = frame.winner;
    match stage {
        0 => {
            if winner != Some(owner) {
                if ctx.add_skill_mark(owner, "gushe", None, 1).is_err() {
                    return TriggerStep::Done;
                }
                if ctx.game().skill_mark_count(owner, "gushe", None) >= 7 {
                    let _ = ctx.die(owner, None);
                }
            }
            TriggerStep::Continue(1)
        }
        1 if winner != Some(owner) => loser_choice(ctx, owner, 2),
        1 => TriggerStep::Continue(3),
        2 => {
            settle_loser(ctx, owner, selected);
            TriggerStep::Continue(3)
        }
        3 if winner != Some(target) => loser_choice(ctx, target, 4),
        3 => TriggerStep::Done,
        4 => {
            settle_loser(ctx, target, selected);
            TriggerStep::Done
        }
        _ => TriggerStep::Done,
    }
}

/// 胆烈 settles all bids at once and damages the selected targets only for a unique win.
pub fn danlie(owner: SeatId) -> ActiveSkillRegistration {
    active(
        owner,
        "twdanlie",
        "胆烈",
        Arc::new(|ctx, stage, selection| match stage {
            0 => {
                if let Some(selection) = selection {
                    if ctx
                        .compare_many(
                            ctx.owner(),
                            &selection.targets,
                            CompareMode::Simultaneous,
                            None,
                        )
                        .is_ok()
                    {
                        return TriggerStep::Continue(1);
                    }
                }
                TriggerStep::Done
            }
            1 => {
                let Some(frame) = ctx.compare_many_result() else {
                    return TriggerStep::Done;
                };
                let winner = frame.result.as_ref().and_then(|result| result.winner);
                let targets = frame.group.targets.clone();
                let owner = ctx.owner();
                if winner == Some(owner) {
                    for target in targets {
                        if ctx.game().is_in(target) {
                            let _ = ctx.damage(Some(owner), target, 1, None, None, &[]);
                        }
                    }
                } else if ctx.game().is_in(owner) {
                    let _ = ctx.lose_hp(owner, 1);
                }
                TriggerStep::Done
            }
            _ => TriggerStep::Done,
        }),
    )
}

fn number_value(game: &Game, event: &crate::TriggerEvent, owner: SeatId) -> Option<i32> {
    let (hp, max) = game.health(owner)?;
    let lost = max.saturating_sub(hp).max(0);
    if lost == 0 {
        return None;
    }
    if let Some(frame) = game.group_comparison(event.subject) {
        if frame.target != Some(owner) && frame.index != 0 {
            return None;
        }
        let number = if frame.player == Some(owner) {
            frame.numbers[0]?
        } else if frame.target == Some(owner) {
            frame.numbers[1]?
        } else {
            return None;
        };
        Some(number.saturating_add(lost).min(13))
    } else {
        let result = game.comparison(event.subject)?;
        let number = if result.participants.initiator == owner {
            result.numbers[0]
        } else if result.participants.target == owner {
            result.numbers[1]
        } else {
            return None;
        };
        Some(number.saturating_add(lost).min(13))
    }
}

/// The same skill identity covers both compare roles and therefore dispatches once per occurrence.
pub fn danlie_number(owner: SeatId) -> Vec<TriggerRegistration> {
    [TriggerRole::Player, TriggerRole::Target]
        .into_iter()
        .map(|role| TriggerRegistration {
            owner,
            skill: "twdanlie_add".into(),
            timing: "compare".into(),
            role,
            order: TriggerOrder::Normal,
            priority: 0,
            label: Some("胆烈".into()),
            silent: true,
            filter: Arc::new(|game, event, owner| number_value(game, event, owner).is_some()),
            effect: Arc::new(|ctx, _, _| {
                if let Some(value) = number_value(ctx.game(), ctx.event(), ctx.owner()) {
                    let _ = ctx.set_compare_number(ctx.owner(), value);
                }
                TriggerStep::Done
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests;
