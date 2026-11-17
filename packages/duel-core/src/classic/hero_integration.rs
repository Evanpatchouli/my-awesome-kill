use super::skills::{HeroAdjustment, HeroMoment};
use crate::scheduler::Outcome;
use crate::{Card, CardKind, EventKind, Game, Program, Resolution, SeatId, Zone};

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

#[derive(Clone)]
pub(crate) struct CardDamage {
    source: Option<SeatId>,
    target: SeatId,
    amount: u16,
    cards: Vec<Card>,
    allows_damage_skills: bool,
}

impl CardDamage {
    pub(crate) fn kind(&self) -> EventKind {
        if self.allows_damage_skills {
            EventKind::Damage {
                source: self.source,
                target: self.target,
                amount: self.amount,
            }
        } else {
            EventKind::LoseHp {
                seat: self.target,
                amount: self.amount,
            }
        }
    }
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
                    self.scheduler.next(self.hero_triggers(moment));
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

    pub(crate) fn damage_program(
        &self,
        source: Option<SeatId>,
        target: SeatId,
        amount: u16,
        kind: Option<CardKind>,
        cards: Vec<Card>,
    ) -> Program {
        let amount = amount.saturating_add(self.hero_damage_bonus(source, kind));
        Program::CardDamage(CardDamage {
            source,
            target,
            amount,
            cards,
            allows_damage_skills: true,
        })
    }

    pub(crate) fn queue_lose_hp(&mut self, seat: SeatId, amount: u16) {
        self.scheduler.next(Program::CardDamage(CardDamage {
            source: None,
            target: seat,
            amount,
            cards: vec![],
            allows_damage_skills: false,
        }));
    }

    pub(crate) fn damage_context(
        &self,
        source: Option<SeatId>,
        target: SeatId,
        amount: u16,
        cards: Vec<Card>,
    ) -> CardDamage {
        CardDamage {
            source,
            target,
            amount,
            cards,
            allows_damage_skills: true,
        }
    }

    pub(crate) fn resolve_card_damage(
        &mut self,
        damage: CardDamage,
        child: Option<Outcome<Resolution>>,
    ) {
        use Outcome::Finished;
        use Resolution::{Complete, HpChanged};
        let CardDamage {
            source,
            target,
            amount,
            cards,
            allows_damage_skills,
        } = damage;
        match child {
            None if self.match_result().is_some() => self.scheduler.end(Outcome::Cancelled),
            None => self.queue_hp_change(target, -i32::from(amount)),
            Some(Finished(HpChanged))
                if self.classic.is_some()
                    && self
                        .seat(target)
                        .is_ok_and(|seat| seat.hp <= 0 && seat.alive) =>
            {
                self.next(EventKind::Dying { source, target });
            }
            Some(Finished(HpChanged | Complete))
                if allows_damage_skills
                    && self.classic.is_some()
                    && self.match_result().is_none()
                    && self.seat(target).is_ok_and(|seat| seat.alive) =>
            {
                if !self.queue_hero_moment(HeroMoment::DamageAfter {
                    source,
                    target,
                    amount,
                    cards,
                }) {
                    self.finish();
                }
            }
            Some(_) => self.finish(),
        }
    }

    pub(crate) fn queue_hero_moment(&mut self, moment: HeroMoment) -> bool {
        if self.classic.is_none()
            || self.match_result().is_some()
            || !self.has_hero_triggers(&moment)
        {
            return false;
        }
        self.scheduler.next(self.hero_triggers(moment));
        true
    }

    pub(crate) fn queue_discard_batch(&mut self, seat: SeatId, cards: Vec<(Zone, Card)>) {
        if !cards.is_empty() {
            self.scheduler.next(Program::DiscardGroup { seat, cards });
        }
    }

    pub(crate) fn resolve_discard_group(&mut self, seat: SeatId, cards: Vec<(Zone, Card)>) {
        let mut hand = 0;
        let mut equipment = 0;
        for (zone, card) in cards {
            let card = self.take_card(seat, zone, card.id);
            self.discard.push(card);
            self.scheduler.next(Program::Discarded(card));
            hand += usize::from(matches!(zone, Zone::Hand));
            equipment += usize::from(matches!(zone, Zone::Equipment));
        }
        self.queue_hero_losses(seat, hand, equipment);
        self.finish();
    }

    pub(crate) fn queue_hero_losses(
        &mut self,
        seat: SeatId,
        hand_count: usize,
        equipment_count: usize,
    ) {
        if hand_count > 0 || equipment_count > 0 {
            self.queue_hero_moment(HeroMoment::LostCards {
                seat,
                hand_count,
                equipment_count,
            });
        }
    }

    pub(crate) fn queue_use_costs(&mut self, seat: SeatId, cards: &[Card]) {
        let mut hand = 0;
        let mut equipment = 0;
        for &card in cards {
            let zone = self.zone_for_held_card(seat, card.id);
            self.next(EventKind::UseCost { seat, card });
            hand += usize::from(matches!(zone, Zone::Hand));
            equipment += usize::from(matches!(zone, Zone::Equipment));
        }
        let empties_hand = hand > 0
            && self
                .seat(seat)
                .is_ok_and(|player| player.hand.len() == hand);
        if empties_hand && self.has_skill(seat, "lianying")
            || equipment > 0 && self.has_skill(seat, "xiaoji")
        {
            // The trigger queue checks the zones once all materials have moved.
            self.scheduler
                .next(self.hero_triggers(HeroMoment::LostCards {
                    seat,
                    hand_count: hand,
                    equipment_count: equipment,
                }));
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
