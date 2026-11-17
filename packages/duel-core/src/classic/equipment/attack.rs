use super::{skill::Skill, Flow};
use crate::scheduler::Outcome;
use crate::{Card, CardKind, EventKind, Game, Program, Resolution, SeatId};

#[derive(Clone, Copy)]
enum AttackStep {
    Pay,
    Declare(usize),
    Resolve(usize),
}

#[derive(Clone)]
pub(crate) struct Attack {
    seat: SeatId,
    cards: Vec<Card>,
    targets: Vec<SeatId>,
    step: AttackStep,
    ignore_armor: bool,
}

impl Attack {
    pub(crate) fn event(&self) -> EventKind {
        match *self.cards.as_slice() {
            [card] if self.targets.len() == 1 => EventKind::UseSha {
                seat: self.seat,
                target: self.targets[0],
                card,
            },
            [card] => EventKind::UseShaTargets {
                seat: self.seat,
                card,
            },
            _ => EventKind::UseVirtual {
                seat: self.seat,
                kind: CardKind::Sha,
            },
        }
    }
}

#[derive(Clone, Copy)]
enum TargetStep {
    Defend,
    Miss,
    Damage,
}

#[derive(Clone, Copy)]
pub(crate) struct Target {
    pub(super) seat: SeatId,
    pub(super) target: SeatId,
    step: TargetStep,
    ignore_armor: bool,
}

#[derive(Clone, Copy)]
enum DamageStep {
    Before,
    AfterBow,
    Apply,
}

#[derive(Clone, Copy)]
pub(crate) struct ShaDamage {
    pub(super) seat: SeatId,
    pub(super) target: SeatId,
    step: DamageStep,
}

impl Game {
    pub(crate) fn attack_program(
        &self,
        seat: SeatId,
        cards: Vec<Card>,
        targets: Vec<SeatId>,
    ) -> Program {
        Program::Equipment(Flow::Attack(Attack {
            seat,
            cards,
            targets,
            step: AttackStep::Pay,
            ignore_armor: self.has_weapon(seat, CardKind::Qinggang),
        }))
    }

    pub(super) fn resolve_attack(&mut self, mut attack: Attack) {
        if self.match_result().is_some() {
            self.finish();
            return;
        }
        let seat = attack.seat;
        match attack.step {
            AttackStep::Pay => {
                for card in &attack.cards {
                    if attack.cards.len() == 1 {
                        self.move_to_resolving(seat, card.id);
                    } else {
                        self.next(EventKind::UseCost { seat, card: *card });
                    }
                    self.scheduler
                        .after(Program::event(EventKind::Discard { card: *card }));
                }
                attack.step = AttackStep::Declare(0);
                self.continue_equipment(Flow::Attack(attack));
            }
            AttackStep::Declare(index) => {
                attack.step = AttackStep::Resolve(0);
                if self.has_weapon(seat, CardKind::Cixiong) {
                    let source_gender = self
                        .seat(seat)
                        .ok()
                        .and_then(|source| source.hero)
                        .map(|hero| hero.gender.as_str());
                    if let Some((offset, target)) =
                        attack.targets.iter().copied().skip(index).enumerate().find(
                            |(_, target)| {
                                self.seat(*target)
                                    .ok()
                                    .and_then(|target| target.hero)
                                    .is_some_and(|hero| {
                                        source_gender.is_some_and(|source| source != hero.gender)
                                    })
                            },
                        )
                    {
                        attack.step = AttackStep::Declare(index + offset + 1);
                        self.scheduler
                            .next(Program::Equipment(Flow::Skill(Skill::new(
                                seat,
                                target,
                                CardKind::Cixiong,
                            ))));
                    }
                }
                self.continue_equipment(Flow::Attack(attack));
            }
            AttackStep::Resolve(index) => {
                let Some((offset, target)) = attack
                    .targets
                    .iter()
                    .copied()
                    .skip(index)
                    .enumerate()
                    .find(|(_, target)| self.seat(*target).is_ok_and(|target| target.alive))
                else {
                    self.finish();
                    return;
                };
                let target = Target {
                    seat,
                    target,
                    step: TargetStep::Defend,
                    ignore_armor: attack.ignore_armor,
                };
                attack.step = AttackStep::Resolve(index + offset + 1);
                self.continue_equipment(Flow::Attack(attack));
                self.scheduler
                    .next(Program::Equipment(Flow::Target(target)));
            }
        }
    }

    pub(super) fn resolve_sha_target(
        &mut self,
        mut attack: Target,
        child: Option<Outcome<Resolution>>,
    ) {
        match (attack.step, child) {
            (TargetStep::Defend, None) => self.scheduler.next(Program::Equipment(Flow::response(
                attack.target,
                CardKind::Shan,
                attack.ignore_armor,
            ))),
            (TargetStep::Defend, Some(Outcome::Finished(Resolution::Passed)))
            | (TargetStep::Miss, Some(Outcome::Finished(Resolution::ForcedHit))) => {
                attack.step = TargetStep::Damage;
                self.continue_equipment(Flow::Target(attack));
                self.scheduler
                    .next(Program::Equipment(Flow::ShaDamage(ShaDamage {
                        seat: attack.seat,
                        target: attack.target,
                        step: DamageStep::Before,
                    })));
            }
            (TargetStep::Defend, Some(Outcome::Finished(Resolution::Responded))) => {
                if let Some(kind) = self.miss_skill(attack.seat, attack.target) {
                    attack.step = TargetStep::Miss;
                    self.continue_equipment(Flow::Target(attack));
                    self.scheduler
                        .next(Program::Equipment(Flow::Skill(Skill::new(
                            attack.seat,
                            attack.target,
                            kind,
                        ))));
                } else {
                    self.scheduler.end(Outcome::Cancelled);
                }
            }
            (TargetStep::Defend | TargetStep::Miss, Some(_)) => {
                self.scheduler.end(Outcome::Cancelled)
            }
            (TargetStep::Damage, Some(_)) => self.finish(),
            _ => unreachable!("the attack target is waiting for a defense, miss skill, or damage"),
        }
    }

    pub(super) fn resolve_sha_damage(
        &mut self,
        mut damage: ShaDamage,
        child: Option<Outcome<Resolution>>,
    ) {
        let seat = damage.seat;
        let target = damage.target;
        match damage.step {
            DamageStep::Before
                if self.has_weapon(seat, CardKind::Qilin)
                    && !self.target_horses(target).is_empty() =>
            {
                damage.step = DamageStep::AfterBow;
                self.continue_equipment(Flow::ShaDamage(damage));
                self.scheduler
                    .next(Program::Equipment(Flow::Skill(Skill::new(
                        seat,
                        target,
                        CardKind::Qilin,
                    ))));
            }
            DamageStep::Before | DamageStep::AfterBow => {
                damage.step = DamageStep::Apply;
                self.continue_equipment(Flow::ShaDamage(damage));
                self.resolve_damage(Some(seat), target, 1, None);
            }
            DamageStep::Apply => self.resolve_damage(Some(seat), target, 1, child),
        }
    }
}
