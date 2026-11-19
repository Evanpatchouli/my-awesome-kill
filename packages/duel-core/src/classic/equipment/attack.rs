use super::{skill::Skill, Flow};
use crate::classic::skills::{HeroAdjustment, HeroMoment};
use crate::scheduler::{EventRef, Outcome};
use crate::{
    Card, CardKind, DamageNature, EventKind, Game, Program, Resolution, RuleError, SeatId,
    TriggerEvent,
};

pub(crate) enum UseChange {
    Cancel,
    Exclude(SeatId),
    ExcludeAll,
}

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
    UseEnd,
    After,
    FinishCancelled,
}

#[derive(Clone)]
pub(crate) struct Attack {
    base_damage: u16,
    seat: SeatId,
    payer: SeatId,
    cards: Vec<Card>,
    targets: Vec<SeatId>,
    announced_target: Option<SeatId>,
    direct_hits: Vec<SeatId>,
    step: AttackStep,
    nature: Option<DamageNature>,
    content_use: bool,
    cancel_requested: bool,
    cancel_dispatch: Option<Box<crate::classic::skills::Dispatch>>,
    all_excluded: bool,
    excluded: Vec<SeatId>,
}

impl Attack {
    pub(crate) fn cancellation_requested(&self) -> bool {
        self.cancel_requested
    }
    pub(crate) fn matches_root_timing(&self, timing: &str) -> bool {
        matches!(
            (timing, self.step),
            ("useCard2", AttackStep::TargetsChanged) | ("useCard", AttackStep::BeforeTargets)
        )
    }
    pub(crate) fn matches_target_timing(&self, seat: SeatId, target: SeatId) -> bool {
        self.seat == seat
            && matches!(self.step, AttackStep::Aimed(index) if self.targets.get(index) == Some(&target))
    }
    pub(crate) fn exclude_all(&mut self) {
        self.all_excluded = true;
    }
    pub(crate) fn exclude_target(&mut self, target: SeatId) -> Result<(), RuleError> {
        if !self.targets.contains(&target) {
            return Err(RuleError::InvalidTarget);
        }
        if !self.excluded.contains(&target) {
            self.excluded.push(target);
        }
        Ok(())
    }
    pub(crate) fn base_damage_mut(&mut self) -> Option<&mut u16> {
        matches!(self.step, AttackStep::TargetsChanged).then_some(&mut self.base_damage)
    }
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
    MissTiming,
    Miss,
    Damage,
    AfterDamage,
}

#[derive(Clone)]
pub(crate) struct Target {
    base_damage: u16,
    pub(super) seat: SeatId,
    pub(super) target: SeatId,
    step: TargetStep,
    use_event: crate::EventId,
    direct_hit: bool,
    shan_required: u8,
    cards: Vec<Card>,
    nature: Option<DamageNature>,
}

impl Game {
    pub(crate) fn change_sha_use(
        &mut self,
        event: &TriggerEvent,
        change: UseChange,
    ) -> Result<(), RuleError> {
        let mut cleanup = Vec::new();
        let result = self.change_sha_use_with_cleanup(event, change, &mut cleanup);
        for program in cleanup {
            self.scheduler.next_cleanup(program);
        }
        result
    }

