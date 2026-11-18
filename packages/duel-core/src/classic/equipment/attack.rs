use super::{skill::Skill, Flow};
use crate::classic::skills::{HeroAdjustment, HeroMoment};
use crate::scheduler::Outcome;
use crate::{
    Card, CardKind, DamageNature, EventKind, Game, Program, Resolution, RuleError, SeatId,
};

#[derive(Clone, Copy)]
enum AttackStep {
    Pay,
    UseCard2,
    TargetsChanged,
    BeforeTargets,
    Redirect(usize),
    Redirected(usize),
    Declare(usize),
    Aim(usize),
    Aimed(usize),
    Resolve(usize),
    After,
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
    nature: Option<DamageNature>,
    content_use: bool,
}

impl Attack {
    pub(crate) fn use_targets(&self) -> &[SeatId] {
        &self.targets
    }

    pub(crate) fn editable_targets(&mut self) -> Option<&mut Vec<SeatId>> {
        matches!(self.step, AttackStep::TargetsChanged).then_some(&mut self.targets)
    }

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
    AfterDamage,
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
    nature: Option<DamageNature>,
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
            nature: None,
            content_use: false,
        }))
    }

    /// Build a directly authorized virtual Sha without inventing a physical card.
    pub(crate) fn content_sha_program(
        &self,
        source: SeatId,
        target: SeatId,
        nature: Option<DamageNature>,
    ) -> Result<Program, RuleError> {
        if !self.accepts_sha_target(source, target) || !self.card_enabled(source, CardKind::Sha) {
            return Err(RuleError::InvalidTarget);
        }
        let Program::Equipment(Flow::Attack(mut attack)) =
            self.attack_program(source, vec![], vec![target])
        else {
            unreachable!("attack_program creates an attack")
        };
        attack.nature = nature;
        attack.content_use = true;
        Ok(Program::Equipment(Flow::Attack(attack)))
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
                if attack.content_use
                    && (!self.card_enabled(seat, CardKind::Sha)
                        || !attack
                            .targets
                            .iter()
                            .all(|&target| self.accepts_sha_target(seat, target)))
                {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                if attack.content_use {
                    if let Some(state) = self.classic.as_mut().filter(|state| {
                        state.turn.is_some_and(|turn| {
                            turn.seat == seat && turn.phase == crate::Phase::Play
                        })
                    }) {
                        state.sha_played = true;
                    }
                }
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
                if self.classic.is_some() {
                    self.queue_use_costs(attack.payer, &attack.cards);
                } else {
                    // The prescribed table records material payment on the Sha event itself.
                    for card in &attack.cards {
                        self.move_to_resolving(attack.payer, card.id);
                    }
                }
                for &card in &attack.cards {
                    self.scheduler
                        .after(Program::event(EventKind::Discard { card }));
                }
                attack.step = AttackStep::UseCard2;
            }
            AttackStep::UseCard2 => {
                if !self.wants_timing("useCard2") && !self.wants_timing("useCard") {
                    self.sort_card_targets(&mut attack.targets);
                    attack.step = AttackStep::Redirect(0);
                    self.continue_equipment(Flow::Attack(attack));
                    return;
                }
                attack.step = AttackStep::TargetsChanged;
                // The dispatcher reads and edits this same paid attack while it is suspended.
                let cards = attack.cards.clone();
                self.continue_equipment(Flow::Attack(attack));
                self.queue_use_card2(seat, CardKind::Sha, &cards);
                return;
            }
            AttackStep::TargetsChanged => {
                if matches!(child, Some(Outcome::Cancelled)) {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                self.queue_timing("useCard", Some(seat), None, None, 0, &attack.cards);
                attack.step = AttackStep::BeforeTargets;
            }
            AttackStep::BeforeTargets => {
                if matches!(child, Some(Outcome::Cancelled)) {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                self.sort_card_targets(&mut attack.targets);
                attack.step = AttackStep::Redirect(0);
            }
            AttackStep::Redirect(index) => {
                attack.step = if let Some(&target) = attack.targets.get(index) {
                    if !self.seat(target).is_ok_and(|s| s.alive) {
                        attack.step = AttackStep::Redirect(index + 1);
                        self.continue_equipment(Flow::Attack(attack));
                        return;
                    }
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
                    if !self.seat(target).is_ok_and(|s| s.alive) {
                        attack.step = AttackStep::Declare(index + 1);
                        self.continue_equipment(Flow::Attack(attack));
                        return;
                    }
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
                    attack.step = AttackStep::After;
                    if self.queue_timing("useCardAfter", Some(seat), None, None, 0, &attack.cards) {
                        self.continue_equipment(Flow::Attack(attack));
                    } else {
                        self.finish();
                    }
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
                    nature: attack.nature,
                };
                attack.step = AttackStep::Resolve(index + offset + 1);
                self.scheduler
                    .next(Program::Equipment(Flow::Target(target)));
            }
            AttackStep::After => {
                self.finish_attack_hook(child);
                return;
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
                let damage = self.damage_program_with_nature(
                    Some(seat),
                    target,
                    1,
                    Some(CardKind::Sha),
                    attack.cards.clone(),
                    attack.nature,
                );
                attack.step = TargetStep::Damage;
                self.continue_equipment(Flow::Target(attack));
                self.scheduler.next(damage);
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
            (TargetStep::Damage, Some(_)) => {
                // A landed Sha reports its damage result even when damage was cancelled.
                if self.queue_timing(
                    "shaDamage",
                    Some(seat),
                    None,
                    Some(target),
                    0,
                    &attack.cards,
                ) {
                    attack.step = TargetStep::AfterDamage;
                    self.continue_equipment(Flow::Target(attack));
                } else {
                    self.finish();
                }
            }
            (TargetStep::AfterDamage, child @ Some(_)) => self.finish_attack_hook(child),
            _ => unreachable!("the attack target waits for defense, a miss skill, or damage"),
        }
    }

    fn ask_shan(&mut self, target: SeatId, ignore_armor: bool) {
        if self.classic.is_some() || !self.card_enabled(target, CardKind::Shan) {
            self.scheduler.next(Program::Equipment(Flow::response(
                target,
                CardKind::Shan,
                ignore_armor,
            )));
        } else {
            self.next(EventKind::ChooseShan { seat: target });
        }
    }

    fn finish_attack_hook(&mut self, child: Option<Outcome<Resolution>>) {
        if matches!(child, Some(Outcome::Cancelled)) {
            self.scheduler.end(Outcome::Cancelled);
        } else {
            self.finish();
        }
    }

    pub(crate) fn qilin_program(&self, seat: SeatId, target: SeatId) -> Option<Program> {
        (self.seat(seat).is_ok_and(|seat| seat.alive)
            && self.has_weapon(seat, CardKind::Qilin)
            && !self.target_horses(target).is_empty())
        .then(|| Program::Equipment(Flow::Skill(Skill::new(seat, target, CardKind::Qilin))))
    }
}

#[cfg(test)]
mod tests;
