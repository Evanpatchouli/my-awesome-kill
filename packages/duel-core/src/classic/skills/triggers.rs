use super::{branch, option, selection, HeroAdjustment, HeroFlow, HeroMoment, SkillId};
use crate::classic_data::Suit;
use crate::scheduler::Outcome;
use crate::{CardId, CardKind, DrawReason, EventKind, Game, Program, Resolution, SeatId, Zone};

#[derive(Clone)]
pub(crate) struct Queue {
    moment: Option<HeroMoment>,
    event: crate::TriggerEvent,
    pending: Option<Vec<Candidate>>,
    adjustment: HeroAdjustment,
    choosing: bool,
    choice_indices: Vec<usize>,
    anchor: SeatId,
    scanned_registrations: std::collections::HashSet<crate::TriggerId>,
    completed: std::collections::HashSet<(SeatId, String)>,
    cursor: Option<(u8, usize)>,
}
#[derive(Clone)]
enum Candidate {
    Elemental(crate::elemental::Rule),
    Native(SeatId, SkillId),
    Registered(crate::TriggerId),
    Qilin { source: SeatId, target: SeatId },
}
impl Queue {
    pub(crate) fn with_elemental_rule(mut self, rule: Option<crate::elemental::Rule>) -> Self {
        if let Some(rule) = rule {
            self.pending = Some(vec![Candidate::Elemental(rule)]);
        }
        self
    }
    pub(crate) fn kind(&self) -> EventKind {
        EventKind::Trigger {
            seat: self
                .event
                .player
                .or(self.event.target)
                .unwrap_or(self.anchor),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Trigger {
    owner: SeatId,
    skill: SkillId,
    moment: HeroMoment,
    stage: Stage,
}

#[derive(Clone)]
enum Stage {
    Start,
    Option,
    Judge,
    Discard,
    GuanxingTop,
    GuanxingBottom { top: Vec<CardId> },
    Distribute,
    Repeat,
}

impl Trigger {
    pub(super) fn kind(&self) -> EventKind {
        EventKind::SkillTrigger {
            seat: self.owner,
            skill: self.skill,
        }
    }
}

impl Game {
    fn moment_event(&self, moment: &HeroMoment) -> crate::TriggerEvent {
        use HeroMoment::*;
        let (timing, player, source, target, amount, cards) = match moment {
            Preparation { seat } => ("phaseZhunbeiBegin", Some(*seat), None, None, 0, vec![]),
            Draw { seat, amount } => ("phaseDrawBegin1", Some(*seat), None, None, *amount, vec![]),
            Discard { seat } => ("phaseDiscardBefore", Some(*seat), None, None, 0, vec![]),
            End { seat } => ("phaseJieshuBegin", Some(*seat), None, None, 0, vec![]),
            UseTrick { seat, cards, .. } => ("useCard", Some(*seat), None, None, 0, cards.clone()),
            DamageAfter {
                source,
                target,
                amount,
                cards,
            } => (
                "damageEnd",
                Some(*target),
                *source,
                None,
                *amount,
                cards.clone(),
            ),
            JudgeBefore { seat, card } => ("judge", Some(*seat), None, None, 0, vec![*card]),
            JudgeAfter { seat, card, .. } => ("judgeEnd", Some(*seat), None, None, 0, vec![*card]),
            ShaRedirect { seat, target, .. } => (
                "useCardToTarget",
                Some(*seat),
                None,
                Some(*target),
                0,
                vec![],
            ),
            ShaTarget { seat, target } => (
                "useCardToPlayered",
                Some(*seat),
                None,
                Some(*target),
                0,
                vec![],
            ),
            LostCards { seat, cards } => (
                "loseAfter",
                Some(*seat),
                None,
                None,
                cards.len() as u16,
                cards.iter().map(|(_, card)| *card).collect(),
            ),
        };
        self.subject_timing(timing, player, source, target, amount, &cards)
    }
    pub(crate) fn has_hero_triggers(&self, moment: &HeroMoment) -> bool {
        if !self.register_hero_triggers(moment).is_empty() {
            return true;
        }
        if self.trigger_registrations.is_empty() && self.temporary_skills.is_empty() {
            return false;
        }
        let event = self.moment_event(moment);
        self.has_registered_candidates(&event)
            || self.has_matching_expiry(&event)
            || (matches!(moment, HeroMoment::Draw { .. }) && {
                let mut second = event;
                second.timing = "phaseDrawBegin2".into();
                self.has_registered_candidates(&second) || self.has_matching_expiry(&second)
            })
    }
    pub(crate) fn hero_triggers(&mut self, moment: HeroMoment) -> Program {
        let adjustment = match moment {
            HeroMoment::Draw { amount, .. } => HeroAdjustment::Draw(amount),
            HeroMoment::JudgeBefore { card, .. } => HeroAdjustment::Judgment(card),
            _ => HeroAdjustment::None,
        };
        let event = self.moment_event(&moment);
        self.begin_timing_request(&event);
        Program::Dispatch(Queue {
            moment: Some(moment),
            event,
            pending: None,
            adjustment,
            choosing: false,
            choice_indices: Vec::new(),
            anchor: self.active_seat,
            scanned_registrations: Default::default(),
            completed: Default::default(),
            cursor: None,
        })
    }
    pub(crate) fn dispatch_for_timing(&self, event: crate::TriggerEvent) -> Queue {
        Queue {
            moment: None,
            event,
            pending: None,
            adjustment: HeroAdjustment::None,
            choosing: false,
            choice_indices: Vec::new(),
            anchor: self.active_seat,
            scanned_registrations: Default::default(),
            completed: Default::default(),
            cursor: None,
        }
    }
    pub(crate) fn queue_sha_damage_begin2(
        &mut self,
        source: SeatId,
        target: SeatId,
        amount: u16,
        cards: &[crate::Card],
    ) -> bool {
        let native = self.qilin_program(source, target);
        if !self.has_timing_registrations("damageBegin2") {
            if let Some(program) = native {
                self.scheduler.next(program);
                return true;
            }
            return false;
        }
        let event = self.subject_timing(
            "damageBegin2",
            Some(target),
            Some(source),
            None,
            amount,
            cards,
        );
        self.begin_timing_request(&event);
        if !self.has_registered_candidates(&event) {
            if let Some(program) = native {
                self.scheduler.next(program);
                return true;
            }
            return false;
        }
        let has_bow = self.seat(source).is_ok_and(|seat| {
            seat.alive
                && seat
                    .equipment
                    .iter()
                    .any(|card| card.kind == CardKind::Qilin)
        });
        let mut queue = self.dispatch_for_timing(event);
        queue.pending = Some(if has_bow {
            vec![Candidate::Qilin { source, target }]
        } else {
            vec![]
        });
        self.scheduler.next(Program::Dispatch(queue));
        true
    }
    pub(crate) fn resolve_dispatch(
        &mut self,
        mut queue: Queue,
        child: Option<Outcome<Resolution>>,
    ) {
        if queue.event.source.is_some()
            && matches!(
                self.scheduler.active_frame(queue.event.subject),
                Some((_, Program::CardDamage(_)))
            )
        {
            queue.event.source = self.live_damage_source(queue.event.source);
            if let Some(HeroMoment::DamageAfter { source, .. }) = &mut queue.moment {
                *source = queue.event.source;
            }
        }
        if matches!(child, Some(Outcome::Cancelled)) && !queue.choosing {
            self.scheduler.end(Outcome::Cancelled);
            return;
        }
        if let Some(Outcome::Finished(Resolution::Hero(adjustment))) = &child {
            match *adjustment {
                HeroAdjustment::None => {}
                HeroAdjustment::Draw(amount) => {
                    if let Some(HeroMoment::Draw {
                        amount: current, ..
                    }) = &mut queue.moment
                    {
                        *current = amount;
                    }
                    queue.event.amount = amount;
                    queue.adjustment = *adjustment;
                }
                HeroAdjustment::Judgment(card) => {
                    if let Some(HeroMoment::JudgeBefore { card: current, .. }) = &mut queue.moment {
                        *current = card;
                    }
                    queue.event.cards = vec![card];
                    queue.adjustment = *adjustment;
                }
                _ => {
                    self.finish_hero(*adjustment);
                    return;
                }
            }
        }
        if self.match_result().is_some() {
            self.finish_hero(queue.adjustment);
            return;
        }
        if queue.pending.is_none() {
            let mut candidates = Vec::new();
            if let Some(moment) = &queue.moment {
                let mut native = self.register_hero_triggers(moment);
                if matches!(moment, HeroMoment::Draw { .. }) {
                    native.retain(|(_, skill)| {
                        (*skill == SkillId::Tuxi) == (queue.event.timing == "phaseDrawBegin1")
                    });
                }
                native.reverse();
                candidates.extend(
                    native
                        .into_iter()
                        .map(|(seat, skill)| Candidate::Native(seat, skill)),
                );
            }
            queue.pending = Some(candidates);
        }
        let order = self.seat_order_from(queue.anchor).unwrap_or_default();
        // A suspended choice uses stable indexes. Newly registered content is merged
        // after that choice resumes, into groups which have not yet closed.
        if !queue.choosing {
            let mut ids: Vec<_> = self.trigger_registrations.keys().copied().collect();
            ids.sort_unstable();
            for id in ids {
                if !queue.scanned_registrations.insert(id) {
                    continue;
                }
                let registration = &self.trigger_registrations[&id];
                if registration.timing != queue.event.timing
                    || queue
                        .completed
                        .contains(&(registration.owner, registration.skill.clone()))
                {
                    continue;
                }
                let seat = order
                    .iter()
                    .position(|s| *s == registration.owner)
                    .unwrap_or(usize::MAX);
                let group = match registration.order {
                    crate::TriggerOrder::First => (0, 0),
                    crate::TriggerOrder::Normal => (1, seat),
                    crate::TriggerOrder::Last => (2, 0),
                };
                if queue.cursor.is_some_and(|cursor| group < cursor) {
                    continue;
                }
                queue
                    .pending
                    .as_mut()
                    .unwrap()
                    .push(Candidate::Registered(id));
            }
        }
        let pending = queue.pending.as_ref().unwrap();
        let key = |candidate: &Candidate| -> Option<(u8, usize, i64, usize)> {
            let (owner, group, priority) = match candidate {
                Candidate::Elemental(rule) => (
                    rule.owner(self),
                    crate::TriggerOrder::Normal,
                    rule.priority(),
                ),
                Candidate::Native(owner, _) => (*owner, crate::TriggerOrder::Normal, 0),
                Candidate::Qilin { source, .. } => (*source, crate::TriggerOrder::Normal, 0),
                Candidate::Registered(id) => {
                    let r = self.trigger_registrations.get(id)?;
                    (r.owner, r.order, r.priority)
                }
            };
            let seat = order.iter().position(|s| *s == owner).unwrap_or(usize::MAX);
            let subpriority = match candidate {
                Candidate::Elemental(_) => -1,
                Candidate::Qilin { .. } => 25,
                Candidate::Registered(id) if self.trigger_registrations[id].silent => -1,
                _ => 0,
            };
            Some(match group {
                crate::TriggerOrder::First => {
                    (0, 0, -i64::from(priority) * 100 + subpriority, seat)
                }
                crate::TriggerOrder::Normal => {
                    (1, seat, -i64::from(priority) * 100 + subpriority, 0)
                }
                crate::TriggerOrder::Last => (2, 0, -i64::from(priority) * 100 + subpriority, seat),
            })
        };
        let live_native = queue
            .moment
            .as_ref()
            .map(|moment| self.register_hero_triggers(moment));
        let eligible = |c: &Candidate| match c {
            Candidate::Elemental(rule) => rule.eligible(self),
            Candidate::Native(owner, skill) => {
                self.seat(*owner).is_ok_and(|s| s.alive)
                    && live_native
                        .as_ref()
                        .is_some_and(|skills| skills.contains(&(*owner, *skill)))
                    && queue
                        .moment
                        .as_ref()
                        .is_some_and(|moment| self.native_trigger_eligible(*owner, *skill, moment))
            }
            Candidate::Registered(id) => self.registered_matches(*id, &queue.event),
            Candidate::Qilin { source, target } => self.qilin_program(*source, *target).is_some(),
        };
        let selected = if queue.choosing {
            queue.choosing = false;
            match child {
                Some(Outcome::Finished(Resolution::Selected(s))) if s.branch > 0 => {
                    Some((s.branch - 1) as usize)
                }
                _ => None,
            }
        } else {
            None
        };
        // A pass evaluates every candidate in the current first/seat/last group once.
        // A choice resumes that same pass without running its predicates again.
        let mut distinct = Vec::new();
        if selected.is_some() {
            distinct.extend(queue.choice_indices.iter().copied().filter(|&i| {
                pending.get(i).is_some_and(|candidate| match candidate {
                    Candidate::Elemental(rule) => rule.eligible(self),
                    Candidate::Native(owner, skill) => {
                        self.seat(*owner).is_ok_and(|s| s.alive)
                            && self.has_skill(*owner, skill.as_str())
                    }
                    Candidate::Registered(id) => self.trigger_registrations.contains_key(id),
                    Candidate::Qilin { source, target } => {
                        self.qilin_program(*source, *target).is_some()
                    }
                })
            }));
        } else {
            let mut groups: Vec<_> = pending
                .iter()
                .filter_map(&key)
                .map(|k| (k.0, k.1))
                .collect();
            groups.sort_unstable();
            groups.dedup();
            for group in groups {
                let mut seen = std::collections::HashMap::new();
                for (i, candidate) in pending.iter().enumerate() {
                    if !key(candidate).is_some_and(|k| (k.0, k.1) == group) || !eligible(candidate)
                    {
                        continue;
                    }
                    let identity = match candidate {
                        Candidate::Elemental(rule) => (rule.owner(self), rule.skill().to_owned()),
                        Candidate::Native(owner, skill) => (*owner, skill.as_str().to_owned()),
                        Candidate::Qilin { source, .. } => (*source, "qilin_skill".to_owned()),
                        Candidate::Registered(id) => {
                            let r = &self.trigger_registrations[id];
                            (r.owner, r.skill.clone())
                        }
                    };
                    if let Some(&index) = seen.get(&identity) {
                        if key(candidate) < key(&pending[distinct[index]]) {
                            distinct[index] = i;
                        }
                    } else {
                        seen.insert(identity, distinct.len());
                        distinct.push(i);
                    }
                }
                if !distinct.is_empty() {
                    break;
                }
            }
        }
        if selected.is_some_and(|index| !distinct.contains(&index)) {
            queue.choice_indices.clear();
            self.scheduler.replace_current(Program::Dispatch(queue));
            return;
        }
        let best = distinct
            .iter()
            .filter_map(|&i| key(&pending[i]).map(|k| (i, k)))
            .min_by_key(|(_, k)| *k);
        let Some((first, best_key)) = best else {
            if matches!(queue.moment, Some(HeroMoment::Draw { .. }))
                && queue.event.timing == "phaseDrawBegin1"
            {
                queue.event.timing = "phaseDrawBegin2".into();
                self.begin_timing_request(&queue.event);
                queue.pending = None;
                queue.scanned_registrations.clear();
                queue.completed.clear();
                queue.cursor = None;
                self.scheduler.replace_current(Program::Dispatch(queue));
            } else {
                self.finish_hero(queue.adjustment);
            }
            return;
        };
        let selected = selected.filter(|i| distinct.contains(i));
        let tied: Vec<usize> = distinct
            .iter()
            .copied()
            .filter(|i| key(&pending[*i]) == Some(best_key))
            .collect();
        let visible = |c: &Candidate| match c {
            Candidate::Elemental(_) => None,
            Candidate::Native(owner, skill) => self
                .seat(*owner)
                .ok()
                .and_then(|seat| seat.hero)
                .and_then(|hero| {
                    hero.skills
                        .iter()
                        .find(|definition| definition.id == skill.as_str())
                })
                .map(|definition| definition.name.clone()),
            Candidate::Qilin { .. } => Some("麒麟弓".into()),
            Candidate::Registered(id) => self.trigger_registrations.get(id).and_then(|r| {
                if r.silent {
                    None
                } else {
                    r.label.clone().filter(|s| !s.is_empty())
                }
            }),
        };
        let owner = |c: &Candidate| match c {
            Candidate::Elemental(rule) => rule.owner(self),
            Candidate::Native(s, _) => *s,
            Candidate::Qilin { source, .. } => *source,
            Candidate::Registered(id) => self.trigger_registrations[id].owner,
        };
        if selected.is_none()
            && best_key.0 == 1
            && tied.len() > 1
            && tied.iter().all(|i| visible(&pending[*i]).is_some())
        {
            {
                let seat = owner(&pending[first]);
                let branches = tied
                    .iter()
                    .map(|i| option((*i + 1) as u32, &visible(&pending[*i]).unwrap()))
                    .collect();
                queue.choosing = true;
                queue.choice_indices = tied.clone();
                self.scheduler.replace_current(Program::Dispatch(queue));
                self.scheduler.next(Program::Choose(crate::ChoiceRequest {
                    seat,
                    target: None,
                    branches,
                }));
                return;
            }
        }
        let chosen = selected.unwrap_or_else(|| {
            if best_key.0 == 1 {
                tied.iter()
                    .copied()
                    .find(|i| visible(&pending[*i]).is_none())
                    .unwrap_or(first)
            } else {
                first
            }
        });
        let chosen_candidate = pending[chosen].clone();
        let identity = match &chosen_candidate {
            Candidate::Elemental(rule) => (rule.owner(self), rule.skill().to_owned()),
            Candidate::Native(owner, skill) => (*owner, skill.as_str().to_owned()),
            Candidate::Qilin { source, .. } => (*source, "qilin_skill".to_owned()),
            Candidate::Registered(id) => {
                let r = &self.trigger_registrations[id];
                (r.owner, r.skill.clone())
            }
        };
        let registered_chosen = matches!(chosen_candidate, Candidate::Registered(_));
        let pruned: Vec<_> = pending
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                if *i == chosen || !key(c).is_some_and(|k| k >= best_key) {
                    return false;
                }
                // Indexed native repeats are separate activations; duplicate registrations across roles are not.
                match c {
                    Candidate::Elemental(rule) => {
                        (rule.owner(self), rule.skill()) != (identity.0, identity.1.as_str())
                    }
                    Candidate::Registered(id) => {
                        let r = &self.trigger_registrations[id];
                        (r.owner, r.skill.as_str()) != (identity.0, identity.1.as_str())
                    }
                    Candidate::Qilin { source, .. } => {
                        !registered_chosen
                            || (*source, "qilin_skill") != (identity.0, identity.1.as_str())
                    }
                    Candidate::Native(owner, skill) => {
                        !registered_chosen
                            || (*owner, skill.as_str()) != (identity.0, identity.1.as_str())
                    }
                }
            })
            .map(|(_, c)| c.clone())
            .collect();
        queue.pending = Some(pruned);
        queue.cursor = Some((best_key.0, best_key.1));
        queue.completed.insert(identity);
        let event = queue.event.clone();
        let moment = queue.moment.clone();
        self.scheduler.replace_current(Program::Dispatch(queue));
        match chosen_candidate {
            Candidate::Elemental(rule) => self.scheduler.next(rule.program(self)),
            Candidate::Qilin { source, target } => {
                if let Some(program) = self.qilin_program(source, target) {
                    self.scheduler.next(program);
                }
            }
            Candidate::Native(owner, skill) => {
                self.scheduler
                    .next(Program::Hero(HeroFlow::Trigger(Trigger {
                        owner,
                        skill,
                        moment: moment.unwrap(),
                        stage: Stage::Start,
                    })))
            }
            Candidate::Registered(id) => {
                let owner = self.trigger_registrations[&id].owner;
                let effect = self.trigger_registrations[&id].effect.clone();
                self.scheduler
                    .next(Program::Registered(crate::triggers::Registered::new(
                        id, owner, event, effect,
                    )));
            }
        }
    }

    // Match the availability gates used by the native skill before presenting
    // it alongside other same-priority skills to the owner.
    fn native_trigger_eligible(&self, owner: SeatId, skill: SkillId, moment: &HeroMoment) -> bool {
        use SkillId::*;
        match (skill, moment) {
            (Jianxiong, HeroMoment::DamageAfter { cards, .. }) => {
                cards.iter().any(|card| self.resolving.contains(card))
            }
            (Fankui, HeroMoment::DamageAfter { source, .. }) => source.is_some_and(|source| {
                self.seat(source).is_ok_and(|seat| {
                    seat.alive
                        && ((!seat.hand.is_empty() && source != owner)
                            || !seat.equipment.is_empty())
                })
            }),
            (Tuxi, HeroMoment::Draw { .. }) => {
                self.living_order_from(owner).into_iter().any(|target| {
                    target != owner && self.seat(target).is_ok_and(|seat| !seat.hand.is_empty())
                })
            }
            (Luoyi | Yingzi, HeroMoment::Draw { amount, .. }) => *amount > 0,
            (Keji, HeroMoment::Discard { .. }) => {
                self.classic
                    .as_ref()
                    .is_some_and(|state| !state.hero_turn.played_sha)
                    && self
                        .seat(owner)
                        .is_ok_and(|seat| seat.hand.len() > usize::try_from(seat.hp).unwrap_or(0))
            }
            (Guicai, HeroMoment::JudgeBefore { .. }) => {
                !self.hero_owned_card_ids(owner, false).is_empty()
            }
            (Tiandu, HeroMoment::JudgeAfter { card, .. }) => self.resolving.contains(card),
            (
                Liuli,
                HeroMoment::ShaRedirect {
                    seat: attacker,
                    target: original,
                    excluded,
                },
            ) => self
                .hero_owned_card_ids(owner, true)
                .into_iter()
                .any(|cost| {
                    self.living_order_from(*original).into_iter().any(|target| {
                        target != *original
                            && target != *attacker
                            && !excluded.contains(&target)
                            && self.hero_allows_target(target, CardKind::Sha)
                            && self.in_sha_range_without(owner, target, Some(cost))
                    })
                }),
            _ => true,
        }
    }

    fn register_hero_triggers(&self, moment: &HeroMoment) -> Vec<(SeatId, SkillId)> {
        use HeroMoment::*;
        use SkillId::*;
        let mut calls = Vec::new();
        let mut register = |seat, skill: SkillId, times| {
            if self.has_skill(seat, skill.as_str()) {
                calls.extend(std::iter::repeat_n((seat, skill), times));
            }
        };
        match *moment {
            Preparation { seat } => {
                register(seat, Guanxing, 1);
                register(seat, Luoshen, 1);
            }
            Draw { seat, .. } => {
                register(seat, Tuxi, 1);
                register(seat, Luoyi, 1);
                register(seat, Yingzi, 1);
            }
            Discard { seat } => register(seat, Keji, 1),
            End { seat } => register(seat, Biyue, 1),
            UseTrick { seat, kind, .. } if !matches!(kind, CardKind::Lebu | CardKind::Shandian) => {
                register(seat, Jizhi, 1)
            }
            DamageAfter { target, amount, .. } => {
                register(target, Jianxiong, 1);
                register(target, Fankui, 1);
                register(target, Ganglie, 1);
                register(target, Yiji, amount as usize);
            }
            JudgeBefore { .. } => {
                for seat in self.living_order_from(self.active_seat) {
                    register(seat, Guicai, 1);
                }
            }
            JudgeAfter {
                seat, keep_black, ..
            } => {
                register(seat, Tiandu, 1);
                if keep_black {
                    register(seat, Luoshen, 1);
                }
            }
            ShaRedirect { target, .. } => register(target, Liuli, 1),
            ShaTarget { seat, .. } => register(seat, Tieji, 1),
            LostCards { seat, ref cards } => {
                if cards.iter().any(|(zone, _)| matches!(zone, Zone::Hand))
                    && self.seat(seat).is_ok_and(|player| player.hand.is_empty())
                {
                    register(seat, Lianying, 1);
                }
                register(
                    seat,
                    Xiaoji,
                    cards
                        .iter()
                        .filter(|(zone, _)| matches!(zone, Zone::Equipment))
                        .count(),
                );
            }
            _ => {}
        }
        calls.reverse();
        calls
    }

    pub(super) fn resolve_hero_trigger(
        &mut self,
        trigger: Trigger,
        child: Option<Outcome<Resolution>>,
    ) {
        use SkillId::*;
        if matches!(trigger.stage, Stage::Option | Stage::Repeat)
            && matches!(&child, Some(Outcome::Finished(Resolution::Selected(choice))) if choice.branch != 0)
        {
            self.scheduler.next(Program::Hero(HeroFlow::Activation {
                seat: trigger.owner,
                skill: trigger.skill,
            }));
        }
        match trigger.skill {
            Biyue | Jizhi | Lianying | Xiaoji => self.resolve_hero_optional_draw(trigger, child),
            Guanxing => self.resolve_guanxing(trigger, child),
            Luoshen => self.resolve_luoshen(trigger, child),
            Tuxi | Yingzi | Luoyi | Keji => self.resolve_hero_phase_modifier(trigger, child),
            Jianxiong | Fankui | Ganglie | Yiji => self.resolve_hero_damage_trigger(trigger, child),
            Guicai | Tiandu => self.resolve_hero_judgment_trigger(trigger, child),
            Liuli | Tieji => self.resolve_hero_sha_trigger(trigger, child),
            _ => unreachable!("only registered skills can be queued"),
        }
    }

    fn continue_hero_trigger(&mut self, mut trigger: Trigger, stage: Stage) {
        trigger.stage = stage;
        self.scheduler
            .replace_current(Program::Hero(HeroFlow::Trigger(trigger)));
    }

    fn resolve_hero_optional_draw(&mut self, trigger: Trigger, child: Option<Outcome<Resolution>>) {
        let owner = trigger.owner;
        let (name, amount) = match trigger.skill {
            SkillId::Biyue => ("闭月", 1),
            SkillId::Jizhi => ("集智", 1),
            SkillId::Lianying if self.seat(owner).is_ok_and(|player| player.hand.is_empty()) => {
                ("连营", 1)
            }
            SkillId::Xiaoji => ("枭姬", 2),
            SkillId::Lianying => {
                self.finish_hero(HeroAdjustment::None);
                return;
            }
            _ => unreachable!(),
        };
        if matches!(trigger.stage, Stage::Start) {
            self.continue_hero_trigger(trigger, Stage::Option);
            self.choose_hero_yes(owner, None, name);
        } else {
            if accepted(&child) {
                self.queue_draw(owner, amount, DrawReason::CardEffect);
            }
            self.finish_hero(HeroAdjustment::None);
        }
    }

    fn resolve_hero_phase_modifier(
        &mut self,
        trigger: Trigger,
        child: Option<Outcome<Resolution>>,
    ) {
        use SkillId::*;
        let owner = trigger.owner;
        let amount = match trigger.moment {
            HeroMoment::Draw { amount, .. } => amount,
            _ => 0,
        };
        let modifier = match trigger.stage {
            Stage::Start => {
                let branches = match trigger.skill {
                    Tuxi => {
                        let targets: Vec<_> = self
                            .living_order_from(owner)
                            .into_iter()
                            .skip(1)
                            .filter(|&target| {
                                self.seat(target).is_ok_and(|seat| !seat.hand.is_empty())
                            })
                            .collect();
                        if targets.is_empty() {
                            Vec::new()
                        } else {
                            vec![
                                option(0, "正常摸牌"),
                                branch(1, "突袭", vec![], (0, 0), targets, (1, 2)),
                            ]
                        }
                    }
                    Luoyi if amount > 0 => vec![option(0, "正常摸牌"), option(1, "裸衣")],
                    Yingzi if amount > 0 => vec![option(0, "正常摸牌"), option(1, "英姿")],
                    Keji if self
                        .classic
                        .as_ref()
                        .is_some_and(|state| !state.hero_turn.played_sha)
                        && self.seat(owner).is_ok_and(|seat| {
                            seat.hand.len() > usize::try_from(seat.hp).unwrap_or(0)
                        }) =>
                    {
                        vec![option(0, "正常弃牌"), option(1, "克己")]
                    }
                    _ => Vec::new(),
                };
                if !branches.is_empty() {
                    self.continue_hero_trigger(trigger, Stage::Option);
                    self.choose_hero(owner, None, branches);
                    return;
                }
                HeroAdjustment::None
            }
            _ => match selection(child).filter(|choice| choice.branch == 1) {
                Some(choice) => match trigger.skill {
                    Tuxi => {
                        for target in choice.targets {
                            if let Some(card) = self.random_hand_card(target) {
                                self.hero_transfer_card(target, Zone::Hand, card.id, owner);
                            }
                        }
                        HeroAdjustment::Draw(0)
                    }
                    Luoyi => {
                        self.classic.as_mut().expect("classic turn").hero_turn.luoyi = Some(owner);
                        HeroAdjustment::Draw(amount - 1)
                    }
                    Yingzi => HeroAdjustment::Draw(amount + 1),
                    Keji => HeroAdjustment::Skip,
                    _ => unreachable!(),
                },
                None => HeroAdjustment::None,
            },
        };
        self.finish_hero(modifier);
    }

    fn resolve_hero_damage_trigger(
        &mut self,
        trigger: Trigger,
        child: Option<Outcome<Resolution>>,
    ) {
        let HeroMoment::DamageAfter {
            source, ref cards, ..
        } = trigger.moment
        else {
            unreachable!()
        };
        let owner = trigger.owner;
        match trigger.skill {
            SkillId::Jianxiong => {
                let available: Vec<_> = cards
                    .iter()
                    .filter(|card| self.resolving.contains(card))
                    .copied()
                    .collect();
                if matches!(trigger.stage, Stage::Start) && !available.is_empty() {
                    self.continue_hero_trigger(trigger, Stage::Option);
                    self.choose_hero_yes(owner, source, "奸雄");
                    return;
                }
                if accepted(&child) {
                    for card in available {
                        let gained = self.take_resolving(card.id);
                        self.put_in_hand(owner, gained);
                    }
                }
            }
            SkillId::Fankui => {
                if let Some(source) =
                    source.filter(|seat| self.seat(*seat).is_ok_and(|player| player.alive))
                {
                    if self.resolve_fankui(trigger, source, child) {
                        return;
                    }
                }
            }
            SkillId::Ganglie => {
                if self.resolve_ganglie(trigger, source, child) {
                    return;
                }
            }
            SkillId::Yiji => {
                if self.resolve_yiji(trigger, child) {
                    return;
                }
            }
            _ => unreachable!(),
        }
        self.finish_hero(HeroAdjustment::None);
    }

    fn resolve_fankui(
        &mut self,
        trigger: Trigger,
        source: SeatId,
        child: Option<Outcome<Resolution>>,
    ) -> bool {
        let owner = trigger.owner;
        if matches!(trigger.stage, Stage::Start) {
            let opponent = self.seat(source).expect("the source exists");
            let mut options = vec![option(0, "放弃")];
            if source != owner && !opponent.hand.is_empty() {
                options.push(option(1, "反馈：随机获得手牌"));
            }
            if !opponent.equipment.is_empty() {
                options.push(branch(
                    2,
                    "反馈：获得装备",
                    opponent.equipment.iter().map(|card| card.id).collect(),
                    (1, 1),
                    vec![],
                    (0, 0),
                ));
            }
            if options.len() == 1 {
                return false;
            }
            self.continue_hero_trigger(trigger, Stage::Option);
            self.choose_hero(owner, Some(source), options);
            return true;
        }
        let Some(choice) = selection(child) else {
            return false;
        };
        if choice.branch == 1 {
            if let Some(card) = self.random_hand_card(source) {
                self.hero_transfer_card(source, Zone::Hand, card.id, owner);
            }
        } else if choice.branch == 2 {
            self.hero_transfer_card(source, Zone::Equipment, choice.cards[0], owner);
        }
        false
    }

    fn resolve_ganglie(
        &mut self,
        trigger: Trigger,
        source: Option<SeatId>,
        child: Option<Outcome<Resolution>>,
    ) -> bool {
        let owner = trigger.owner;
        match trigger.stage {
            Stage::Start => {
                self.continue_hero_trigger(trigger, Stage::Option);
                self.choose_hero_yes(owner, source, "刚烈");
            }
            Stage::Option if accepted(&child) => {
                self.continue_hero_trigger(trigger, Stage::Judge);
                self.scheduler.next(self.judge_program(owner, false));
            }
            Stage::Judge => {
                let Some(source) =
                    source.filter(|seat| self.seat(*seat).is_ok_and(|player| player.alive))
                else {
                    return false;
                };
                let Some(Outcome::Finished(Resolution::Judged(card))) = child else {
                    return false;
                };
                if self
                    .face(card.id)
                    .is_none_or(|face| face.suit == Suit::Heart)
                {
                    return false;
                }
                let hand = self.hero_owned_card_ids(source, false);
                if hand.len() < 2 {
                    self.next(EventKind::Damage {
                        source: Some(owner),
                        target: source,
                        amount: 1,
                    });
                    return false;
                }
                self.continue_hero_trigger(trigger, Stage::Discard);
                self.choose_hero(
                    source,
                    Some(owner),
                    vec![
                        option(0, "受到刚烈伤害"),
                        branch(1, "弃置两张手牌", hand, (2, 2), vec![], (0, 0)),
                    ],
                );
            }
            Stage::Discard => {
                let source = source.expect("only a living source receives the discard choice");
                if let Some(choice) = selection(child).filter(|choice| choice.branch == 1) {
                    self.hero_discard_cards(source, &choice.cards);
                } else {
                    self.next(EventKind::Damage {
                        source: Some(owner),
                        target: source,
                        amount: 1,
                    });
                }
                return false;
            }
            _ => return false,
        }
        true
    }

    fn resolve_yiji(&mut self, trigger: Trigger, child: Option<Outcome<Resolution>>) -> bool {
        let owner = trigger.owner;
        match trigger.stage {
            Stage::Start => {
                self.continue_hero_trigger(trigger, Stage::Option);
                self.choose_hero_yes(owner, None, "遗计");
            }
            Stage::Option if accepted(&child) => {
                let Some(cards) = self.draw_from_piles(2) else {
                    return false;
                };
                self.classic
                    .as_mut()
                    .expect("the table is classic")
                    .hero_turn
                    .preview = cards;
                self.continue_hero_trigger(trigger, Stage::Distribute);
                self.choose_yiji(owner);
            }
            Stage::Distribute => {
                let choice = selection(child).expect("the distribution selection is required");
                for id in choice.cards {
                    let preview = &mut self
                        .classic
                        .as_mut()
                        .expect("the table is classic")
                        .hero_turn
                        .preview;
                    let card = Self::remove_card(preview, id);
                    self.put_in_hand(choice.targets[0], card);
                }
                if !self.choose_yiji(owner) {
                    return false;
                }
            }
            _ => return false,
        }
        true
    }

    fn choose_yiji(&mut self, owner: SeatId) -> bool {
        let preview = &self
            .classic
            .as_ref()
            .expect("the table is classic")
            .hero_turn
            .preview;
        if preview.is_empty() {
            return false;
        }
        self.choose_hero(
            owner,
            None,
            vec![branch(
                1,
                "遗计：选择牌和获得者",
                preview.iter().map(|card| card.id).collect(),
                (1, preview.len()),
                self.living_order_from(owner),
                (1, 1),
            )],
        );
        true
    }

    fn resolve_hero_judgment_trigger(
        &mut self,
        trigger: Trigger,
        child: Option<Outcome<Resolution>>,
    ) {
        let owner = trigger.owner;
        match trigger.moment {
            HeroMoment::JudgeBefore { seat, card } => {
                if matches!(trigger.stage, Stage::Start) {
                    let hand = self.hero_owned_card_ids(owner, false);
                    if !hand.is_empty() {
                        self.continue_hero_trigger(trigger, Stage::Option);
                        self.choose_hero(
                            owner,
                            Some(seat),
                            vec![
                                option(0, "保留判定"),
                                branch(1, "鬼才", hand, (1, 1), vec![], (0, 0)),
                            ],
                        );
                        return;
                    }
                } else if let Some(choice) = selection(child).filter(|choice| choice.branch == 1) {
                    let replacement = self.take_card(owner, Zone::Hand, choice.cards[0]);
                    self.resolving.push(replacement);
                    self.queue_hero_losses(owner, vec![(Zone::Hand, replacement)]);
                    self.next(EventKind::Discard { card });
                    self.next(EventKind::Reveal {
                        seat,
                        card: replacement,
                    });
                    self.finish_hero(HeroAdjustment::Judgment(replacement));
                    return;
                }
            }
            HeroMoment::JudgeAfter { card, .. } if self.resolving.contains(&card) => {
                if matches!(trigger.stage, Stage::Start) {
                    self.continue_hero_trigger(trigger, Stage::Option);
                    self.choose_hero_yes(owner, None, "天妒");
                    return;
                }
                if accepted(&child) {
                    let card = self.take_resolving(card.id);
                    self.put_in_hand(owner, card);
                }
            }
            _ => {}
        }
        self.finish_hero(HeroAdjustment::None);
    }

    fn resolve_guanxing(&mut self, trigger: Trigger, child: Option<Outcome<Resolution>>) {
        let owner = trigger.owner;
        match trigger.stage {
            Stage::Start => {
                self.continue_hero_trigger(trigger, Stage::Option);
                self.choose_hero_yes(owner, None, "观星");
                return;
            }
            Stage::Option if accepted(&child) => {
                let amount = self.living_order_from(owner).len().min(5);
                if let Some(cards) = self.draw_from_piles(amount) {
                    let ids = cards.iter().map(|card| card.id).collect();
                    self.classic
                        .as_mut()
                        .expect("the table is classic")
                        .hero_turn
                        .preview = cards;
                    self.continue_hero_trigger(trigger, Stage::GuanxingTop);
                    self.choose_hero(
                        owner,
                        None,
                        vec![branch(
                            1,
                            "观星：按顺序放在牌堆顶",
                            ids,
                            (0, amount),
                            vec![],
                            (0, 0),
                        )],
                    );
                    return;
                }
            }
            Stage::GuanxingTop => {
                let top = selection(child).expect("the top order is required").cards;
                let preview = &self
                    .classic
                    .as_ref()
                    .expect("the table is classic")
                    .hero_turn
                    .preview;
                let bottom: Vec<_> = preview
                    .iter()
                    .filter(|card| !top.contains(&card.id))
                    .map(|card| card.id)
                    .collect();
                if bottom.len() <= 1 {
                    self.place_guanxing_cards(&top, &bottom);
                } else {
                    let count = bottom.len();
                    self.continue_hero_trigger(trigger, Stage::GuanxingBottom { top });
                    self.choose_hero(
                        owner,
                        None,
                        vec![branch(
                            1,
                            "观星：按顺序放在牌堆底",
                            bottom,
                            (count, count),
                            vec![],
                            (0, 0),
                        )],
                    );
                    return;
                }
            }
            Stage::GuanxingBottom { top } => {
                let bottom = selection(child)
                    .expect("the bottom order is required")
                    .cards;
                self.place_guanxing_cards(&top, &bottom);
            }
            _ => {}
        }
        self.finish_hero(HeroAdjustment::None);
    }

    fn place_guanxing_cards(&mut self, top: &[CardId], bottom: &[CardId]) {
        let state = self.classic.as_mut().expect("the table is classic");
        let preview = &mut state.hero_turn.preview;
        let bottom_cards: Vec<_> = bottom
            .iter()
            .rev()
            .map(|id| Self::remove_card(preview, *id))
            .collect();
        state.deck.splice(..0, bottom_cards);
        state
            .deck
            .extend(top.iter().rev().map(|id| Self::remove_card(preview, *id)));
    }

    fn resolve_luoshen(&mut self, trigger: Trigger, child: Option<Outcome<Resolution>>) {
        let owner = trigger.owner;
        if let HeroMoment::JudgeAfter { card, .. } = trigger.moment {
            if self.hero_card_is_black(card) && self.resolving.contains(&card) {
                let card = self.take_resolving(card.id);
                self.put_in_hand(owner, card);
            }
        } else {
            match trigger.stage {
                Stage::Start => {
                    self.continue_hero_trigger(trigger, Stage::Option);
                    self.choose_hero_yes(owner, None, "洛神");
                    return;
                }
                Stage::Option | Stage::Repeat if accepted(&child) => {
                    self.continue_hero_trigger(trigger, Stage::Judge);
                    self.scheduler.next(self.judge_program(owner, true));
                    return;
                }
                Stage::Judge => {
                    if let Some(Outcome::Finished(Resolution::Judged(card))) = child {
                        if self.hero_card_is_black(card) && self.match_result().is_none() {
                            self.continue_hero_trigger(trigger, Stage::Repeat);
                            self.choose_hero_yes(owner, None, "再次发动洛神");
                            return;
                        }
                    }
                }
                _ => {}
            }
        }
        self.finish_hero(HeroAdjustment::None);
    }

    fn resolve_hero_sha_trigger(&mut self, trigger: Trigger, child: Option<Outcome<Resolution>>) {
        let owner = trigger.owner;
        let result = match &trigger.moment {
            HeroMoment::ShaTarget { target, .. } => {
                let target = *target;
                match trigger.stage {
                    Stage::Start => {
                        self.continue_hero_trigger(trigger, Stage::Option);
                        self.choose_hero_yes(owner, Some(target), "铁骑");
                        return;
                    }
                    Stage::Option if accepted(&child) => {
                        self.continue_hero_trigger(trigger, Stage::Judge);
                        self.scheduler.next(self.judge_program(owner, false));
                        return;
                    }
                    Stage::Judge if matches!(child, Some(Outcome::Finished(Resolution::Judged(card))) if self.hero_card_is_red(card)) => {
                        HeroAdjustment::DirectHit
                    }
                    _ => HeroAdjustment::None,
                }
            }
            HeroMoment::ShaRedirect {
                seat,
                target,
                excluded,
            } => {
                let attacker = *seat;
                let original = *target;
                if matches!(trigger.stage, Stage::Start) {
                    let targets: Vec<_> = self
                        .living_order_from(original)
                        .into_iter()
                        .skip(1)
                        .filter(|&target| {
                            target != attacker
                                && !excluded.contains(&target)
                                && self.hero_allows_target(target, CardKind::Sha)
                        })
                        .collect();
                    let mut branches = vec![option(0, "放弃")];
                    for cost in self.hero_owned_card_ids(owner, true) {
                        let choices: Vec<_> = targets
                            .iter()
                            .copied()
                            .filter(|&target| self.in_sha_range_without(owner, target, Some(cost)))
                            .collect();
                        if choices.is_empty() {
                            continue;
                        }
                        if let Some(existing) =
                            branches.iter_mut().find(|branch| branch.targets == choices)
                        {
                            existing.cards.push(cost);
                        } else {
                            branches.push(branch(
                                branches.len() as u32,
                                "流离",
                                vec![cost],
                                (1, 1),
                                choices,
                                (1, 1),
                            ));
                        }
                    }
                    if branches.len() > 1 {
                        self.continue_hero_trigger(trigger, Stage::Option);
                        self.choose_hero(owner, Some(attacker), branches);
                        return;
                    }
                    HeroAdjustment::None
                } else if let Some(choice) = selection(child).filter(|choice| choice.branch != 0) {
                    self.hero_discard_cards(owner, &choice.cards);
                    HeroAdjustment::Redirect(choice.targets[0])
                } else {
                    HeroAdjustment::None
                }
            }
            _ => unreachable!(),
        };
        self.finish_hero(result);
    }
}

fn accepted(child: &Option<Outcome<Resolution>>) -> bool {
    matches!(child, Some(Outcome::Finished(Resolution::Selected(choice))) if choice.branch == 1)
}

#[cfg(test)]
mod dispatch_regressions {
    use super::*;
    use crate::{
        Action, Card, ChoiceKind, Command, GameSetup, SeatSetup, TriggerEvent, TriggerOrder,
        TriggerRegistration, TriggerRole, TriggerStep,
    };
    use std::sync::{Arc, Mutex};

