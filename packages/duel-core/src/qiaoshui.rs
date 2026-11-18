//! Optional Yijiang content authored against the same rules API as the classic table.

use crate::{
    CardType, ChoiceBranch, CompareResult, RuleError, SeatId, TemporarySkill, TriggerContext,
    TriggerOrder, TriggerRegistration, TriggerRole, TriggerSelection, TriggerStep,
};
use std::sync::Arc;

const ADD: u32 = 1;
const REMOVE: u32 = 2;

fn choice(id: u32, label: &str, targets: Vec<SeatId>) -> ChoiceBranch {
    let count = usize::from(!targets.is_empty());
    ChoiceBranch {
        id,
        label: label.into(),
        cards: vec![],
        min_cards: 0,
        max_cards: 0,
        targets,
        min_targets: count,
        max_targets: count,
    }
}

pub fn registration(owner: SeatId) -> TriggerRegistration {
    TriggerRegistration {
        owner,
        skill: "qiaoshui".into(),
        timing: "phaseUseBegin".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::Normal,
        priority: 0,
        label: Some("巧说".into()),
        silent: false,
        filter: Arc::new(|game, _, owner| {
            game.seat_order_from(owner)
                .is_ok_and(|seats| seats.into_iter().any(|seat| game.can_compare(owner, seat)))
        }),
        effect: Arc::new(|ctx, stage, selected| {
            let owner = ctx.owner();
            match stage {
                0 => {
                    let seats = ctx.game().seat_order_from(owner).expect("registered seat");
                    let targets = seats
                        .into_iter()
                        .filter(|&seat| ctx.game().can_compare(owner, seat))
                        .collect();
                    TriggerStep::Choose {
                        branches: vec![
                            choice(0, "放弃", vec![]),
                            choice(1, "选择拼点对手", targets),
                        ],
                        then: 1,
                    }
                }
                1 => {
                    if let Some(target) = selected.and_then(|s| s.targets.first()) {
                        if ctx.compare(owner, *target).is_ok() {
                            return TriggerStep::Continue(2);
                        }
                    }
                    TriggerStep::Done
                }
                _ => {
                    if let Some(result) = ctx.compare_result() {
                        if ctx.game().is_alive(owner) {
                            grant_result(ctx, result).expect("valid comparison grant");
                        }
                    }
                    TriggerStep::Done
                }
            }
        }),
    }
}

/// Grant the consequence of the comparison after its physical cleanup finishes.
pub fn grant_result(ctx: &mut TriggerContext<'_>, result: CompareResult) -> Result<(), RuleError> {
    let skill = if result.won {
        win_skill(ctx.owner())
    } else {
        let mut skill = TemporarySkill::until_turn_end("qiaoshui2");
        skill.card_enabled = Some(Arc::new(|_, _, kind| {
            !matches!(kind.card_type(), CardType::Trick | CardType::Delay)
        }));
        skill
    };
    ctx.grant_temporary(ctx.owner(), skill)
}

pub fn win_skill(owner: SeatId) -> TemporarySkill {
    let mut skill = TemporarySkill::until_turn_end("qiaoshui3");
    skill.triggers.push(TriggerRegistration {
        owner,
        skill: "qiaoshui3".into(),
        timing: "useCard2".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, event, _| {
            event
                .card_kind
                .is_some_and(|kind| matches!(kind.card_type(), CardType::Basic | CardType::Trick))
        }),
        effect: Arc::new(change_targets),
    });
    skill
}

fn change_targets(
    ctx: &mut TriggerContext<'_>,
    stage: u16,
    selected: Option<&TriggerSelection>,
) -> TriggerStep {
    let subject = ctx.event().subject;
    if stage == 0 {
        ctx.remove_temporary(ctx.owner(), "qiaoshui3");
        let add = ctx.game().additional_use_targets(subject);
        let remove = ctx.game().removable_use_targets(subject);
        let mut branches = vec![choice(0, "放弃", vec![])];
        match (add.is_empty(), remove.is_empty()) {
            (true, true) => return TriggerStep::Done,
            (false, true) => branches.push(choice(ADD, "巧说：增加目标", add)),
            (true, false) => branches.push(choice(REMOVE, "巧说：减少目标", remove)),
            (false, false) => {
                branches.push(choice(ADD, "巧说：增加目标", vec![]));
                branches.push(choice(REMOVE, "巧说：减少目标", vec![]));
                return TriggerStep::Choose { branches, then: 1 };
            }
        }
        return TriggerStep::Choose { branches, then: 2 };
    }
    let Some(selected) = selected else {
        return TriggerStep::Done;
    };
    if selected.branch == 0 {
        return TriggerStep::Done;
    }
    if stage == 1 {
        let branch = if selected.branch == ADD {
            choice(
                ADD,
                "巧说：选择额外目标",
                ctx.game().additional_use_targets(subject),
            )
        } else {
            choice(
                REMOVE,
                "巧说：选择要移除的目标",
                ctx.game().removable_use_targets(subject),
            )
        };
        if branch.targets.is_empty() {
            return TriggerStep::Done;
        }
        return TriggerStep::Choose {
            branches: vec![branch],
            then: 2,
        };
    }
    if let Some(&target) = selected.targets.first() {
        if selected.branch == ADD {
            let _ = ctx.add_use_target(target);
        } else {
            let _ = ctx.remove_use_target(target);
        }
    }
    TriggerStep::Done
}
