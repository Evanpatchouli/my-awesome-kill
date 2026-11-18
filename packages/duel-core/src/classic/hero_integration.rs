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

#[derive(Clone, Copy)]
enum DamageStage {
    Before(usize),
    Hp,
    CheckDying,
    Dying,
    Source,
    End,
    After,
    Finish,
    Cancelled,
    CancelledFinish,
}

#[derive(Clone)]
pub(crate) struct CardDamage {
    source: Option<SeatId>,
    target: SeatId,
    amount: u16,
    card_kind: Option<CardKind>,
    cards: Vec<Card>,
    allows_damage_skills: bool,
    stage: DamageStage,
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
        Program::CardDamage(self.damage_context(source, target, amount, kind, cards))
    }

    pub(crate) fn queue_lose_hp(&mut self, seat: SeatId, amount: u16) {
        self.scheduler.next(Program::CardDamage(CardDamage {
            source: None,
            target: seat,
            amount,
            card_kind: None,
            cards: vec![],
            allows_damage_skills: false,
            stage: DamageStage::Before(0),
        }));
    }

    pub(crate) fn damage_context(
        &self,
        source: Option<SeatId>,
        target: SeatId,
        amount: u16,
        card_kind: Option<CardKind>,
        cards: Vec<Card>,
    ) -> CardDamage {
        CardDamage {
            source,
            target,
            amount,
            card_kind,
            cards,
            allows_damage_skills: true,
            stage: DamageStage::Before(0),
        }
    }

    pub(crate) fn resolve_card_damage(
        &mut self,
        mut damage: CardDamage,
        child: Option<Outcome<Resolution>>,
    ) {
        use DamageStage::*;
        use Outcome::Finished;
        use Resolution::HpChanged;

        if matches!(child, Some(Outcome::Cancelled))
            && matches!(damage.stage, Before(_) | CheckDying | End | After | Finish)
        {
            damage.stage = Cancelled;
        }
        if self.match_result().is_some() && !matches!(damage.stage, Cancelled | CancelledFinish) {
            if matches!(damage.stage, Before(0)) {
                self.scheduler.end(Outcome::Cancelled);
            } else {
                self.finish();
            }
            return;
        }
        let (source, target, amount) = (damage.source, damage.target, damage.amount);
        let damage_timings = [
            "damageBefore",
            "damageBegin",
            "damageBegin1",
            "damageBegin2",
            "damageBegin3",
            "damageBegin4",
        ];
        let queue_hook = |game: &mut Self, timing, cards: &[Card]| {
            game.queue_timing(timing, Some(target), source, None, amount, cards)
        };
        loop {
            let queued = match damage.stage {
                Before(index) if damage.allows_damage_skills && index < damage_timings.len() => {
                    let timing = damage_timings[index];
                    damage.stage = Before(index + 1);
                    match (timing, damage.card_kind, source) {
                        ("damageBegin2", Some(CardKind::Sha), Some(source)) => {
                            self.queue_sha_damage_begin2(source, target, amount, &damage.cards)
                        }
                        _ => queue_hook(self, timing, &damage.cards),
                    }
                }
                Before(_) => {
                    damage.stage = Hp;
                    self.queue_hp_change(target, -i32::from(amount));
                    true
                }
                Cancelled => {
                    damage.stage = CancelledFinish;
                    queue_hook(self, "damageCancelled", &damage.cards)
                }
                CancelledFinish => {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                Hp if !matches!(child, Some(Finished(HpChanged))) => {
                    self.finish();
                    return;
                }
                Hp => {
                    damage.stage = CheckDying;
                    damage.allows_damage_skills && queue_hook(self, "damage", &damage.cards)
                }
                CheckDying
                    if self.classic.is_some()
                        && self
                            .seat(target)
                            .is_ok_and(|seat| seat.hp <= 0 && seat.alive) =>
                {
                    damage.stage = Dying;
                    self.next(EventKind::Dying { source, target });
                    true
                }
                CheckDying | Dying => {
                    if !damage.allows_damage_skills {
                        self.finish();
                        return;
                    }
                    damage.stage = Source;
                    false
                }
                Source => {
                    damage.stage = End;
                    queue_hook(self, "damageSource", &damage.cards)
                }
                End => {
                    damage.stage = After;
                    let moment = |cards| HeroMoment::DamageAfter {
                        source,
                        target,
                        amount,
                        cards,
                    };
                    if self.has_hero_triggers(&moment(Vec::new())) {
                        self.scheduler
                            .next(self.hero_triggers(moment(damage.cards.clone())));
                        true
                    } else {
                        false
                    }
                }
                After => {
                    damage.stage = Finish;
                    queue_hook(self, "damageAfter", &damage.cards)
                }
                Finish => {
                    self.finish();
                    return;
                }
            };
            if queued {
                self.scheduler.replace_current(Program::CardDamage(damage));
                return;
            }
        }
    }

    pub(crate) fn queue_hero_moment(&mut self, moment: HeroMoment) -> bool {
        if self.match_result().is_some() || !self.has_hero_triggers(&moment) {
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
            self.scheduler.next(self.hero_triggers(moment));
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