    fn game() -> Game {
        Game::new(GameSetup {
            active_seat: 0,
            seats: vec![
                SeatSetup {
                    id: 0,
                    hp: 4,
                    hand: vec![],
                },
                SeatSetup {
                    id: 1,
                    hp: 4,
                    hand: vec![],
                },
            ],
        })
        .unwrap()
    }
    fn registration(
        name: &'static str,
        order: TriggerOrder,
        label: Option<&str>,
        log: Arc<Mutex<Vec<&'static str>>>,
    ) -> TriggerRegistration {
        TriggerRegistration {
            owner: 0,
            skill: name.into(),
            timing: "testTiming".into(),
            role: TriggerRole::Global,
            order,
            priority: 0,
            label: label.map(str::to_owned),
            silent: false,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(move |_, _, _| {
                log.lock().unwrap().push(name);
                TriggerStep::Done
            }),
        }
    }
    fn start(game: &mut Game) {
        let event = TriggerEvent {
            timing: "testTiming".into(),
            parent: None,
            card_kind: None,
            nature: None,
            delta: None,
            subject: 0,
            player: Some(0),
            source: None,
            target: None,
            amount: 0,
            cards: vec![],
        };
        game.scheduler
            .start(Program::Dispatch(game.dispatch_for_timing(event)));
        game.run(None);
    }
    fn choose(game: &mut Game, branch: u32) {
        let choice = game.view(Some(0)).unwrap().choice.unwrap();
        assert_eq!(choice.kind, ChoiceKind::Content);
        game.submit(Command {
            version: game.version,
            seat: 0,
            action: Action::Select {
                event: choice.event,
                branch,
                cards: vec![],
                targets: vec![],
            },
        })
        .unwrap();
    }
    #[test]
    fn first_and_last_keep_registration_order_when_one_is_unnamed() {
        let mut game = game();
        let log = Arc::new(Mutex::new(vec![]));
        for (name, order, label) in [
            ("first_named", TriggerOrder::First, Some("named")),
            ("first_unnamed", TriggerOrder::First, None),
            ("last_named", TriggerOrder::Last, Some("named")),
            ("last_unnamed", TriggerOrder::Last, None),
        ] {
            game.register_trigger(registration(name, order, label, log.clone()))
                .unwrap();
        }
        start(&mut game);
        assert_eq!(
            *log.lock().unwrap(),
            ["first_named", "first_unnamed", "last_named", "last_unnamed"]
        );
    }
    #[test]
    fn callbacks_add_open_group_and_revoke_pending_content() {
        let mut game = game();
        let log = Arc::new(Mutex::new(vec![]));
        let revoked = game
            .register_trigger(registration(
                "revoked",
                TriggerOrder::Last,
                None,
                log.clone(),
            ))
            .unwrap();
        let log_clone = log.clone();
        let mut add = registration("add", TriggerOrder::Normal, None, log.clone());
        add.effect = Arc::new(move |ctx, _, _| {
            log_clone.lock().unwrap().push("add");
            assert!(ctx.unregister_trigger(revoked));
            ctx.register_trigger(registration(
                "added",
                TriggerOrder::Last,
                None,
                log_clone.clone(),
            ))
            .unwrap();
            TriggerStep::Done
        });
        game.register_trigger(add).unwrap();
        start(&mut game);
        assert_eq!(*log.lock().unwrap(), ["add", "added"]);
    }
    #[test]
    fn registrations_added_while_choice_is_suspended_join_later_group() {
        let mut game = game();
        let log = Arc::new(Mutex::new(vec![]));
        for name in ["one", "two"] {
            game.register_trigger(registration(
                name,
                TriggerOrder::Normal,
                Some(name),
                log.clone(),
            ))
            .unwrap();
        }
        start(&mut game);
        assert!(game.view(Some(0)).unwrap().choice.is_some());
        game.register_trigger(registration("last", TriggerOrder::Last, None, log.clone()))
            .unwrap();
        choose(&mut game, 1);
        assert_eq!(*log.lock().unwrap(), ["one", "two", "last"]);
    }
    #[test]
    fn revoking_a_suspended_choice_does_not_execute_it_or_stall() {
        let mut game = game();
        let log = Arc::new(Mutex::new(vec![]));
        let revoked = game
            .register_trigger(registration(
                "one",
                TriggerOrder::Normal,
                Some("one"),
                log.clone(),
            ))
            .unwrap();
        game.register_trigger(registration(
            "two",
            TriggerOrder::Normal,
            Some("two"),
            log.clone(),
        ))
        .unwrap();
        start(&mut game);
        assert!(game.unregister_trigger(revoked));
        choose(&mut game, 1);
        assert_eq!(*log.lock().unwrap(), ["two"]);
        assert!(game.view(Some(0)).unwrap().choice.is_none());
    }

