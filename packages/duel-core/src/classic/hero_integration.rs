use super::skills::{HeroAdjustment, HeroMoment};
use crate::scheduler::Outcome;
use crate::{Card, EventKind, Game, Program, Resolution, SeatId, Zone};

#[derive(Clone, Copy)]
enum JudgeStep {
    Draw,
    Revealed(Card),
    Replaced(Card),
    Finish(Card),
}

#[derive(Clone, Copy)]
pub(crate) struct Judgment {
    pub(crate) seat: SeatId,
    keep_black: bool,
    step: JudgeStep,
}

impl Game {
    pub(crate) fn judge_program(&self, seat: SeatId, keep_black: bool) -> Program {
        Program::Judgment(Judgment {
            seat,
            keep_black,
            step: JudgeStep::Draw,
        })
    }

    pub(crate) fn resolve_judgment(
        &mut self,
        mut judgment: Judgment,
        child: Option<Outcome<Resolution>>,
    ) {
        use Outcome::{Cancelled, Finished};
        use Resolution::{Hero, Judged};
        let seat = judgment.seat;
        match judgment.step {
            JudgeStep::Draw => {
                let Some(cards) = self.reveal_cards(1) else {
                    self.scheduler.end(Cancelled);
                    return;
                };
                let card = cards[0];
                judgment.step = JudgeStep::Revealed(card);
                self.scheduler.replace_current(Program::Judgment(judgment));
                self.next(EventKind::Reveal { seat, card });
            }
            JudgeStep::Revealed(card) => {
                let moment = HeroMoment::JudgeBefore { seat, card };
                if self.has_hero_triggers(&moment) {
                    judgment.step = JudgeStep::Replaced(card);
                    self.scheduler.replace_current(Program::Judgment(judgment));
                    let dispatch = self.hero_triggers(moment);
                    self.scheduler.next(dispatch);
                } else {
                    self.apply_judgment(judgment, card);
                }
            }
            JudgeStep::Replaced(original) => {
                let card = match child {
                    Some(Finished(Hero(HeroAdjustment::Judgment(card)))) => card,
                    _ => original,
                };
                self.apply_judgment(judgment, card);
            }
            JudgeStep::Finish(card) => {
                if self.resolving.contains(&card) {
                    self.scheduler
                        .after(Program::event(EventKind::Discard { card }));
                }
                self.scheduler.end(Finished(Judged(card)));
            }
        }
    }

    fn apply_judgment(&mut self, mut judgment: Judgment, card: Card) {
        let moment = HeroMoment::JudgeAfter {
            seat: judgment.seat,
            card,
            keep_black: judgment.keep_black,
        };
        judgment.step = JudgeStep::Finish(card);
        self.scheduler.replace_current(Program::Judgment(judgment));
        self.queue_hero_moment(moment);
    }

    pub(crate) fn queue_hero_moment(&mut self, moment: HeroMoment) -> bool {
        if self.match_result().is_some() || !self.has_hero_triggers(&moment) {
            return false;
        }
        let dispatch = self.hero_triggers(moment);
        self.scheduler.next(dispatch);
        true
    }

    pub(crate) fn queue_discard_batch(&mut self, seat: SeatId, cards: Vec<(Zone, Card)>) {
        if !cards.is_empty() {
            self.scheduler.next(Program::DiscardGroup { seat, cards });
        }
    }

    pub(crate) fn resolve_discard_group(&mut self, seat: SeatId, cards: Vec<(Zone, Card)>) {
        for &(zone, card) in &cards {
            let card = self.take_card(seat, zone, card.id);
            self.discard.push(card);
            self.scheduler.next(Program::Discarded(card));
        }
        self.queue_hero_losses(seat, cards);
        self.finish();
    }

    pub(crate) fn queue_hero_losses(&mut self, seat: SeatId, cards: Vec<(Zone, Card)>) -> bool {
        cards
            .iter()
            .any(|(zone, _)| matches!(zone, Zone::Hand | Zone::Equipment))
            && self.queue_hero_moment(HeroMoment::LostCards { seat, cards })
    }

    pub(crate) fn queue_use_costs(&mut self, seat: SeatId, cards: &[Card]) {
        if cards.is_empty() {
            return;
        }
        let cards = cards
            .iter()
            .map(|&card| (self.zone_for_held_card(seat, card.id), card))
            .collect();
        self.scheduler.next(Program::UseCosts {
            seat,
            cards,
            paid: false,
        });
    }

    pub(crate) fn resolve_use_costs(&mut self, seat: SeatId, cards: Vec<(Zone, Card)>, paid: bool) {
        if !paid {
            for &(_, card) in &cards {
                self.next(EventKind::UseCost { seat, card });
            }
            self.scheduler.replace_current(Program::UseCosts {
                seat,
                cards,
                paid: true,
            });
            return;
        }
        let moment = HeroMoment::LostCards { seat, cards };
        if self.wants_timing("loseAfter") || self.has_hero_triggers(&moment) {
            let dispatch = self.hero_triggers(moment);
            self.scheduler.next(dispatch);
        }
        self.finish();
    }
}
#[cfg(test)]
mod dispatch_tests;
