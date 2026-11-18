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
        let mut hand = 0;
        let mut losses = Vec::with_capacity(cards.len());
        for &card in cards {
            let zone = self.zone_for_held_card(seat, card.id);
            self.next(EventKind::UseCost { seat, card });
            hand += usize::from(matches!(zone, Zone::Hand));
            losses.push((zone, card));
        }
        let moment = HeroMoment::LostCards {
            seat,
            cards: losses,
        };
        let empties_hand = hand > 0
            && self
                .seat(seat)
                .is_ok_and(|player| player.hand.len() == hand);
        if !cards.is_empty()
            && (self.has_hero_triggers(&moment)
                || (empties_hand && self.has_skill(seat, "lianying"))
                || self.has_timing_registrations("loseAfter"))
        {
            // Eligibility is evaluated after the queued materials move.
            let dispatch = self.hero_triggers(moment);
            self.scheduler.next(dispatch);
        }
    }

    pub(crate) fn resolve_virtual_tao(
        &mut self,
        seat: SeatId,
        target: SeatId,
        card: Card,
        child: Option<Outcome<Resolution>>,
    ) {
        if child.is_none() {
            self.queue_use_costs(seat, &[card]);
            self.next(EventKind::Recover {
                seat,
                target,
                amount: 1 + self.hero_tao_bonus(seat, target),
            });
            self.scheduler
                .after(Program::event(EventKind::Discard { card }));
        } else {
            self.finish();
        }
    }
}
#[cfg(test)]
mod dispatch_tests;
