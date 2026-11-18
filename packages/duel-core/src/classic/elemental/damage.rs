use super::{DamageCard, Rule};
use crate::classic::skills::HeroMoment;
use crate::scheduler::{EventRef, Outcome};
use crate::{Card, CardKind, EventKind, Game, Program, Resolution, SeatId, TriggerEvent};

#[derive(Clone, Copy)]
enum Stage {
    Before(usize),
    Hp,
    CheckDying,
    Dying,
    Source,
    End,
    After,
    Finish,
    Cancel,
    FinishCancelled,
}

#[derive(Clone)]
pub(crate) struct CardDamage {
    source: Option<SeatId>,
    target: SeatId,
    amount: u16,
    pub(crate) card: DamageCard,
    lose_hp: bool,
    propagated: bool,
    record: Option<usize>,
    on_damage: Option<TriggerEvent>,
    stage: Stage,
}

impl CardDamage {
    pub(crate) fn kind(&self) -> EventKind {
        if self.lose_hp {
            EventKind::LoseHp {
                seat: self.target,
                amount: self.amount,
            }
        } else {
            EventKind::Damage {
                source: self.source,
                target: self.target,
                amount: self.amount,
            }
        }
    }
}

impl Game {
    pub(crate) fn live_damage_source(&self, source: Option<SeatId>) -> Option<SeatId> {
        source.filter(|id| self.seat(*id).is_ok_and(|seat| seat.alive))
    }
    pub(crate) fn damage_program(
        &self,
        source: Option<SeatId>,
        target: SeatId,
        amount: u16,
        kind: Option<CardKind>,
        cards: Vec<Card>,
    ) -> Program {
        let source = self.live_damage_source(source);
        let amount = amount.saturating_add(self.hero_damage_bonus(source, kind));
        self.elemental_damage_program(
            source,
            target,
            amount,
            self.physical_damage_card(kind, cards),
            false,
        )
    }

    pub(crate) fn elemental_damage_program(
        &self,
        source: Option<SeatId>,
        target: SeatId,
        amount: u16,
        card: DamageCard,
        propagated: bool,
    ) -> Program {
        Program::CardDamage(CardDamage {
            source: self.live_damage_source(source),
            target,
            amount,
            card,
            lose_hp: false,
            propagated,
            record: None,
            on_damage: None,
            stage: Stage::Before(0),
        })
    }

    pub(crate) fn lose_hp_program(&self, target: SeatId, amount: u16) -> Program {
        Program::CardDamage(CardDamage {
            source: None,
            target,
            amount,
            card: DamageCard::default(),
            lose_hp: true,
            propagated: false,
            record: None,
            on_damage: None,
            stage: Stage::Before(0),
        })
    }

    pub(crate) fn queue_lose_hp(&mut self, target: SeatId, amount: u16) {
        self.scheduler.next(self.lose_hp_program(target, amount));
    }

