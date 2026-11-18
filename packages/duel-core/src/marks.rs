use crate::{Game, RuleError, SeatId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A public, named counter; source distinguishes marks granted by different players.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMark {
    pub skill: String,
    pub count: u32,
    pub source: Option<SeatId>,
}

#[derive(Default)]
pub(crate) struct State {
    counts: BTreeMap<(SeatId, String, Option<SeatId>), u32>,
}

impl Game {
    pub fn skill_marks(&self, seat: SeatId) -> Vec<SkillMark> {
        self.marks
            .counts
            .iter()
            .filter(|((owner, _, _), _)| *owner == seat)
            .map(|((_, skill, source), &count)| SkillMark {
                skill: skill.clone(),
                count,
                source: *source,
            })
            .collect()
    }

    pub fn skill_mark_count(&self, seat: SeatId, skill: &str, source: Option<SeatId>) -> u32 {
        self.marks
            .counts
            .get(&(seat, skill.to_owned(), source))
            .copied()
            .unwrap_or(0)
    }

    pub fn add_skill_mark(
        &mut self,
        seat: SeatId,
        skill: &str,
        source: Option<SeatId>,
        count: u32,
    ) -> Result<(), RuleError> {
        self.seat(seat)?;
        if let Some(id) = source {
            self.seat(id)?;
        }
        if skill.is_empty() || count == 0 {
            return Err(RuleError::InvalidSelection);
        }
        let key = (seat, skill.to_owned(), source);
        let next = self
            .marks
            .counts
            .get(&key)
            .copied()
            .unwrap_or(0)
            .checked_add(count)
            .ok_or(RuleError::InvalidSelection)?;
        self.marks.counts.insert(key, next);
        Ok(())
    }

    pub fn remove_skill_mark(
        &mut self,
        seat: SeatId,
        skill: &str,
        source: Option<SeatId>,
        count: u32,
    ) -> Result<(), RuleError> {
        self.seat(seat)?;
        let key = (seat, skill.to_owned(), source);
        let current = self.marks.counts.get(&key).copied().unwrap_or(0);
        if count == 0 || count > current {
            return Err(RuleError::InvalidSelection);
        }
        if count == current {
            self.marks.counts.remove(&key);
        } else {
            self.marks.counts.insert(key, current - count);
        }
        Ok(())
    }

    pub fn clear_skill_mark(&mut self, seat: SeatId, skill: &str, source: Option<SeatId>) {
        self.marks.counts.remove(&(seat, skill.to_owned(), source));
    }

    pub(crate) fn clear_marks_on_death(&mut self, seat: SeatId) {
        self.marks.counts.retain(|(owner, _, _), _| *owner != seat);
    }
}
