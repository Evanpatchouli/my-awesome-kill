use super::{skill::Skill, Flow};
use crate::classic::skills::{HeroAdjustment, HeroMoment};
use crate::scheduler::Outcome;
use crate::{Card, CardKind, EventKind, Game, Program, Resolution, SeatId};

#[derive(Clone, Copy)]
enum AttackStep {
    Pay,
    Redirect(usize),
    Redirected(usize),
    Declare(usize),
    Aim(usize),
    Aimed(usize),
    Resolve(usize),
}

#[derive(Clone)]
pub(crate) struct Attack {
    seat: SeatId,
    payer: SeatId,
    cards: Vec<Card>,
    targets: Vec<SeatId>,
    announced_target: Option<SeatId>,
    direct_hits: Vec<SeatId>,
    step: AttackStep,
    ignore_armor: bool,
}

impl Attack {
    pub(crate) fn event(&self) -> EventKind {
        match *self.cards.as_slice() {
            [card] if card.kind == CardKind::Sha => match self.announced_target {
                Some(target) => EventKind::UseSha {
                    seat: self.seat,
                    target,
                    card,
                },
                None => EventKind::UseShaTargets {
                    seat: self.seat,
                    card,
                },
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

#[derive(Clone)]
pub(crate) struct Target {
    pub(super) seat: SeatId,
    pub(super) target: SeatId,
    step: TargetStep,
    ignore_armor: bool,
    direct_hit: bool,
    shan_required: u8,
    cards: Vec<Card>,
}

#[derive(Clone, Copy)]
enum DamageStep {
    Before,
    AfterBow,
    Apply,
}

#[derive(Clone)]
pub(crate) struct ShaDamage {
    pub(super) seat: SeatId,
    pub(super) target: SeatId,
    pub(super) amount: u16,
    step: DamageStep,
    cards: Vec<Card>,
}

impl Game {
    pub(crate) fn attack_program(
        &self,
        seat: SeatId,
        cards: Vec<Card>,
        targets: Vec<SeatId>,
    ) -> Program {
        self.delegated_sha_program(seat, seat, cards, targets)
    }

    pub(crate) fn delegated_sha_program(
        &self,
        seat: SeatId,
        payer: SeatId,
        cards: Vec<Card>,
        targets: Vec<SeatId>,
    ) -> Program {
        let announced_target = (targets.len() == 1).then(|| targets[0]);
        let ignore_armor = self.weapon(seat).is_some_and(|weapon| {
            weapon.kind == CardKind::Qinggang && (payer != seat || !cards.contains(&weapon))
        });
        Program::Equipment(Flow::Attack(Attack {
            seat,
            payer,
            cards,
            targets,
            announced_target,
            direct_hits: vec![],
            step: AttackStep::Pay,
            ignore_armor,
        }))
    }

    pub(super) fn resolve_attack(
        &mut self,
        mut attack: Attack,
        child: Option<Outcome<Resolution>>,
    ) {
        if self.match_result().is_some() {
            self.finish();
            return;
        }
        let seat = attack.seat;
        match attack.step {
            AttackStep::Pay => {
                self.note_hero_sha_play(seat);
                if attack.payer != seat
                    && self
                        .classic
                        .as_ref()
                        .and_then(|state| state.turn)
                        .is_some_and(|turn| turn.seat == seat && turn.phase == crate::Phase::Play)
                {
                    self.classic
                        .as_mut()
                        .expect("the table is classic")
                        .sha_played = true;
                }
                self.queue_use_costs(attack.payer, &attack.cards);
                for &card in &attack.cards {
                    self.scheduler
                        .after(Program::event(EventKind::Discard { card }));
                }
                attack.step = AttackStep::Redirect(0);
            }
            AttackStep::Redirect(index) => {
                attack.step = if let Some(&target) = attack.targets.get(index) {
                    let moment = HeroMoment::ShaRedirect {
                        seat,
                        target,
                        excluded: attack.targets.clone(),
                    };
                    if self.queue_hero_moment(moment) {
                        AttackStep::Redirected(index)
                    } else {
                        AttackStep::Redirect(index + 1)
                    }
                } else {
                    AttackStep::Declare(0)
                };
            }
            AttackStep::Redirected(index) => {
                if let Some(Outcome::Finished(Resolution::Hero(HeroAdjustment::Redirect(target)))) =
                    child
                {
                    attack.targets[index] = target;
                }
                attack.step = AttackStep::Redirect(index + 1);
            }
            AttackStep::Declare(index) => {
                if let Some(&target) = attack.targets.get(index) {
                    attack.step = AttackStep::Aim(index);
                    let gender = |id| {
                        self.seat(id)
                            .ok()
                            .and_then(|actor| actor.hero)
                            .map(|hero| hero.gender.as_str())
                    };
                    if self.has_weapon(seat, CardKind::Cixiong)
                        && matches!((gender(seat), gender(target)), (Some(a), Some(b)) if a != b)
                    {
                        self.scheduler
                            .next(Program::Equipment(Flow::Skill(Skill::new(
                                seat,
                                target,
                                CardKind::Cixiong,
                            ))));
                    }
                } else {
                    attack.step = AttackStep::Resolve(0);
                }
            }
            AttackStep::Aim(index) => {
                let target = attack.targets[index];
                let moment = HeroMoment::ShaTarget { seat, target };
                attack.step = if self.queue_hero_moment(moment) {
                    AttackStep::Aimed(index)
                } else {
                    AttackStep::Declare(index + 1)
                };
            }
            AttackStep::Aimed(index) => {
                if matches!(
                    child,
                    Some(Outcome::Finished(Resolution::Hero(
                        HeroAdjustment::DirectHit
                    )))
                ) {
                    attack.direct_hits.push(attack.targets[index]);
                }
                attack.step = AttackStep::Declare(index + 1);
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
                    direct_hit: attack.direct_hits.contains(&target),
                    shan_required: self.hero_response_count(seat, CardKind::Shan),
                    cards: attack.cards.clone(),
                };
                attack.step = AttackStep::Resolve(index + offset + 1);
                self.scheduler
                    .next(Program::Equipment(Flow::Target(target)));
            }
        }
        self.continue_equipment(Flow::Attack(attack));
    }

    pub(super) fn resolve_sha_target(
        &mut self,
        mut attack: Target,
        child: Option<Outcome<Resolution>>,
    ) {
        let (seat, target) = (attack.seat, attack.target);
        match (attack.step, child) {
            (TargetStep::Defend, None) if !attack.direct_hit => {
                self.ask_shan(target, attack.ignore_armor)
            }
            (TargetStep::Defend, None | Some(Outcome::Finished(Resolution::Passed)))
            | (TargetStep::Miss, Some(Outcome::Finished(Resolution::ForcedHit))) => {
                let damage = ShaDamage {
                    seat,
                    target,
                    step: DamageStep::Before,
                    amount: 1 + self.hero_damage_bonus(Some(seat), Some(CardKind::Sha)),
                    cards: attack.cards.clone(),
                };
                attack.step = TargetStep::Damage;
                self.continue_equipment(Flow::Target(attack));
                self.scheduler
                    .next(Program::Equipment(Flow::ShaDamage(damage)));
            }
            (TargetStep::Defend, Some(Outcome::Finished(Resolution::Responded))) => {
                if attack.shan_required > 1 {
                    attack.shan_required -= 1;
                    self.ask_shan(target, attack.ignore_armor);
                    self.continue_equipment(Flow::Target(attack));
                } else if let Some(kind) = self.miss_skill(seat, target) {
                    attack.step = TargetStep::Miss;
                    self.continue_equipment(Flow::Target(attack));
                    self.scheduler
                        .next(Program::Equipment(Flow::Skill(Skill::new(
                            seat, target, kind,
                        ))));
                } else {
                    self.scheduler.end(Outcome::Cancelled);
                }
            }
            (TargetStep::Defend | TargetStep::Miss, Some(_)) => {
                self.scheduler.end(Outcome::Cancelled)
            }
            (TargetStep::Damage, Some(_)) => self.finish(),
            _ => unreachable!("the attack target waits for defense, a miss skill, or damage"),
        }
    }

    fn ask_shan(&mut self, target: SeatId, ignore_armor: bool) {
        self.scheduler.next(Program::Equipment(Flow::response(
            target,
            CardKind::Shan,
            ignore_armor,
        )));
    }

    pub(super) fn resolve_sha_damage(
        &mut self,
        mut damage: ShaDamage,
        child: Option<Outcome<Resolution>>,
    ) {
        let (seat, target) = (damage.seat, damage.target);
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
                let context =
                    self.damage_context(Some(seat), target, damage.amount, damage.cards.clone());
                self.continue_equipment(Flow::ShaDamage(damage));
                self.resolve_card_damage(context, None);
            }
            DamageStep::Apply => self.resolve_card_damage(
                self.damage_context(Some(seat), target, damage.amount, damage.cards),
                child,
            ),
        }
    }
}
