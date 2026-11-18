//! The limited team-mode skill from the standard pack's alternate Guan Yu.

use crate::{
    ActiveSkillRegistration, ChoiceBranch, EventKind, SeatId, SkillExpiry, TemporarySkill,
    TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::Arc;

const BUFF: &str = "zhongyi2";

#[cfg(test)]
mod tests;

pub fn registration(owner: SeatId) -> ActiveSkillRegistration {
    ActiveSkillRegistration {
        owner,
        skill: "zhongyi".into(),
        limited: true,
        choice: Arc::new(|game, owner| {
            let cards: Vec<_> = game
                .hand_and_equipment(owner)
                .ok()?
                .map(|card| card.id)
                .collect();
            (!cards.is_empty()).then(|| ChoiceBranch {
                id: 1,
                label: "忠义".into(),
                cards,
                min_cards: 1,
                max_cards: 1,
                targets: Vec::new(),
                min_targets: 0,
                max_targets: 0,
            })
        }),
        effect: Arc::new(|ctx, _, selected| {
            let owner = ctx.owner();
            if ctx.game().is_alive(owner) {
                ctx.grant_temporary(owner, damage_skill(owner))
                    .expect("valid temporary skill");
                if let Some(card) = selected.and_then(|selection| selection.cards.first()) {
                    // A useSkillBegin effect may already have taken the selected card.
                    let _ = ctx.add_to_expansion(owner, *card, BUFF);
                }
            }
            TriggerStep::Done
        }),
    }
}

fn damage_skill(owner: SeatId) -> TemporarySkill {
    let mut skill = TemporarySkill::new(
        BUFF,
        vec![SkillExpiry::new(TriggerRole::Global, "roundStart")],
    );
    skill.on_remove = Some(Arc::new(|cleanup| cleanup.discard_expansions(BUFF)));
    skill.triggers.push(TriggerRegistration {
        owner,
        skill: BUFF.into(),
        timing: "damageBegin1".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::Normal,
        priority: 0,
        label: None,
        silent: false,
        filter: Arc::new(|game, event, owner| {
            event
                .source
                .is_some_and(|source| game.is_friend_of(source, owner))
                && matches!(
                    event.parent.and_then(|parent| game.event_kind(parent)),
                    Some(EventKind::ShaEffect { .. })
                )
        }),
        effect: Arc::new(|ctx, _, _| {
            let _ = ctx.add_damage(1);
            TriggerStep::Done
        }),
    });
    skill
}