    #[test]
    fn newly_registered_closed_groups_and_completed_skills_do_not_reappear() {
        let mut game = game();
        let log = Arc::new(Mutex::new(vec![]));
        let output = log.clone();
        let mut normal = registration("normal", TriggerOrder::Normal, None, log.clone());
        normal.effect = Arc::new(move |ctx, _, _| {
            output.lock().unwrap().push("normal");
            ctx.register_trigger(registration(
                "late_first",
                TriggerOrder::First,
                None,
                output.clone(),
            ))
            .unwrap();
            ctx.register_trigger(registration(
                "normal",
                TriggerOrder::Last,
                None,
                output.clone(),
            ))
            .unwrap();
            ctx.register_trigger(registration(
                "late_last",
                TriggerOrder::Last,
                None,
                output.clone(),
            ))
            .unwrap();
            TriggerStep::Done
        });
        game.register_trigger(normal).unwrap();
        start(&mut game);
        assert_eq!(*log.lock().unwrap(), ["normal", "late_last"]);
    }

    #[test]
    fn a_callback_can_add_a_skill_to_its_open_seat_group() {
        let mut game = game();
        let log = Arc::new(Mutex::new(vec![]));
        let output = log.clone();
        let mut first = registration("first", TriggerOrder::Normal, None, log.clone());
        first.effect = Arc::new(move |ctx, _, _| {
            output.lock().unwrap().push("first");
            ctx.register_trigger(registration(
                "second",
                TriggerOrder::Normal,
                None,
                output.clone(),
            ))
            .unwrap();
            TriggerStep::Done
        });
        game.register_trigger(first).unwrap();
        start(&mut game);
        assert_eq!(*log.lock().unwrap(), ["first", "second"]);
    }

