//! Ordered partition choices. This flow validates layouts without moving physical cards.
use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::scheduler::{EventRef, Outcome};
use crate::{
    Action, Answer, Card, CardId, ChoiceKind, ChoiceView, Decision, EventId, EventKind, Game,
    Program, Resolution, RuleError, SeatId,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveStep {
    pub card: CardId,
    pub row: usize,
    /// Insertion index after removing the card from its previous row.
    pub index: usize,
}

#[derive(Clone, Debug)]
pub struct MoveRow {
    pub label: String,
    pub cards: Vec<Card>,
    pub min: usize,
    pub max: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRowView {
    pub label: String,
    pub cards: Vec<CardId>,
    pub min: usize,
    pub max: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveChoiceView {
    pub prompt: String,
    pub rows: Vec<MoveRowView>,
    pub allow_decline: bool,
}

pub type MoveFilter = Arc<dyn Fn(&Game, &[Vec<Card>], MoveStep) -> bool + Send + Sync>;
pub type MoveConfirmFilter = Arc<dyn Fn(&Game, &[Vec<Card>]) -> bool + Send + Sync>;
pub type MoveBot = Arc<dyn Fn(&Game, &[Vec<Card>]) -> Option<Vec<MoveStep>> + Send + Sync>;

#[derive(Clone)]
pub struct MoveChoiceRequest {
    pub seat: SeatId,
    pub prompt: String,
    pub rows: Vec<MoveRow>,
    pub allow_decline: bool,
    pub filter_move: Option<MoveFilter>,
    pub filter_ok: Option<MoveConfirmFilter>,
    pub bot: Option<MoveBot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveResult {
    pub event: EventId,
    pub rows: Option<Vec<Vec<Card>>>,
    pub cancelled: bool,
}

#[derive(Clone, Copy)]
enum Stage {
    Before,
    Begin,
    Choose,
    End,
    After,
    Finish,
    CancelNotify,
    CancelFinish,
}

#[derive(Clone)]
pub(crate) struct Flow {
    request: MoveChoiceRequest,
    stage: Stage,
    result: Option<MoveResult>,
}

impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        EventKind::ChooseToMove {
            seat: self.request.seat,
        }
    }
    fn rows(&self) -> Vec<Vec<Card>> {
        self.request
            .rows
            .iter()
            .map(|row| row.cards.clone())
            .collect()
    }
    fn view(&self) -> MoveChoiceView {
        MoveChoiceView {
            prompt: self.request.prompt.clone(),
            rows: self
                .request
                .rows
                .iter()
                .map(|row| MoveRowView {
                    label: row.label.clone(),
                    cards: row.cards.iter().map(|card| card.id).collect(),
                    min: row.min,
                    max: row.max,
                })
                .collect(),
            allow_decline: self.request.allow_decline,
        }
    }
}

impl Game {
    pub(crate) fn plan_move_choice(
        &self,
        request: MoveChoiceRequest,
    ) -> Result<Program, RuleError> {
        if !self.seat(request.seat)?.alive {
            return Err(RuleError::InactiveSeat { seat: request.seat });
        }
        if request.rows.is_empty() || request.rows.iter().any(|row| row.min > row.max) {
            return Err(RuleError::InvalidSelection);
        }
        let mut seen = HashSet::new();
        let physical: Vec<_> = self.physical_cards().copied().collect();
        for card in request.rows.iter().flat_map(|row| &row.cards) {
            if !seen.insert(card.id) || !physical.contains(card) {
                return Err(RuleError::InvalidCard);
            }
        }
        Ok(Program::MoveChoice(Flow {
            request,
            stage: Stage::Before,
            result: None,
        }))
    }

    pub(crate) fn validate_move_choice(
        &self,
        seat: SeatId,
        event: EventId,
        moves: Option<Vec<MoveStep>>,
    ) -> Result<Answer, RuleError> {
        if self.checked_choice(seat, event)?.move_cards.is_none() {
            return Err(RuleError::InvalidSelection);
        }
        let (_, Program::MoveChoice(flow)) = self
            .scheduler
            .active_frame(event)
            .ok_or(RuleError::WrongEvent)?
        else {
            return Err(RuleError::WrongEvent);
        };
        let Some(moves) = moves else {
            return if flow.request.allow_decline {
                Ok(Answer::MoveCards(None))
            } else {
                Err(RuleError::InvalidSelection)
            };
        };
        let mut rows = flow.rows();
        for step in moves {
            let Some((old_row, old_index)) = rows.iter().enumerate().find_map(|(row, cards)| {
                cards
                    .iter()
                    .position(|card| card.id == step.card)
                    .map(|index| (row, index))
            }) else {
                return Err(RuleError::InvalidCard);
            };
            if step.row >= rows.len()
                || step.index > rows[step.row].len() - usize::from(old_row == step.row)
            {
                return Err(RuleError::InvalidSelection);
            }
            if old_row != step.row && rows[step.row].len() >= flow.request.rows[step.row].max {
                return Err(RuleError::InvalidSelection);
            }
            if flow
                .request
                .filter_move
                .as_ref()
                .is_some_and(|filter| !filter(self, &rows, step))
            {
                return Err(RuleError::InvalidSelection);
            }
            let card = rows[old_row].remove(old_index);
            rows[step.row].insert(step.index, card);
        }
        if rows
            .iter()
            .zip(&flow.request.rows)
            .any(|(cards, row)| !(row.min..=row.max).contains(&cards.len()))
            || flow
                .request
                .filter_ok
                .as_ref()
                .is_some_and(|filter| !filter(self, &rows))
        {
            return Err(RuleError::InvalidSelection);
        }
        Ok(Answer::MoveCards(Some(rows)))
    }

    pub(crate) fn bot_move_choice(&self, event: EventId) -> Option<Action> {
        let (_, Program::MoveChoice(flow)) = self.scheduler.active_frame(event)? else {
            return None;
        };
        let seat = flow.request.seat;
        if let Some(moves) = flow
            .request
            .bot
            .as_ref()
            .and_then(|bot| bot(self, &flow.rows()))
        {
            if self
                .validate_move_choice(seat, event, Some(moves.clone()))
                .is_ok()
            {
                return Some(Action::MoveCards {
                    event,
                    moves: Some(moves),
                });
            }
        }
        if self.validate_move_choice(seat, event, Some(vec![])).is_ok() {
            return Some(Action::MoveCards {
                event,
                moves: Some(vec![]),
            });
        }
        if self.validate_move_choice(seat, event, None).is_ok() {
            return Some(Action::MoveCards { event, moves: None });
        }
        None
    }

    pub fn move_choice_result(&self, event: EventId) -> Option<&MoveResult> {
        match self.scheduler.active_frame(event)?.1 {
            Program::MoveChoice(flow) => flow.result.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn move_choice_cards(&self, event: EventId) -> Vec<Card> {
        match self
            .scheduler
            .active_frame(event)
            .map(|(_, program)| program)
        {
            Some(Program::MoveChoice(flow)) => flow
                .request
                .rows
                .iter()
                .flat_map(|row| row.cards.iter().copied())
                .collect(),
            _ => vec![],
        }
    }

    pub(crate) fn resolve_move_choice(
        &mut self,
        event: EventRef,
        mut flow: Flow,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        use Stage::*;
        if flow.result.is_none() {
            flow.result = Some(MoveResult {
                event: event.id,
                rows: None,
                cancelled: false,
            });
        }
        if (matches!(child, Some(Outcome::Cancelled))
            || (matches!(flow.stage, Choose) && !self.is_alive(flow.request.seat)))
            && !matches!(flow.stage, CancelNotify | CancelFinish)
        {
            flow.result.as_mut().unwrap().cancelled = true;
            flow.stage = CancelNotify;
        }
        let seat = flow.request.seat;
        match flow.stage {
            Before | Begin | End | After => {
                let (timing, next) = match flow.stage {
                    Before => ("chooseToMoveBefore", Begin),
                    Begin => ("chooseToMoveBegin", Choose),
                    End => ("chooseToMoveEnd", After),
                    After => ("chooseToMoveAfter", Finish),
                    _ => unreachable!(),
                };
                flow.stage = next;
                self.scheduler.replace_current(Program::MoveChoice(flow));
                self.queue_timing(timing, Some(seat), None, None, 0, &[]);
            }
            Choose => match answer.take() {
                Some(Answer::MoveCards(rows)) => {
                    flow.result.as_mut().unwrap().rows = rows;
                    flow.stage = End;
                    self.scheduler.replace_current(Program::MoveChoice(flow));
                }
                None => {
                    let view = flow.view();
                    let cards = view
                        .rows
                        .iter()
                        .flat_map(|row| row.cards.iter().copied())
                        .collect::<Vec<_>>();
                    self.decision = Some(Decision::Cards(ChoiceView {
                        event: event.id,
                        seat,
                        kind: ChoiceKind::Content,
                        target: None,
                        min: 0,
                        max: cards.len(),
                        cards,
                        branches: vec![],
                        counter: None,
                        compare: None,
                        compare_group: None,
                        move_cards: Some(view),
                    }));
                }
                _ => unreachable!("a move choice receives a move answer"),
            },
            CancelNotify => {
                flow.stage = CancelFinish;
                self.scheduler.replace_current(Program::MoveChoice(flow));
                self.queue_timing("chooseToMoveCancelled", Some(seat), None, None, 0, &[]);
            }
            Finish | CancelFinish => self.scheduler.end(Outcome::Finished(Resolution::MovedCards(
                flow.result.unwrap(),
            ))),
        }
    }
}
