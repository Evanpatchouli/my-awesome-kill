//! Install declarative self-targeting content skills.

use std::sync::Arc;

use crate::content::{Effect, SkillDefinition, SkillTiming};
use crate::{
    ActiveSkillRegistration, ChoiceBranch, Game, RuleError, SeatId, TriggerEffect, TriggerOrder,
    TriggerRegistration, TriggerRole, TriggerStep,
};

pub(crate) fn install<'a>(
    game: &mut Game,
    owner: SeatId,
    skills: impl IntoIterator<Item = &'a SkillDefinition>,
) -> Result<(), RuleError> {
    for skill in skills {
        let effects = skill.effects.clone();
        let effect: TriggerEffect = Arc::new(move |ctx, _, _| {
            let owner = ctx.owner();
            for &effect in &effects {
                let result = match effect {
                    Effect::Draw { count } => ctx.draw_cards(owner, count),
                    Effect::Recover { count } => ctx.recover(owner, count),
                };
                if result.is_err() {
                    break;
                }
            }
            TriggerStep::Done
        });

        match &skill.timing {
            SkillTiming::PlayOnce => {
                let id = skill.id.clone();
                let label = skill.name.clone();
                game.register_active_skill(ActiveSkillRegistration {
                    owner,
                    skill: skill.id.clone(),
                    limited: false,
                    choice: Arc::new(move |game, seat| {
                        (game.active_skill_uses_this_phase(seat, &id) == 0).then(|| ChoiceBranch {
                            id: 1,
                            label: label.clone(),
                            cards: Vec::new(),
                            min_cards: 0,
                            max_cards: 0,
                            targets: Vec::new(),
                            min_targets: 0,
                            max_targets: 0,
                        })
                    }),
                    effect,
                })?;
            }
            SkillTiming::DrawAfter | SkillTiming::DamageAfter => {
                let timing = match &skill.timing {
                    SkillTiming::DrawAfter => "phaseDrawAfter",
                    SkillTiming::DamageAfter => "damageAfter",
                    SkillTiming::PlayOnce => unreachable!(),
                };
                game.register_trigger(TriggerRegistration {
                    owner,
                    skill: skill.id.clone(),
                    timing: timing.into(),
                    role: TriggerRole::Player,
                    order: TriggerOrder::Normal,
                    priority: 0,
                    label: Some(skill.name.clone()),
                    silent: false,
                    filter: Arc::new(|_, _, _| true),
                    effect,
                })?;
            }
        }
    }
    Ok(())
}