    #[test]
    fn unavailable_native_skill_is_absent_from_order_choice() {
        let mut game = game();
        game.seat_mut(0).hero = crate::classic_data::roster()
            .iter()
            .find(|hero| hero.id == "caocao");
        let log = Arc::new(Mutex::new(vec![]));
        let mut custom = registration("custom", TriggerOrder::Normal, Some("custom"), log.clone());
        custom.timing = "damageEnd".into();
        game.register_trigger(custom).unwrap();
        let moment = HeroMoment::DamageAfter {
            source: Some(1),
            target: 0,
            amount: 1,
            cards: vec![],
        };
        let dispatch = game.hero_triggers(moment);
        game.scheduler.start(dispatch);
        game.run(None);
        assert_eq!(*log.lock().unwrap(), ["custom"]);
        assert!(game.view(Some(0)).unwrap().choice.is_none());
    }

    #[test]
    fn native_damage_skills_without_material_are_not_eligible() {
        let mut game = game();
        let material = Card {
            id: 8,
            kind: CardKind::Sha,
        };
        let moment = HeroMoment::DamageAfter {
            source: Some(1),
            target: 0,
            amount: 1,
            cards: vec![material],
        };
        assert!(!game.native_trigger_eligible(0, SkillId::Jianxiong, &moment));
        assert!(!game.native_trigger_eligible(0, SkillId::Fankui, &moment));
        game.resolving.push(material);
        game.seat_mut(1).hand.push(Card {
            id: 9,
            kind: CardKind::Tao,
        });
        assert!(game.native_trigger_eligible(0, SkillId::Jianxiong, &moment));
        assert!(game.native_trigger_eligible(0, SkillId::Fankui, &moment));
    }
}
