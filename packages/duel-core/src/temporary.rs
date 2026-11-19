use crate::{
    CardKind, Game, RuleError, Seat, SeatId, TriggerEvent, TriggerId, TriggerRegistration,
    TriggerRole,
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
    cleanup: &'a mut Vec<crate::Program>,
}

impl SkillRemoval<'_> {
    pub fn clear_mark(&mut self, skill: &str, source: Option<SeatId>) {
        self.game.clear_skill_mark(self.owner, skill, source);
    }
    pub fn discard_expansions(&mut self, tag: &str) {
        if let Some(program) = self.game.discard_expansions_program(self.owner, tag) {
            self.cleanup.push(program);
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
    pub indexed_triggers: Vec<(TriggerRegistration, crate::triggers::TriggerIndex)>,
    pub card_enabled: Option<CardEnabled>,
    pub on_remove: Option<SkillRemovalEffect>,
}

impl TemporarySkill {
    pub fn new(name: impl Into<String>, expires: Vec<SkillExpiry>) -> Self {
        Self {
            name: name.into(),
            expires,
            triggers: Vec::new(),
            indexed_triggers: Vec::new(),
            card_enabled: None,
            on_remove: None,
        }
    }

    /// Stays active until a skill callback revokes the grant or its holder dies.
    pub fn until_removed(name: impl Into<String>) -> Self {
        Self::new(name, Vec::new())
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
        self.seat(seat)
            .is_ok_and(|player| self.seat_has_skill(player, id))
    }

    pub(crate) fn seat_has_skill(&self, player: &Seat, id: &str) -> bool {
        crate::equipment::has_equipment_skill(player, id)
            || self.seat_has_non_equipment_skill(player, id)
    }

    pub(crate) fn seat_has_non_equipment_skill(&self, player: &Seat, id: &str) -> bool {
        (id == "undist" && player.rest_undist)
            || self
                .general(player)
                .is_some_and(|hero| hero.skills.iter().any(|skill| skill.id == id))
            || self.owns_active_skill(player.id, id)
            || self
                .temporary_skills
                .iter()
                .any(|grant| grant.owner == player.id && grant.name == id)
    }

    pub(crate) fn add_qinggang_use(&mut self, owner: SeatId, use_event: crate::EventId) {
        self.grant_temporary(owner, TemporarySkill::until_turn_end("qinggang2"))
            .expect("a live target can gain qinggang2");
        let uses = &mut self.seat_mut(owner).qinggang_uses;
        if !uses.contains(&use_event) {
            uses.push(use_event);
        }
    }
    pub(crate) fn has_qinggang_use(&self, owner: SeatId, use_event: crate::EventId) -> bool {
        self.seat(owner)
            .is_ok_and(|seat| seat.qinggang_uses.contains(&use_event))
    }
    pub(crate) fn eligible_qinggang_cleanup(
        &self,
        owner: SeatId,
        use_event: crate::EventId,
    ) -> bool {
        self.is_in(owner)
            && self.has_skill(owner, "qinggang2")
            && self.has_qinggang_use(owner, use_event)
    }
    pub(crate) fn remove_qinggang_use(&mut self, owner: SeatId, use_event: crate::EventId) {
        let uses = &mut self.seat_mut(owner).qinggang_uses;
        let Some(index) = uses.iter().position(|id| *id == use_event) else {
            return;
        };
        uses.remove(index);
        if uses.is_empty() {
            self.revoke_active_ownership(owner, "qinggang2");
            self.remove_temporary(owner, "qinggang2");
        }
    }
    /// Whether the holder's effective-card modifiers permit this kind of use.
    pub fn card_enabled(&self, seat: SeatId, kind: CardKind) -> bool {
        self.is_in(seat)
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
            || skill.expires.iter().any(|term| term.timing.is_empty())
            || skill.triggers.iter().any(|r| r.owner != owner)
            || skill.indexed_triggers.iter().any(|(r, _)| r.owner != owner)
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
            .checked_add(skill.triggers.len() as u64 + skill.indexed_triggers.len() as u64)
            .ok_or(RuleError::VersionExhausted)?;
        let mut triggers = Vec::with_capacity(skill.triggers.len());
        for registration in skill.triggers {
            triggers.push(self.register_trigger(registration)?);
        }
        for (registration, count) in skill.indexed_triggers {
            triggers.push(self.register_indexed_trigger(registration, count)?);
        }
        self.seat_mut(owner).equipment.register_hook(&skill.name);
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
        let mut cleanup = Vec::new();
        self.remove_grant(index, &mut cleanup);
        for program in cleanup {
            self.scheduler.next_cleanup(program);
        }
        true
    }

    pub(crate) fn remove_temporary_with_cleanup(
        &mut self,
        owner: SeatId,
        name: &str,
        cleanup: &mut Vec<crate::Program>,
    ) -> bool {
        let Some(index) = self
            .temporary_skills
            .iter()
            .position(|grant| grant.owner == owner && grant.name == name)
        else {
            return false;
        };
        self.remove_grant(index, cleanup);
        true
    }

    fn remove_grant(&mut self, index: usize, cleanup: &mut Vec<crate::Program>) {
        let grant = self.temporary_skills.remove(index);
        if grant.name == "qinggang2" {
            self.seat_mut(grant.owner).qinggang_uses.clear();
        }
        self.seat_mut(grant.owner)
            .equipment
            .remove_hook(&grant.name);
        for id in grant.triggers {
            self.unregister_trigger(id);
        }
        if let Some(effect) = grant.on_remove {
            let mut removal = SkillRemoval {
                game: self,
                owner: grant.owner,
                cleanup,
            };
            effect(&mut removal);
        }
    }

    pub(crate) fn remove_temporary_for_seat(&mut self, owner: SeatId) {
        while let Some(index) = self
            .temporary_skills
            .iter()
            .position(|grant| grant.owner == owner)
        {
            let mut cleanup = Vec::new();
            self.remove_grant(index, &mut cleanup);
            for program in cleanup {
                self.scheduler.next_cleanup(program);
            }
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
        let mut cleanup = Vec::new();
        let result = self.begin_timing_request_with_cleanup(event, &mut cleanup);
        for program in cleanup {
            self.scheduler.next_cleanup(program);
        }
        result
    }

    pub(crate) fn begin_timing_request_with_cleanup(
        &mut self,
        event: &TriggerEvent,
        cleanup: &mut Vec<crate::Program>,
    ) -> bool {
        self.observe_timing_request(event);
        let start = cleanup.len();
        let mut index = 0;
        while index < self.temporary_skills.len() {
            let grant = &self.temporary_skills[index];
            if grant
                .expires
                .iter()
                .any(|term| term.matches(grant.owner, event))
            {
                self.remove_grant(index, cleanup);
            } else {
                index += 1;
            }
        }
        cleanup.len() != start || self.has_registered_candidates(event)
    }
}
