use crate::scheduler::{EventRef, Outcome};
use crate::{Card, CardId, EventKind, Game, Program, Resolution, RuleError, SeatId, Zone};

#[derive(Clone)]
pub(crate) enum Flow {
    Add {
        owner: SeatId,
        card: Card,
        zone: Zone,
        tag: String,
        stage: AddStage,
    },
    Discard {
        owner: SeatId,
        cards: Vec<Card>,
        tag: Option<String>,
        stage: DiscardStage,
    },
    Lose {
        owner: SeatId,
        cards: Vec<Card>,
        tag: Option<String>,
        moved: Vec<Card>,
        stage: LoseStage,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum AddStage {
    Before,
    Begin,
    Lose,
    Move,
    End,
    After,
    Finish,
}
#[derive(Clone, Copy)]
pub(crate) enum DiscardStage {
    Before,
    Begin,
    Lose,
    End,
    EndWithoutLoss,
    After,
    Finish,
}
#[derive(Clone, Copy)]
pub(crate) enum LoseStage {
    Before,
    Begin,
    Move,
    End,
    After,
    Finish,
}

impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::Add { owner, .. } => EventKind::AddToExpansion {
                seat: *owner,
                amount: 1,
            },
            Self::Discard {
                owner, cards, tag, ..
            } => {
                if tag.is_some() {
                    EventKind::LoseToDiscardpile {
                        seat: *owner,
                        amount: cards.len(),
                    }
                } else {
                    EventKind::DiscardGroup {
                        seat: *owner,
                        amount: cards.len(),
                    }
                }
            }
            Self::Lose { owner, cards, .. } => EventKind::Lose {
                seat: *owner,
                amount: cards.len(),
            },
        }
    }
}

impl Game {
    /// Physical expansion cards carrying this exact tag, in zone order.
    pub fn expansions(&self, seat: SeatId, tag: &str) -> Vec<Card> {
        self.seat(seat).map_or_else(
            |_| Vec::new(),
            |player| {
                player
                    .expansions
                    .iter()
                    .copied()
                    .filter(|card| self.expansion_tags(card.id).iter().any(|t| t == tag))
                    .collect()
            },
        )
    }

