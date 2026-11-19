//! Generic registered play-phase skills and per-seat limited-use marks.
use crate::scheduler::Outcome;
use crate::{
    ActivePlay, CardId, ChoiceBranch, EventKind, Game, Program, Resolution, RuleError, SeatId,
    TriggerEffect, TriggerSelection,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[cfg(test)]
mod tests;

pub type ActiveSkillId = u64;

#[derive(Clone)]
pub struct ActiveSkillRegistration {
    pub owner: SeatId,
    pub skill: String,
    pub limited: bool,
    pub choice: Arc<dyn Fn(&Game, SeatId) -> Option<ChoiceBranch> + Send + Sync>,
    pub effect: TriggerEffect,
}

#[derive(Default)]
pub(crate) struct State {
    next_id: ActiveSkillId,
    registrations: BTreeMap<ActiveSkillId, ActiveSkillRegistration>,
    revoked: BTreeSet<ActiveSkillId>,
    spent: BTreeSet<(SeatId, String)>,
    last_attempt: BTreeMap<ActiveSkillId, crate::EventId>,
    phase_uses: BTreeMap<(SeatId, String), (crate::EventId, u32)>,
}

#[derive(Clone, Copy)]
enum Stage {
    Begin,
    Accepted,
    AfterEffect,
    Finish,
}

#[derive(Clone)]
pub(crate) struct Flow {
    phase: crate::EventId,
    owner: SeatId,
    registration: ActiveSkillId,
    skill: String,
    limited: bool,
    effect: TriggerEffect,
    selection: TriggerSelection,
    stage: Stage,
}
impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        EventKind::ActivateSkill {
            seat: self.owner,
            registration: self.registration,
        }
    }
}

impl Game {
    pub fn active_skill_uses_this_phase(&self, owner: SeatId, skill: &str) -> u32 {
        let Some((phase, count)) = self
            .active_skills
            .phase_uses
            .get(&(owner, skill.to_owned()))
        else {
            return 0;
        };
        self.scheduler
            .active_events()
            .find_map(|(event, program)| match program.kind() {
                EventKind::Phase {
                    seat,
                    phase: crate::Phase::Play,
                } if seat == owner => Some(event.id),
                _ => None,
            })
            .filter(|event| event == phase)
            .map_or(0, |_| *count)
    }
    pub(crate) fn bot_active_skill(&self, seat: SeatId) -> Option<crate::Action> {
        let phase = self.scheduler.current_id();
        self.active_plays(seat).into_iter().find_map(|play| {
            if self.active_skills.last_attempt.get(&play.registration) == Some(&phase) {
                return None;
            }
            let cards = play.branch.cards.get(..play.branch.min_cards)?.to_vec();
            let targets = play.branch.targets.get(..play.branch.min_targets)?.to_vec();
            self.plan_active_skill(seat, play.registration, cards.clone(), targets.clone())
                .ok()?;
            Some(crate::Action::ActivateSkill {
                registration: play.registration,
                cards,
                targets,
            })
        })
    }

    pub fn register_active_skill(
        &mut self,
        registration: ActiveSkillRegistration,
    ) -> Result<ActiveSkillId, RuleError> {
        self.seat(registration.owner)?;
        if registration.skill.is_empty() {
            return Err(RuleError::InvalidSelection);
        }
        let id = self
            .active_skills
            .next_id
            .checked_add(1)
            .ok_or(RuleError::VersionExhausted)?;
        self.active_skills.next_id = id;
        self.seat_mut(registration.owner)
            .equipment
            .register_hook(&registration.skill);
        self.active_skills.registrations.insert(id, registration);
        Ok(id)
    }

    pub fn unregister_active_skill(&mut self, id: ActiveSkillId) -> bool {
        self.active_skills.last_attempt.remove(&id);
        self.active_skills.revoked.remove(&id);
        let Some(registration) = self.active_skills.registrations.remove(&id) else {
            return false;
        };
        if !self.has_skill(registration.owner, &registration.skill) {
            if registration.skill == "qinggang2" {
                self.seat_mut(registration.owner).qinggang_uses.clear();
            }
            self.seat_mut(registration.owner)
                .equipment
                .remove_hook(&registration.skill);
        }
        true
    }

    pub fn active_skill(&self, id: ActiveSkillId) -> Option<&ActiveSkillRegistration> {
        self.active_skills.registrations.get(&id)
    }

    pub fn limited_spent(&self, seat: SeatId) -> Vec<String> {
        self.active_skills
            .spent
            .iter()
            .filter(|(owner, _)| *owner == seat)
            .map(|(_, name)| name.clone())
            .collect()
    }

    pub fn limited_skill_used(&self, seat: SeatId, skill: &str) -> bool {
        self.active_skills.spent.contains(&(seat, skill.to_owned()))
    }

    pub fn has_active_skill(&self, seat: SeatId, skill: &str) -> bool {
        self.is_in(seat) && self.owns_active_skill(seat, skill)
    }

