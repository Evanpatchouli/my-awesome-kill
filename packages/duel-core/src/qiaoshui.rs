//! Optional Yijiang content authored against the same rules API as the classic table.

use crate::{CardType, RuleError, TemporarySkill, TriggerContext};
use std::sync::Arc;

/// Apply Qiaoshui's restriction after a tied or lost comparison.
pub fn grant_loss(ctx: &mut TriggerContext<'_>) -> Result<(), RuleError> {
    let mut skill = TemporarySkill::until_turn_end("qiaoshui2");
    skill.card_enabled = Some(Arc::new(|_, _, kind| {
        !matches!(kind.card_type(), CardType::Trick | CardType::Delay)
    }));
    ctx.grant_temporary(ctx.owner(), skill)
}