    pub fn expansion_tags(&self, card: CardId) -> &[String] {
        self.expansion_tags.get(&card).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn expansion_program(
        &self,
        owner: SeatId,
        card: Card,
        tag: String,
    ) -> Result<Program, RuleError> {
        let player = self.seat(owner)?;
        if !self.is_in(owner) {
            return Err(RuleError::InactiveSeat { seat: owner });
        }
        let zone = if player.hand.contains(&card) {
            Zone::Hand
        } else if player.equipment.contains(&card) {
            Zone::Equipment
        } else if player.judges.contains(&card) {
            Zone::Judgment
        } else {
            return Err(RuleError::InvalidCard);
        };
        Ok(Program::Expansion(Flow::Add {
            owner,
            card,
            zone,
            tag,
            stage: AddStage::Before,
        }))
    }

    pub(crate) fn discard_expansions_program(&self, owner: SeatId, tag: &str) -> Option<Program> {
        let cards = self.expansions(owner, tag);
        (!cards.is_empty()).then(|| {
            Program::Expansion(Flow::Discard {
                owner,
                cards,
                tag: Some(tag.into()),
                stage: DiscardStage::Before,
            })
        })
    }

    pub(crate) fn discard_all_expansions_program(&self, owner: SeatId) -> Option<Program> {
        let cards = self.seat(owner).ok()?.expansions.clone();
        (!cards.is_empty()).then(|| {
            Program::Expansion(Flow::Discard {
                owner,
                cards,
                tag: None,
                stage: DiscardStage::Before,
            })
        })
    }

    pub(crate) fn gain_expansion_program(
        &self,
        from: SeatId,
        to: SeatId,
        card: Card,
    ) -> Result<Program, RuleError> {
        if !self.seat(from)?.expansions.contains(&card) {
            return Err(RuleError::InvalidCard);
        }
        self.seat(to)?;
        if !self.is_in(to) {
            return Err(RuleError::InactiveSeat { seat: to });
        }
        Ok(self.expansion_gain_program(from, to, card))
    }

    pub(crate) fn resolve_expansion(
        &mut self,
        _event: EventRef,
        mut flow: Flow,
        child: Option<Outcome<Resolution>>,
    ) {
        match &mut flow {
            Flow::Add {
                owner,
                card,
                zone,
                tag,
                stage,
            } => {
                let (owner, card, zone) = (*owner, *card, *zone);
                match *stage {
                    AddStage::Before => {
                        *stage = if zone == Zone::Judgment {
                            AddStage::Begin
                        } else {
                            AddStage::Lose
                        };
                        let queued = self.queue_timing(
                            "addToExpansionBefore",
                            Some(owner),
                            Some(owner),
                            None,
                            0,
                            &[card],
                        );
                        self.scheduler.replace_current(Program::Expansion(flow));
                        if queued {
                            return;
                        }
                    }
                    AddStage::Begin => {
                        if matches!(child, Some(Outcome::Cancelled)) {
                            self.scheduler.end(Outcome::Cancelled);
                            return;
                        }
                        *stage = AddStage::Lose;
                        self.queue_timing(
                            "addToExpansionBegin",
                            Some(owner),
                            Some(owner),
                            None,
                            0,
                            &[card],
                        );
                        self.scheduler.replace_current(Program::Expansion(flow));
                    }
                    AddStage::Lose => {
                        if matches!(child, Some(Outcome::Cancelled)) || !self.is_in(owner) {
                            self.scheduler.end(Outcome::Cancelled);
                            return;
                        }
                        let held = self.seat(owner).is_ok_and(|p| p.zone(zone).contains(&card));
                        if !held {
                            self.scheduler.end(Outcome::Cancelled);
                            return;
                        }
                        // A give becomes public only after the cancellable outer before hook.
                        self.known_to_all.insert(card.id);
                        *stage = AddStage::Move;
                        let loss = self.lose_to_own_special(owner, card, zone);
                        self.scheduler.replace_current(Program::Expansion(flow));
                        self.scheduler.next(loss);
                        return;
                    }
                    AddStage::Move => {
                        let moved = matches!(child, Some(Outcome::Finished(Resolution::RecastMove { card: Some(moved), .. })) if moved == card);
                        if !moved {
                            self.scheduler.end(Outcome::Cancelled);
                            return;
                        }
                        let Some(index) = self.transferring.iter().position(|c| *c == card) else {
                            self.scheduler.end(Outcome::Cancelled);
                            return;
                        };
                        // The gain finishes with the material still in special if the owner left play.
                        if self.is_in(owner) {
                            let material = self.transferring.remove(index);
                            self.seat_mut(owner).expansions.push(material);
                            let tags = self.expansion_tags.entry(material.id).or_default();
                            if !tags.contains(tag) {
                                tags.push(tag.clone());
                            }
                        }
                        *stage = if zone == Zone::Judgment {
                            AddStage::End
                        } else {
                            AddStage::After
                        };
                        self.scheduler.replace_current(Program::Expansion(flow));
                    }
                    AddStage::End => {
                        *stage = AddStage::After;
                        self.queue_timing(
                            "addToExpansionEnd",
                            Some(owner),
                            Some(owner),
                            None,
                            0,
                            &[card],
                        );
                        self.scheduler.replace_current(Program::Expansion(flow));
                    }
                    AddStage::After => {
                        *stage = AddStage::Finish;
                        let queued = self.queue_timing(
                            "addToExpansionAfter",
                            Some(owner),
                            Some(owner),
                            None,
                            0,
                            &[card],
                        );
                        self.scheduler.replace_current(Program::Expansion(flow));
                        if queued {
                            return;
                        }
                    }
                    AddStage::Finish => {
                        self.finish();
                        return;
                    }
                }
            }
            Flow::Discard {
                owner,
                cards,
                tag,
                stage,
            } => {
                let owner = *owner;
                match *stage {
                    DiscardStage::Before => {
                        if tag.is_none() {
                            cards.retain(|card| {
                                self.seat(owner).is_ok_and(|p| p.expansions.contains(card))
                            });
                            if cards.is_empty() {
                                self.finish();
                                return;
                            }
                        }
                        *stage = DiscardStage::Begin;
                        let queued = tag.is_some()
                            && self.queue_timing(
                                "loseToDiscardpileBefore",
                                Some(owner),
                                None,
                                None,
                                0,
                                cards,
                            );
                        self.scheduler.replace_current(Program::Expansion(flow));
                        if queued {
                            return;
                        }
                    }
                    DiscardStage::Begin => {
                        if matches!(child, Some(Outcome::Cancelled)) {
                            self.scheduler.end(Outcome::Cancelled);
                            return;
                        }
                        *stage = DiscardStage::Lose;
                        let queued = tag.is_some()
                            && self.queue_timing(
                                "loseToDiscardpileBegin",
                                Some(owner),
                                None,
                                None,
                                0,
                                cards,
                            );
                        self.scheduler.replace_current(Program::Expansion(flow));
                        if queued {
                            return;
                        }
                    }
                    DiscardStage::Lose => {
                        if matches!(child, Some(Outcome::Cancelled)) {
                            self.scheduler.end(Outcome::Cancelled);
                            return;
                        }
                        if tag.is_some() && !self.seat(owner).is_ok_and(|p| p.alive) {
                            *stage = DiscardStage::End;
                            self.scheduler.replace_current(Program::Expansion(flow));
                            return;
                        }
                        let eligible: Vec<_> = cards
                            .iter()
                            .copied()
                            .filter(|card| {
                                self.seat(owner).is_ok_and(|p| p.expansions.contains(card))
                                    && tag.as_ref().is_none_or(|tag| {
                                        self.expansion_tags(card.id).iter().any(|t| t == tag)
                                    })
                            })
                            .collect();
                        if tag.is_some() && eligible.is_empty() {
                            *stage = DiscardStage::EndWithoutLoss;
                            self.scheduler.replace_current(Program::Expansion(flow));
                            return;
                        }
                        let loss = Flow::Lose {
                            owner,
                            cards: eligible,
                            tag: tag.clone(),
                            moved: vec![],
                            stage: LoseStage::Before,
                        };
                        *stage = DiscardStage::End;
                        self.scheduler.replace_current(Program::Expansion(flow));
                        self.scheduler.next(Program::Expansion(loss));
                        return;
                    }
                    DiscardStage::End | DiscardStage::EndWithoutLoss => {
                        if matches!(*stage, DiscardStage::End) {
                            match child {
                                Some(Outcome::Finished(Resolution::ExpansionDiscarded(actual))) => {
                                    *cards = actual
                                }
                                _ => cards.clear(),
                            }
                        }
                        *stage = DiscardStage::After;
                        let queued = tag.is_some()
                            && self.queue_timing(
                                "loseToDiscardpileEnd",
                                Some(owner),
                                None,
                                None,
                                0,
                                cards,
                            );
                        self.scheduler.replace_current(Program::Expansion(flow));
                        if queued {
                            return;
                        }
                    }
                    DiscardStage::After => {
                        *stage = DiscardStage::Finish;
                        let queued = tag.is_some()
                            && self.queue_timing(
                                "loseToDiscardpileAfter",
                                Some(owner),
                                None,
                                None,
                                0,
                                cards,
                            );
                        self.scheduler.replace_current(Program::Expansion(flow));
                        if queued {
                            return;
                        }
                    }
                    DiscardStage::Finish => {
                        self.finish();
                        return;
                    }
                }
            }
            Flow::Lose {
                owner,
                cards,
                tag,
                moved,
                stage,
            } => {
                let owner = *owner;
                if matches!(child, Some(Outcome::Cancelled)) {
                    if moved.is_empty() {
                        self.scheduler.end(Outcome::Cancelled);
                    } else {
                        self.scheduler
                            .end(Outcome::Finished(Resolution::ExpansionDiscarded(
                                moved.clone(),
                            )));
                    }
                    return;
                }
                let timing = match *stage {
                    LoseStage::Before => {
                        *stage = LoseStage::Begin;
                        Some("loseBefore")
                    }
                    LoseStage::Begin => {
                        *stage = LoseStage::Move;
                        Some("loseBegin")
                    }
                    LoseStage::Move => {
                        // Revalidate the complete selection, then move it without callbacks in between.
                        let eligible: Vec<_> = cards
                            .iter()
                            .copied()
                            .filter(|c| {
                                self.seat(owner).is_ok_and(|p| p.expansions.contains(c))
                                    && tag.as_ref().is_none_or(|tag| {
                                        self.expansion_tags(c.id).iter().any(|t| t == tag)
                                    })
                            })
                            .collect();
                        for card in eligible {
                            let card = self.take_card(owner, Zone::Expansion, card.id);
                            self.discard.push(card);
                            moved.push(card);
                        }
                        *stage = LoseStage::End;
                        None
                    }
                    LoseStage::End => {
                        *stage = LoseStage::After;
                        Some("loseEnd")
                    }
                    LoseStage::After => {
                        *stage = LoseStage::Finish;
                        Some("loseAfter")
                    }
                    LoseStage::Finish => {
                        self.scheduler
                            .end(Outcome::Finished(Resolution::ExpansionDiscarded(
                                moved.clone(),
                            )));
                        return;
                    }
                };
                let queued = timing.is_some_and(|name| {
                    let values = if matches!(*stage, LoseStage::After | LoseStage::Finish) {
                        &*moved
                    } else {
                        &*cards
                    };
                    self.queue_timing(name, Some(owner), None, None, values.len() as u16, values)
                });
                self.scheduler.replace_current(Program::Expansion(flow));
                if queued {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "expansion_tests.rs"]
mod tests;