    pub(crate) fn owns_active_skill(&self, seat: SeatId, skill: &str) -> bool {
        self.active_skills.registrations.iter().any(|(id, r)| {
            !self.active_skills.revoked.contains(id) && r.owner == seat && r.skill == skill
        })
    }

    pub(crate) fn revoke_active_ownership(&mut self, owner: SeatId, skill: &str) {
        self.active_skills.revoked.extend(
            self.active_skills
                .registrations
                .iter()
                .filter_map(|(&id, r)| (r.owner == owner && r.skill == skill).then_some(id)),
        );
    }

    pub fn active_plays(&self, seat: SeatId) -> Vec<ActivePlay> {
        if self.checked_play_seat(seat).is_err() || !self.is_in(seat) {
            return Vec::new();
        }
        self.active_skills
            .registrations
            .iter()
            .filter_map(|(&id, r)| {
                if self.active_skills.revoked.contains(&id)
                    || r.owner != seat
                    || (r.limited && self.limited_skill_used(seat, &r.skill))
                {
                    return None;
                }
                (r.choice)(self, seat).map(|branch| ActivePlay {
                    registration: id,
                    skill: r.skill.clone(),
                    branch,
                })
            })
            .collect()
    }

    pub(crate) fn plan_active_skill(
        &self,
        seat: SeatId,
        registration: ActiveSkillId,
        cards: Vec<CardId>,
        targets: Vec<SeatId>,
    ) -> Result<Program, RuleError> {
        self.checked_play_seat(seat)?;
        if !self.is_in(seat) {
            return Err(RuleError::InactiveSeat { seat });
        }
        if self.active_skills.revoked.contains(&registration) {
            return Err(RuleError::InvalidSelection);
        }
        let r = self
            .active_skills
            .registrations
            .get(&registration)
            .filter(|r| r.owner == seat)
            .ok_or(RuleError::InvalidSelection)?;
        if r.limited && self.limited_skill_used(seat, &r.skill) {
            return Err(RuleError::InvalidSelection);
        }
        let branch = (r.choice)(self, seat).ok_or(RuleError::InvalidSelection)?;
        if !Self::matches_branch(&branch, &cards, &targets) {
            return Err(RuleError::InvalidSelection);
        }
        let player = self.seat(seat)?;
        if !cards.iter().all(|id| {
            player
                .hand
                .iter()
                .chain(player.equipment.iter())
                .any(|card| card.id == *id)
        }) {
            return Err(RuleError::InvalidCard);
        }
        Ok(Program::ActiveSkill(Flow {
            phase: self.scheduler.current_id(),
            owner: seat,
            registration,
            skill: r.skill.clone(),
            limited: r.limited,
            effect: r.effect.clone(),
            selection: TriggerSelection {
                branch: branch.id,
                cards,
                targets,
            },
            stage: Stage::Begin,
        }))
    }

    pub(crate) fn resolve_active_skill(
        &mut self,
        mut flow: Flow,
        child: Option<Outcome<Resolution>>,
    ) {
        match flow.stage {
            Stage::Begin => {
                if let Some(parent) = self
                    .scheduler
                    .active_frame(self.scheduler.current_id())
                    .and_then(|(frame, _)| frame.parent)
                {
                    self.active_skills
                        .last_attempt
                        .insert(flow.registration, parent);
                }
                flow.stage = Stage::Accepted;
                self.queue_timing("useSkillBegin", Some(flow.owner), None, None, 0, &[]);
                self.scheduler.replace_current(Program::ActiveSkill(flow));
            }
            Stage::Accepted => {
                if matches!(child, Some(Outcome::Cancelled))
                    || self.active_skills.revoked.contains(&flow.registration)
                    || !self
                        .active_skills
                        .registrations
                        .contains_key(&flow.registration)
                    || !self.is_in(flow.owner)
                    || self.match_result().is_some()
                {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                if flow.limited {
                    self.active_skills
                        .spent
                        .insert((flow.owner, flow.skill.clone()));
                }
                let uses = self
                    .active_skills
                    .phase_uses
                    .entry((flow.owner, flow.skill.clone()))
                    .or_insert((flow.phase, 0));
                *uses = (
                    flow.phase,
                    if uses.0 == flow.phase {
                        uses.1.saturating_add(1)
                    } else {
                        1
                    },
                );
                let event = self.subject_timing("useSkill", Some(flow.owner), None, None, 0, &[]);
                let effect = crate::triggers::Registered::active(
                    flow.registration,
                    flow.owner,
                    event,
                    flow.effect.clone(),
                    flow.selection.clone(),
                );
                flow.stage = Stage::AfterEffect;
                self.scheduler.replace_current(Program::ActiveSkill(flow));
                self.scheduler.next(Program::Registered(effect));
            }
            Stage::AfterEffect => {
                flow.stage = Stage::Finish;
                self.queue_timing("useSkillAfter", Some(flow.owner), None, None, 0, &[]);
                self.scheduler.replace_current(Program::ActiveSkill(flow));
            }
            Stage::Finish => self.finish(),
        }
    }
}
