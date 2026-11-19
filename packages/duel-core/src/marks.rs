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

#[derive(Clone, Copy)]
struct MarkState {
    count: u32,
    shown: bool,
}

#[derive(Default)]
pub(crate) struct State {
    counts: BTreeMap<(SeatId, String, Option<SeatId>), MarkState>,
}

impl Game {
    pub fn skill_marks(&self, seat: SeatId) -> Vec<SkillMark> {
        self.marks
            .counts
            .iter()
            .filter(|((owner, _, _), mark)| *owner == seat && mark.shown)
            .map(|((_, skill, source), mark)| SkillMark {
                skill: skill.clone(),
                count: mark.count,
                source: *source,
            })
            .collect()
    }

    pub fn skill_mark_count(&self, seat: SeatId, skill: &str, source: Option<SeatId>) -> u32 {
        self.marks
            .counts
            .get(&(seat, skill.to_owned(), source))
            .map(|mark| mark.count)
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
            .map(|mark| mark.count)
            .unwrap_or(0)
            .checked_add(count)
            .ok_or(RuleError::InvalidSelection)?;
        self.marks.counts.insert(
            key,
            MarkState {
                count: next,
                shown: true,
            },
        );
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
        let current = self
            .marks
            .counts
            .get(&key)
            .map(|mark| mark.count)
            .unwrap_or(0);
        if count == 0 || count > current {
            return Err(RuleError::InvalidSelection);
        }
        if count == current {
            self.marks.counts.remove(&key);
        } else {
            self.marks.counts.insert(
                key,
                MarkState {
                    count: current - count,
                    shown: true,
                },
            );
        }
        Ok(())
    }

    pub fn clear_skill_mark(&mut self, seat: SeatId, skill: &str, source: Option<SeatId>) {
        self.marks.counts.remove(&(seat, skill.to_owned(), source));
    }

    pub(crate) fn clear_marks_on_death(&mut self, seat: SeatId) {
        for ((owner, _, _), mark) in &mut self.marks.counts {
            if *owner == seat {
                mark.shown = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameSetup, SeatSetup, TemporarySkill};
    use std::sync::Arc;

    #[test]
    fn hidden_counts_can_be_shown_again_and_explicit_removal_clears_storage() {
        let mut game = Game::new(GameSetup {
            active_seat: 0,
            seats: vec![
                SeatSetup {
                    id: 0,
                    hp: 4,
                    hand: vec![],
                },
                SeatSetup {
                    id: 1,
                    hp: 4,
                    hand: vec![],
                },
            ],
        })
        .unwrap();
        game.add_skill_mark(1, "mark", None, 7).unwrap();
        game.clear_marks_on_death(1);
        assert_eq!(game.skill_mark_count(1, "mark", None), 7);
        assert!(game.skill_marks(1).is_empty());
        game.remove_skill_mark(1, "mark", None, 1).unwrap();
        assert_eq!(game.skill_marks(1)[0].count, 6);
        game.clear_marks_on_death(1);
        game.add_skill_mark(1, "mark", None, 1).unwrap();
        assert_eq!(game.skill_marks(1)[0].count, 7);

        let mut skill = TemporarySkill::until_removed("temporary");
        skill.on_remove = Some(Arc::new(|cleanup| cleanup.clear_mark("mark", None)));
        game.grant_temporary(1, skill).unwrap();
        game.clear_marks_on_death(1);
        assert!(game.remove_temporary(1, "temporary"));
        assert_eq!(game.skill_mark_count(1, "mark", None), 0);
        assert!(game.skill_marks(1).is_empty());
    }
}
