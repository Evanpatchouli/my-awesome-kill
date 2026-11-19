//! Xunxun: privately arrange four cards before the draw phase.
use crate::{
    CardKind, ChoiceBranch, MoveChoiceRequest, MoveRow, MoveStep, SeatId, TriggerOrder,
    TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::Arc;

#[cfg(test)]
mod tests;

fn choice(id: u32, label: &str) -> ChoiceBranch {
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

pub fn registration(owner: SeatId) -> TriggerRegistration {
    TriggerRegistration {
        owner,
        skill: "xunxun".into(),
        timing: "phaseDrawBegin1".into(),
        role: TriggerRole::Player,
        order: TriggerOrder::Normal,
        priority: 0,
        label: Some("恂恂".into()),
        silent: false,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|ctx, stage, selected| {
            let owner = ctx.owner();
            match stage {
                0 => TriggerStep::Choose {
                    branches: vec![choice(0, "放弃"), choice(1, "发动恂恂")],
                    then: 1,
                },
                1 if selected.is_some_and(|s| s.branch == 1) => {
                    if ctx.preview_pile(owner, 4).is_ok() {
                        TriggerStep::Continue(2)
                    } else {
                        TriggerStep::Done
                    }
                }
                2 if ctx.game().is_alive(owner) => {
                    let cards = ctx.preview_cards();
                    if cards.len() < 2 {
                        return TriggerStep::Done;
                    }
                    let n = cards.len();
                    TriggerStep::MoveCards {
                        request: MoveChoiceRequest {
                            seat: owner,
                            prompt: "恂恂：将两张牌置于牌堆底".into(),
                            rows: vec![
                                MoveRow {
                                    label: "牌堆顶".into(),
                                    cards,
                                    min: 0,
                                    max: n,
                                },
                                MoveRow {
                                    label: "牌堆底".into(),
                                    cards: vec![],
                                    min: 2,
                                    max: 2,
                                },
                            ],
                            allow_decline: false,
                            filter_move: None,
                            filter_ok: None,
                            bot: Some(Arc::new(|_, rows| {
                                let mut ranked = rows[0].clone();
                                ranked.sort_by_key(|card| {
                                    std::cmp::Reverse(match card.kind {
                                        CardKind::Tao | CardKind::Wuzhong => 5,
                                        CardKind::Shan
                                        | CardKind::Wuxie
                                        | CardKind::Shunshou
                                        | CardKind::Guohe => 4,
                                        CardKind::Sha | CardKind::Juedou => 3,
                                        CardKind::Lebu | CardKind::Shandian => 1,
                                        _ => 2,
                                    })
                                });
                                let keep = ranked.len() - 2;
                                let mut steps: Vec<_> = ranked[keep..]
                                    .iter()
                                    .enumerate()
                                    .map(|(index, card)| MoveStep {
                                        card: card.id,
                                        row: 1,
                                        index,
                                    })
                                    .collect();
                                steps.extend(ranked[..keep].iter().enumerate().map(
                                    |(index, card)| MoveStep {
                                        card: card.id,
                                        row: 0,
                                        index,
                                    },
                                ));
                                Some(steps)
                            })),
                        },
                        then: 3,
                    }
                }
                3 if ctx.game().is_alive(owner) => {
                    let rows = ctx
                        .move_result()
                        .and_then(|r| (!r.cancelled).then_some(r.rows.as_ref()).flatten())
                        .cloned();
                    if let Some(rows) = rows.filter(|r| r.len() == 2) {
                        let _ = ctx.return_pile_rows(owner, &rows[0], &rows[1]);
                    }
                    TriggerStep::Done
                }
                _ => TriggerStep::Done,
            }
        }),
    }
}