    pub(crate) fn resolve_card_damage(
        &mut self,
        event: EventRef,
        mut damage: CardDamage,
        child: Option<Outcome<Resolution>>,
    ) {
        use Stage::*;
        let live_source = self.live_damage_source(damage.source);
        if damage.source != live_source {
            damage.source = live_source;
            self.scheduler
                .replace_current(Program::CardDamage(damage.clone()));
        }
        if matches!(child, Some(Outcome::Cancelled))
            && matches!(damage.stage, Before(_) | CheckDying | End | After | Finish)
        {
            damage.stage = Cancel;
        }
        if self.match_result().is_some() && !matches!(damage.stage, Cancel | FinishCancelled) {
            if matches!(damage.stage, Before(0)) {
                self.scheduler.end(Outcome::Cancelled);
            } else {
                self.finish();
            }
            return;
        }
        let (source, target, amount) = (damage.source, damage.target, damage.amount);
        let before = [
            "damageBefore",
            "damageBegin",
            "damageBegin1",
            "damageBegin2",
            "damageBegin3",
            "damageBegin4",
        ];
        loop {
            let queued = match damage.stage {
                Before(0) if !damage.lose_hp && amount == 0 => {
                    damage.stage = Finish;
                    self.queue_timing(
                        "damageZero",
                        Some(target),
                        source,
                        None,
                        amount,
                        &damage.card.cards,
                    )
                }
                Before(index) if !damage.lose_hp && index < before.len() => {
                    let timing = before[index];
                    damage.stage = Before(index + 1);
                    match (timing, damage.card.kind, source) {
                        ("damageBegin2", Some(CardKind::Sha), Some(source))
                            if !damage.propagated =>
                        {
                            self.queue_sha_damage_begin2(source, target, amount, &damage.card.cards)
                        }
                        _ => self.queue_timing(
                            timing,
                            Some(target),
                            source,
                            None,
                            amount,
                            &damage.card.cards,
                        ),
                    }
                }
                Before(_) => {
                    if !damage.lose_hp {
                        damage.record = Some(self.commit_damage(
                            event,
                            source,
                            target,
                            amount,
                            &damage.card,
                            damage.propagated,
                        ));
                    }
                    self.queue_health_change(target, -i32::from(amount), damage.record);
                    // A damage request precedes its queued HP change. The callbacks run
                    // after that child has settled, including its armor loss and unlink.
                    if !damage.lose_hp {
                        damage.on_damage = self.prepare_timing(
                            "damage",
                            Some(target),
                            source,
                            None,
                            amount,
                            &damage.card.cards,
                        );
                    }
                    damage.stage = Hp;
                    true
                }
                Hp => {
                    if !matches!(child, Some(Outcome::Finished(Resolution::HpChanged { .. }))) {
                        self.finish();
                        return;
                    }
                    damage.stage = CheckDying;
                    if let Some(on_damage) = damage.on_damage.take() {
                        self.scheduler
                            .next(Program::Dispatch(self.dispatch_for_timing(on_damage)));
                        true
                    } else {
                        false
                    }
                }
                CheckDying
                    if self.classic.is_some()
                        && self.seat(target).is_ok_and(|p| p.hp <= 0 && p.alive) =>
                {
                    self.next(EventKind::Dying { source, target });
                    damage.stage = Dying;
                    true
                }
                CheckDying | Dying => {
                    if damage.lose_hp {
                        self.finish();
                        return;
                    }
                    damage.stage = Source;
                    false
                }
                Source => {
                    damage.stage = End;
                    self.queue_timing(
                        "damageSource",
                        Some(target),
                        source,
                        None,
                        amount,
                        &damage.card.cards,
                    )
                }
                End => {
                    damage.stage = After;
                    self.queue_hero_moment(HeroMoment::DamageAfter {
                        source,
                        target,
                        amount,
                        cards: damage.card.cards.clone(),
                    })
                }
                After => {
                    damage.stage = Finish;
                    let rule = damage
                        .record
                        .filter(|_| damage.card.nature.is_some() && !damage.propagated)
                        .map(Rule::Spread);
                    if rule.is_none() && !self.wants_timing("damageAfter") {
                        continue;
                    }
                    let request = self.subject_timing(
                        "damageAfter",
                        Some(target),
                        source,
                        None,
                        amount,
                        &damage.card.cards,
                    );
                    if let Some(dispatch) = self.timing_program(request, rule) {
                        self.scheduler.next(dispatch);
                        true
                    } else {
                        false
                    }
                }
                Finish => {
                    self.finish();
                    return;
                }
                Cancel => {
                    damage.stage = FinishCancelled;
                    self.queue_timing(
                        "damageCancelled",
                        Some(target),
                        source,
                        None,
                        amount,
                        &damage.card.cards,
                    )
                }
                FinishCancelled => {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
            };
            if queued {
                self.scheduler.replace_current(Program::CardDamage(damage));
                return;
            }
        }
    }
}
