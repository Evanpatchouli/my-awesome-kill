//! Optional content implementations built on the generic phase and trigger APIs.
use crate::{
    ChoiceBranch, Phase, SeatId, TemporarySkill, TriggerContext, TriggerEvent, TriggerOrder,
    TriggerRegistration, TriggerRole, TriggerSelection, TriggerStep,
};
use std::sync::Arc;

fn option(id: u32, label: &str) -> ChoiceBranch {
    ChoiceBranch {
        id,
        label: label.into(),
        cards: vec![],
        min_cards: 0,
        max_cards: 0,
        targets: vec![],
        min_targets: 0,
        max_targets: 0,
    }
}

fn registration(
    owner: SeatId,
    skill: &str,
    timing: &str,
    effect: impl Fn(&mut TriggerContext<'_>, u16, Option<&TriggerSelection>) -> TriggerStep
        + Send
        + Sync
        + 'static,
) -> TriggerRegistration {
    TriggerRegistration {
        owner,
        skill: skill.into(),
        timing: timing.into(),
        role: TriggerRole::Player,
        order: TriggerOrder::Normal,
        priority: 0,
        label: Some(skill.into()),
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(effect),
    }
}

/// 当先 adds an ordinary play phase at the start of its holder's turn.
pub fn dangxian(owner: SeatId) -> TriggerRegistration {
    registration(owner, "dangxian", "phaseBegin", |ctx, _, _| {
        let _ = ctx.insert_next_phase(Phase::Play, "dangxian");
        TriggerStep::Done
    })
}

/// 放权 offers the owner an opportunity to cancel a play phase.
pub fn fangquan(owner: SeatId) -> TriggerRegistration {
    let mut trigger = registration(
        owner,
        "fangquan",
        "phaseUseBefore",
        |ctx, stage, selection| match stage {
            0 => TriggerStep::Choose {
                branches: vec![option(0, "不发动放权"), option(1, "发动放权")],
                then: 1,
            },
            1 if selection.is_some_and(|s| s.branch == 1) => {
                let owner = ctx.owner();
                if ctx.grant_temporary(owner, fangquan2(owner)).is_err()
                    || ctx.add_skill_mark(owner, "fangquan2", None, 1).is_err()
                {
                    return TriggerStep::Done;
                }
                TriggerStep::CancelSubject
            }
            _ => TriggerStep::Done,
        },
    );
    trigger.filter = Arc::new(|game, _, owner| {
        !game.has_skill(owner, "fangquan3")
            && game.seat(owner).is_ok_and(|seat| !seat.hand.is_empty())
    });
    trigger
}

fn fangquan2(owner: SeatId) -> TemporarySkill {
    let mut skill = TemporarySkill::until_turn_end("fangquan2");
    skill.on_remove = Some(Arc::new(|cleanup| cleanup.clear_mark("fangquan2", None)));
    let mut trigger =
        registration(
            owner,
            "fangquan2",
            "phaseEnd",
            |ctx, stage, selection| match stage {
                0 => {
                    let cards = ctx
                        .game()
                        .seat(ctx.owner())
                        .map(|seat| seat.hand.iter().map(|card| card.id).collect::<Vec<_>>())
                        .unwrap_or_default();
                    if cards.is_empty() {
                        return TriggerStep::Done;
                    }
                    let mut pay = option(1, "弃置一张手牌");
                    pay.cards = cards;
                    pay.min_cards = 1;
                    pay.max_cards = 1;
                    TriggerStep::Choose {
                        branches: vec![option(0, "不发动放权"), pay],
                        then: 1,
                    }
                }
                1 if selection.is_some_and(|s| s.branch == 1) => {
                    let Some(card) = selection.and_then(|s| s.cards.first()).copied() else {
                        return TriggerStep::Done;
                    };
                    if ctx.discard_hand(ctx.owner(), &[card]).is_err() {
                        return TriggerStep::Done;
                    }
                    TriggerStep::Continue(2)
                }
                2 => {
                    let targets = ctx
                        .game()
                        .in_order_from(ctx.owner())
                        .into_iter()
                        .filter(|&seat| seat != ctx.owner())
                        .collect::<Vec<_>>();
                    if targets.is_empty() {
                        return TriggerStep::Done;
                    }
                    let mut target = option(1, "选择额外回合角色");
                    target.targets = targets;
                    target.min_targets = 1;
                    target.max_targets = 1;
                    TriggerStep::Choose {
                        branches: vec![target],
                        then: 3,
                    }
                }
                3 => {
                    let Some(target) = selection.and_then(|s| s.targets.first()).copied() else {
                        return TriggerStep::Done;
                    };
                    let owner = ctx.owner();
                    if !ctx.game().is_in(target)
                        || target == owner
                        || ctx.insert_extra_turn(target, "fangquan2").is_err()
                    {
                        return TriggerStep::Done;
                    }
                    let _ = ctx.remove_skill_mark(owner, "fangquan2", None, 1);
                    let _ = ctx.add_skill_mark(target, "fangquan", Some(owner), 1);
                    let _ = ctx.grant_temporary(target, fangquan3(target));
                    TriggerStep::Done
                }
                _ => TriggerStep::Done,
            },
        );
    trigger.silent = false;
    trigger.filter = Arc::new(|game, _, owner| {
        game.skill_mark_count(owner, "fangquan2", None) > 0
            && game.seat(owner).is_ok_and(|seat| !seat.hand.is_empty())
            && game.in_order_from(owner).iter().any(|&seat| seat != owner)
    });
    skill.indexed_triggers.push((
        trigger,
        Arc::new(|game: &crate::Game, _: &TriggerEvent, owner| {
            game.skill_mark_count(owner, "fangquan2", None)
        }),
    ));
    skill
}

fn fangquan3(owner: SeatId) -> TemporarySkill {
    let mut skill = TemporarySkill::until_removed("fangquan3");
    for timing in ["phaseAfter", "phaseCancelled"] {
        skill
            .triggers
            .push(registration(owner, "fangquan3", timing, |ctx, _, _| {
                let owner = ctx.owner();
                let sources = ctx
                    .game()
                    .skill_marks(owner)
                    .into_iter()
                    .filter(|mark| mark.skill == "fangquan")
                    .map(|mark| mark.source)
                    .collect::<Vec<_>>();
                for source in sources {
                    ctx.clear_skill_mark(owner, "fangquan", source);
                }
                ctx.remove_temporary(owner, "fangquan3");
                TriggerStep::Done
            }));
    }
    skill
}

#[cfg(test)]
mod tests;
