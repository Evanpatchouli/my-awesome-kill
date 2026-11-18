use crate::{
    CardKind, Game, RuleError, SeatId, TriggerEvent, TriggerId, TriggerRegistration, TriggerRole,
};
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// A veto for an effective card kind being used by the holder, including virtual cards.
pub type CardEnabled = Arc<dyn Fn(&Game, SeatId, CardKind) -> bool + Send + Sync>;
pub type SkillRemovalEffect = Arc<dyn Fn(&mut SkillRemoval<'_>) + Send + Sync>;

/// Checked cleanup operations executed when a grant expires or is revoked.
pub struct SkillRemoval<'a> {
    game: &'a mut Game,
    owner: SeatId,
    queued: bool,
}

impl SkillRemoval<'_> {
    pub fn discard_expansions(&mut self, tag: &str) {
        if let Some(program) = self.game.discard_expansions_program(self.owner, tag) {
            self.game.scheduler.next_cleanup(program);
            self.queued = true;
        }
    }
}

#[derive(Clone)]
pub struct SkillExpiry {
    pub role: TriggerRole,
    pub timing: String,
}

impl SkillExpiry {
    pub fn new(role: TriggerRole, timing: impl Into<String>) -> Self {
        Self {
            role,
            timing: timing.into(),
        }
    }

    fn matches(&self, owner: SeatId, event: &TriggerEvent) -> bool {
        self.timing == event.timing && self.role.matches(owner, event)
    }
}

/// Content and expiry of a named skill granted to one seat.
pub struct TemporarySkill {
    pub name: String,
    pub expires: Vec<SkillExpiry>,
    pub triggers: Vec<TriggerRegistration>,
    pub card_enabled: Option<CardEnabled>,
    pub on_remove: Option<SkillRemovalEffect>,
}

impl TemporarySkill {
    pub fn new(name: impl Into<String>, expires: Vec<SkillExpiry>) -> Self {
        Self {
            name: name.into(),
            expires,
            triggers: Vec::new(),
            card_enabled: None,
            on_remove: None,
        }
    }

    /// Default addTempSkill lifetime, including the next-turn fallback after interruption.
    pub fn until_turn_end(name: impl Into<String>) -> Self {
        Self::new(
            name,
            vec![
                SkillExpiry::new(TriggerRole::Global, "phaseAfter"),
                SkillExpiry::new(TriggerRole::Global, "phaseBeforeStart"),
            ],
        )
    }
}

pub(crate) struct Grant {
    owner: SeatId,
    name: String,
    expires: Vec<SkillExpiry>,
    triggers: Vec<TriggerId>,
    card_enabled: Option<CardEnabled>,
    on_remove: Option<SkillRemovalEffect>,
}

impl Game {
    /// Native and temporarily acquired skills visible to trusted rules content.
    pub fn has_skill(&self, seat: SeatId, id: &str) -> bool {
        self.seat(seat).is_ok_and(|player| {
            player.alive
                && (self.has_active_skill(seat, id)
                    || player
                        .hero
                        .is_some_and(|hero| hero.skills.iter().any(|skill| skill.id == id)))
                || self
                    .temporary_skills
                    .iter()
                    .any(|grant| grant.owner == seat && grant.name == id)
        })
    }

    /// Whether the holder's effective-card modifiers permit this kind of use.
    pub fn card_enabled(&self, seat: SeatId, kind: CardKind) -> bool {
        self.seat(seat).is_ok_and(|player| player.alive)
            && self.temporary_skills.iter().all(|grant| {
                grant.owner != seat
                    || grant
                        .card_enabled
                        .as_ref()
                        .is_none_or(|allow| allow(self, seat, kind))
            })
    }

    pub(crate) fn grant_temporary(
        &mut self,
        owner: SeatId,
        skill: TemporarySkill,
    ) -> Result<(), RuleError> {
        if !self.seat(owner)?.alive {
            return Err(RuleError::InactiveSeat { seat: owner });
        }
        if skill.name.is_empty()
            || skill.expires.is_empty()
            || skill.expires.iter().any(|term| term.timing.is_empty())
            || skill.triggers.iter().any(|r| r.owner != owner)
        {
            return Err(RuleError::InvalidSelection);
        }
        if let Some(grant) = self
            .temporary_skills
            .iter_mut()
            .find(|grant| grant.owner == owner && grant.name == skill.name)
        {
            grant.expires = skill.expires;
            return Ok(());
        }
        if self.has_skill(owner, &skill.name) {
            return Ok(());
        }
        // Validate every registration and the entire ID range before mutating the table.
        self.next_trigger_id
            .checked_add(skill.triggers.len() as u64)
            .ok_or(RuleError::VersionExhausted)?;
        let mut triggers = Vec::with_capacity(skill.triggers.len());
        for registration in skill.triggers {
            triggers.push(self.register_trigger(registration)?);
        }
        self.temporary_skills.push(Grant {
            owner,
            name: skill.name,
            expires: skill.expires,
            triggers,
            card_enabled: skill.card_enabled,
            on_remove: skill.on_remove,
        });
        Ok(())
    }

    pub(crate) fn remove_temporary(&mut self, owner: SeatId, name: &str) -> bool {
        let Some(index) = self
            .temporary_skills
            .iter()
            .position(|grant| grant.owner == owner && grant.name == name)
        else {
            return false;
        };
        self.remove_grant(index);
        true
    }

    fn remove_grant(&mut self, index: usize) -> bool {
        let grant = self.temporary_skills.remove(index);
        for id in grant.triggers {
            self.unregister_trigger(id);
        }
        if let Some(effect) = grant.on_remove {
            let mut removal = SkillRemoval {
                game: self,
                owner: grant.owner,
                queued: false,
            };
            effect(&mut removal);
            return removal.queued;
        }
        false
    }

    pub(crate) fn remove_temporary_for_seat(&mut self, owner: SeatId) {
        while let Some(index) = self
            .temporary_skills
            .iter()
            .position(|grant| grant.owner == owner)
        {
            self.remove_grant(index);
        }
    }

    pub(crate) fn has_expiry_at(&self, timing: &str) -> bool {
        self.temporary_skills
            .iter()
            .any(|grant| grant.expires.iter().any(|term| term.timing == timing))
    }

    pub(crate) fn has_matching_expiry(&self, event: &TriggerEvent) -> bool {
        self.temporary_skills.iter().any(|grant| {
            grant
                .expires
                .iter()
                .any(|term| term.matches(grant.owner, event))
        })
    }

    pub(crate) fn begin_timing_request(&mut self, event: &TriggerEvent) -> bool {
        self.observe_timing_request(event);
        let mut cleanup = false;
        let mut index = 0;
        while index < self.temporary_skills.len() {
            let grant = &self.temporary_skills[index];
            if grant
                .expires
                .iter()
                .any(|term| term.matches(grant.owner, event))
            {
                cleanup |= self.remove_grant(index);
            } else {
                index += 1;
            }
        }
        cleanup || self.has_registered_candidates(event)
    }
}