    pub(crate) fn change_sha_use_with_cleanup(
        &mut self,
        event: &TriggerEvent,
        change: UseChange,
        cleanup: &mut Vec<Program>,
    ) -> Result<(), RuleError> {
        let (frame, program) = self
            .scheduler
            .active_frame(event.subject)
            .ok_or(RuleError::WrongEvent)?;
        let root = match program {
            Program::Equipment(Flow::Attack(attack))
                if attack.matches_root_timing(&event.timing)
                    && event.player == Some(attack.seat) =>
            {
                event.subject
            }
            Program::TargetTiming(flow)
                if event.timing == "useCardToPlayered"
                    && flow.timing == crate::CardTargetTiming::UseCardToPlayered
                    && flow.card_kind == CardKind::Sha
                    && event.player == Some(flow.seat)
                    && event.target == Some(flow.target)
                    && event.parent == frame.parent =>
            {
                let parent = frame.parent.ok_or(RuleError::WrongEvent)?;
                match self.scheduler.active_frame(parent) {
                    Some((_, Program::Equipment(Flow::Attack(attack))))
                        if attack.matches_target_timing(flow.seat, flow.target) =>
                    {
                        parent
                    }
                    _ => return Err(RuleError::WrongEvent),
                }
            }
            _ => return Err(RuleError::WrongEvent),
        };
        match change {
            UseChange::Cancel => self.request_sha_root_cancel(root, cleanup),
            UseChange::Exclude(target) => {
                let Some(Program::Equipment(Flow::Attack(attack))) =
                    self.scheduler.active_program_mut(root)
                else {
                    return Err(RuleError::WrongEvent);
                };
                attack.exclude_target(target)
            }
            UseChange::ExcludeAll => {
                let Some(Program::Equipment(Flow::Attack(attack))) =
                    self.scheduler.active_program_mut(root)
                else {
                    return Err(RuleError::WrongEvent);
                };
                attack.exclude_all();
                Ok(())
            }
        }
    }
    fn request_sha_root_cancel(
        &mut self,
        root: crate::EventId,
        cleanup: &mut Vec<Program>,
    ) -> Result<(), RuleError> {
        let frame = self
            .scheduler
            .active_frame(root)
            .map(|(frame, _)| frame)
            .ok_or(RuleError::WrongEvent)?;
        let (seat, cards) = {
            let Some(Program::Equipment(Flow::Attack(attack))) =
                self.scheduler.active_program_mut(root)
            else {
                return Err(RuleError::WrongEvent);
            };
            if attack.cancel_requested {
                return Ok(());
            }
            attack.cancel_requested = true;
            (attack.seat, attack.cards.clone())
        };
        if self.wants_timing("useCardCancelled") {
            let mut event = self.subject_timing_at(
                frame,
                "useCardCancelled",
                Some(seat),
                None,
                None,
                0,
                &cards,
            );
            event.card_kind = Some(CardKind::Sha);
            self.begin_timing_request_with_cleanup(&event, cleanup);
            let dispatch = self.dispatch_for_deferred_request(event).map(Box::new);
            let Some(Program::Equipment(Flow::Attack(attack))) =
                self.scheduler.active_program_mut(root)
            else {
                return Err(RuleError::WrongEvent);
            };
            attack.cancel_dispatch = dispatch;
        }
        Ok(())
    }
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
        Program::Equipment(Flow::Attack(Attack {
            base_damage: 1,
            seat,
            payer,
            cards,
            targets,
            announced_target,
            direct_hits: vec![],
            step: AttackStep::Pay,
            nature: None,
            content_use: false,
            cancel_requested: false,
            cancel_dispatch: None,
            all_excluded: false,
            excluded: vec![],
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
        event: EventRef,
        mut attack: Attack,
        child: Option<Outcome<Resolution>>,
    ) {
        if self.match_result().is_some() {
            self.finish();
            return;
        }
        let seat = attack.seat;
        if attack.cancel_requested && !matches!(attack.step, AttackStep::FinishCancelled) {
            attack.step = AttackStep::FinishCancelled;
            let dispatch = attack.cancel_dispatch.take();
            self.continue_equipment(Flow::Attack(attack));
            if let Some(dispatch) = dispatch {
                self.scheduler.next(Program::Dispatch(*dispatch));
            } else {
                self.scheduler.end(Outcome::Cancelled);
            }
            return;
        }
        if matches!(
            attack.step,
            AttackStep::TargetsChanged | AttackStep::BeforeTargets
        ) && matches!(child, Some(Outcome::Cancelled))
        {
            self.scheduler.end(Outcome::Cancelled);
            return;
        }
        if attack.all_excluded
            && matches!(
                attack.step,
                AttackStep::BeforeTargets
                    | AttackStep::Redirect(_)
                    | AttackStep::Redirected(_)
                    | AttackStep::Declare(_)
                    | AttackStep::Aim(_)
                    | AttackStep::Aimed(_)
                    | AttackStep::Resolve(_)
            )
        {
            attack.step = AttackStep::UseEnd;
            let cards = attack.cards.clone();
            self.continue_equipment(Flow::Attack(attack));
            if !self.queue_timing("useCardEnd", Some(seat), None, None, 0, &cards) {
                // Resume through UseEnd even when no listener exists.
            }
            return;
        }
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
                    if !self.is_in(target) {
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
                    if !self.is_in(target) {
                        attack.step = AttackStep::Declare(index + 1);
                        self.continue_equipment(Flow::Attack(attack));
                        return;
                    }
                    attack.step = AttackStep::Aim(index);
                } else {
                    attack.step = AttackStep::Resolve(0);
                }
            }
            AttackStep::Aim(index) => {
                let target = attack.targets[index];
                attack.step = if self.queue_card_target_timing(
                    crate::CardTargetTiming::UseCardToPlayered,
                    seat,
                    target,
                    CardKind::Sha,
                    &attack.cards,
                ) {
                    AttackStep::Aimed(index)
                } else {
                    AttackStep::Declare(index + 1)
                };
            }
            AttackStep::Aimed(index) => {
                attack.step = AttackStep::Declare(index + 1);
            }
            AttackStep::Resolve(index) => {
                let Some((offset, target)) = attack
                    .targets
                    .iter()
                    .copied()
                    .skip(index)
                    .enumerate()
                    .find(|(_, target)| self.is_in(*target))
                else {
                    attack.step = AttackStep::UseEnd;
                    self.queue_qinggang_timing(
                        "useCardEnd",
                        Some(seat),
                        None,
                        None,
                        0,
                        &attack.cards,
                        Some(event.id),
                        None,
                    );
                    self.continue_equipment(Flow::Attack(attack));
                    return;
                };
                let target = Target {
                    base_damage: attack.base_damage,
                    seat,
                    target,
                    step: TargetStep::Defend,
                    use_event: event.id,
                    direct_hit: attack.direct_hits.contains(&target),
                    shan_required: self.hero_response_count(seat, CardKind::Shan),
                    cards: attack.cards.clone(),
                    nature: attack.nature,
                };
                attack.step = AttackStep::Resolve(index + offset + 1);
                if attack.excluded.contains(&target.target) {
                    self.queue_card_target_excluded(seat, target.target, &attack.cards);
                } else {
                    self.scheduler
                        .next(Program::Equipment(Flow::Target(target)));
                }
            }
            AttackStep::UseEnd => {
                if matches!(child, Some(Outcome::Cancelled)) {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                attack.step = AttackStep::After;
                if self.queue_timing("useCardAfter", Some(seat), None, None, 0, &attack.cards) {
                    self.continue_equipment(Flow::Attack(attack));
                } else {
                    self.finish();
                }
                return;
            }
            AttackStep::After => {
                self.finish_attack_hook(child);
                return;
            }
            AttackStep::FinishCancelled => {
                self.scheduler.end(Outcome::Cancelled);
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
        if matches!(
            attack.step,
            TargetStep::Defend | TargetStep::MissTiming | TargetStep::Miss
        ) && !self.is_in(target)
        {
            self.scheduler.end(Outcome::Cancelled);
            return;
        }
        match (attack.step, child) {
            (TargetStep::Defend, None) if !attack.direct_hit => self.ask_shan(target),
            (TargetStep::Defend, None | Some(Outcome::Finished(Resolution::Passed)))
            | (TargetStep::Miss, Some(Outcome::Finished(Resolution::ForcedHit))) => {
                let mut damage = self.damage_program_with_nature(
                    Some(seat),
                    target,
                    attack.base_damage,
                    Some(CardKind::Sha),
                    attack.cards.clone(),
                    attack.nature,
                );
                if let Program::CardDamage(ref mut damage) = damage {
                    damage.sha_use = Some(attack.use_event);
                }
                attack.step = TargetStep::Damage;
                self.continue_equipment(Flow::Target(attack));
                self.scheduler.next(damage);
            }
            (
                TargetStep::Defend,
                Some(Outcome::Finished(Resolution::Responded | Resolution::Dodged)),
            ) => {
                if attack.shan_required > 1 {
                    attack.shan_required -= 1;
                    self.ask_shan(target);
                    self.continue_equipment(Flow::Target(attack));
                } else if self.queue_sha_miss(seat, target, &attack.cards, attack.use_event) {
                    attack.step = TargetStep::MissTiming;
                    self.continue_equipment(Flow::Target(attack));
                } else {
                    self.continue_sha_miss(attack);
                }
            }
            (TargetStep::MissTiming, Some(Outcome::Cancelled)) => {
                self.scheduler.end(Outcome::Cancelled)
            }
            (TargetStep::MissTiming, Some(_)) => self.continue_sha_miss(attack),
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

    fn queue_sha_miss(
        &mut self,
        seat: SeatId,
        target: SeatId,
        cards: &[Card],
        use_event: crate::EventId,
    ) -> bool {
        self.queue_qinggang_timing(
            "shaMiss",
            Some(seat),
            None,
            Some(target),
            0,
            cards,
            Some(use_event),
            Some(&[target]),
        )
    }

    fn continue_sha_miss(&mut self, mut attack: Target) {
        if let Some(kind) = self.miss_skill(attack.seat, attack.target) {
            let (seat, target) = (attack.seat, attack.target);
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

    fn ask_shan(&mut self, target: SeatId) {
        if self.classic.is_some() || !self.card_enabled(target, CardKind::Shan) {
            self.scheduler.next(Program::Equipment(Flow::response(
                target,
                CardKind::Shan,
                false,
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

    pub(crate) fn sha_equipment_eligible(
        &self,
        source: SeatId,
        target: SeatId,
        kind: CardKind,
    ) -> bool {
        let Some(skill) = crate::equipment_registry::sha_equipment_skill(kind) else {
            return false;
        };
        if !self.is_in(source)
            || !self.is_in(target)
            || !self.has_skill(source, skill)
            || self
                .seat(source)
                .ok()
                .and_then(|seat| seat.equipment.hook(skill))
                .is_none()
        {
            return false;
        }
        if kind == CardKind::Cixiong {
            let gender = |id| {
                self.seat(id)
                    .ok()
                    .and_then(|actor| actor.hero)
                    .map(|hero| hero.gender.as_str())
            };
            return matches!((gender(source), gender(target)), (Some(a), Some(b)) if a != b);
        }
        kind == CardKind::Qinggang
    }

    pub(crate) fn sha_equipment_program(
        &self,
        source: SeatId,
        target: SeatId,
        kind: CardKind,
        subject: crate::EventId,
    ) -> Option<Program> {
        self.sha_equipment_eligible(source, target, kind).then(|| {
            Program::Equipment(Flow::Skill(Skill::sha_equipment(
                source, target, kind, subject,
            )))
        })
    }

    pub(crate) fn mark_sha_target(
        &mut self,
        subject: crate::EventId,
        target: SeatId,
        direct_hit: bool,
    ) {
        if let Some(Program::Equipment(Flow::Attack(attack))) =
            self.scheduler.active_program_mut(subject)
        {
            if direct_hit && !attack.direct_hits.contains(&target) {
                attack.direct_hits.push(target);
            }
        }
    }

    pub(crate) fn qilin_program(&self, seat: SeatId, target: SeatId) -> Option<Program> {
        (self.is_in(seat)
            && self.is_in(target)
            && self.has_weapon(seat, CardKind::Qilin)
            && !self.target_horses(target).is_empty())
        .then(|| Program::Equipment(Flow::Skill(Skill::new(seat, target, CardKind::Qilin))))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "attack/timing_tests.rs"]
mod timing_tests;
