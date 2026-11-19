//! Mowang with the final two attendants, as exercised in the pinned identity table.
use crate::{
    Game, RestClock, RestUnit, RuleError, SeatId, TriggerEffect, TriggerFilter, TriggerId,
    TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use std::sync::Arc;

/// Install Mowang's death/rest loop with a companion rule for forming the next party.
/// The companion owns the remaining `mbdanggu` marks and resolves its own choices.
pub fn install_mowang(
    game: &mut Game,
    owner: SeatId,
    clique: TriggerEffect,
) -> Result<Vec<TriggerId>, RuleError> {
    let mut ids = Vec::new();
    let always: TriggerFilter = Arc::new(|_, _, _| true);
    let mut add = |skill: &str, timing: &str, priority, force_inactive, filter, effect| {
        let id = game.register_trigger(TriggerRegistration {
            owner,
            skill: skill.into(),
            timing: timing.into(),
            role: TriggerRole::Player,
            order: TriggerOrder::Normal,
            priority,
            silent: true,
            label: None,
            filter,
            effect,
        })?;
        if force_inactive {
            game.set_trigger_force_die(id, true)?;
            game.set_trigger_force_out(id, true)?;
        }
        ids.push(id);
        Ok::<(), RuleError>(())
    };
    add(
        "mbmowang_die",
        "phaseAfter",
        0,
        false,
        always.clone(),
        Arc::new(|ctx, _, _| {
            let _ = ctx.die(ctx.owner(), None);
            TriggerStep::Done
        }),
    )?;
    add(
        "mbmowang",
        "dieBefore",
        15,
        true,
        Arc::new(|game, _, seat| game.health(seat).is_some_and(|(_, max)| max > 0)),
        Arc::new(|ctx, _, _| {
            let seat = ctx.owner();
            if ctx.game().is_resting(seat) {
                return TriggerStep::CancelSubject;
            }
            if ctx.game().skill_mark_count(seat, "mbdanggu", None) > 0 {
                ctx.reserve_death(ctx.event().subject, &["mbdanggu"])
                    .expect("the trigger belongs to this death");
            }
            TriggerStep::Done
        }),
    )?;
    add(
        "mbmowang",
        "dieAfter",
        15,
        true,
        Arc::new(|game, event, _| game.death_reserved(event.subject)),
        Arc::new(|ctx, _, _| {
            let seat = ctx.owner();
            if ctx.game().skill_mark_count(seat, "mbdanggu", None) > 0 {
                ctx.rest(
                    seat,
                    RestClock {
                        unit: RestUnit::Round,
                        count: 1,
                    },
                )
                .expect("the resting owner exists");
            }
            TriggerStep::Done
        }),
    )?;
    add(
        "mbdanggu_clique",
        "mbdangguClique",
        0,
        false,
        always.clone(),
        clique,
    )?;
    add(
        "mbmowang_return",
        "restEnd",
        0,
        true,
        always,
        Arc::new(|ctx, step, _| {
            let seat = ctx.owner();
            if step == 0 {
                ctx.emit("mbdangguClique", Some(seat), None, None, 0, &[]);
                TriggerStep::Continue(1)
            } else {
                let _ = ctx.draw(seat);
                TriggerStep::Done
            }
        }),
    )?;
    Ok(ids)
}

/// The reference fixture starts with Zhang Rang and Zhao Zhong as the only
/// remaining attendants. Its acceptance boundary is the pair consumption and
/// return draw. Other party selections and the attendants' card skills are separate content.
pub fn install_last_pair(game: &mut Game, owner: SeatId) -> Result<Vec<TriggerId>, RuleError> {
    game.add_skill_mark(owner, "mbdanggu", None, 2)?;
    install_mowang(
        game,
        owner,
        Arc::new(|ctx, _, _| {
            let seat = ctx.owner();
            if ctx.game().skill_mark_count(seat, "mbdanggu", None) == 2 {
                ctx.clear_skill_mark(seat, "mbdanggu", None);
            }
            TriggerStep::Done
        }),
    )
}
