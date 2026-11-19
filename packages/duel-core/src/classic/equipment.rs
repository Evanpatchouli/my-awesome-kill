mod attack;
mod response;
mod skill;

use crate::classic::PlannedUse;
use crate::scheduler::{EventRef, Outcome};
use crate::CounterContext;
use crate::{
    Card, CardId, CardKind, ChoiceBranch, ChoiceRequest, EventKind, Game, Program, Resolution,
    RuleError, Seat, SeatId, VirtualPlay, Zone,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    Weapon,
    Armor,
    AttackHorse,
    DefenseHorse,
}

fn slot(kind: CardKind) -> Option<Slot> {
    use CardKind::*;
    match kind {
        Zhuge | Cixiong | Qinggang | Qinglong | Zhangba | Guanshi | Fangtian | Qilin => {
            Some(Slot::Weapon)
        }
        Bagua => Some(Slot::Armor),
        Jueying | Dilu | Zhuahuang => Some(Slot::DefenseHorse),
        Chitu | Dawan | Zixin => Some(Slot::AttackHorse),
        _ => None,
    }
}

pub(crate) fn is_equipment(kind: CardKind) -> bool {
    slot(kind).is_some()
}

pub(crate) fn has_slot(seat: &Seat, wanted: Slot, ignored: Option<CardId>) -> bool {
    seat.registered_equipment()
        .any(|card| Some(card.id) != ignored && slot(card.kind) == Some(wanted))
}

impl Seat {
    fn registered_equipment(&self) -> impl Iterator<Item = &Card> {
        self.equipment.iter().chain(&self.detached_equipment)
    }
}

pub(crate) fn has_equipment_skill(seat: &Seat, skill: &str) -> bool {
    use CardKind::*;
    let kind = match skill {
        "bagua_skill" => Bagua,
        "zhuge_skill" => Zhuge,
        "cixiong_skill" => Cixiong,
        "qinggang_skill" => Qinggang,
        "qinglong_skill" => Qinglong,
        "zhangba_skill" => Zhangba,
        "guanshi_skill" => Guanshi,
        "fangtian_skill" => Fangtian,
        "qilin_skill" => Qilin,
        _ => return false,
    };
    seat.registered_equipment().any(|card| card.kind == kind)
}

#[derive(Clone, Copy)]
pub(crate) enum EquipStep {
    Pay,
    Timing,
    DirectBefore,
    DirectBegin,
    DirectLose,
    Replace,
    Install,
    DirectEnd,
    DirectAfter,
    DirectFinish,
    DirectCancelled,
}

#[derive(Clone)]
pub(crate) enum Flow {
    Equip {
        seat: SeatId,
        card: Card,
        step: EquipStep,
        direct: bool,
    },
    Attack(attack::Attack),
    Target(attack::Target),
    Response(response::Response),
    Payment(response::Payment),
    Skill(skill::Skill),
}

impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::Equip { seat, card, .. } => EventKind::Equip {
                seat: *seat,
                card: *card,
            },
            Self::Attack(attack) => attack.event(),
            Self::Target(attack) => EventKind::ShaEffect {
                seat: attack.seat,
                target: attack.target,
            },
            Self::Response(response) if response.is_use() => EventKind::ChooseToUse {
                seat: response.seat,
                kind: response.kind,
            },
            Self::Response(response) => EventKind::Respond {
                seat: response.seat,
                kind: response.kind,
            },
            Self::Payment(payment) => payment.event(),
            Self::Skill(skill) => EventKind::EquipmentEffect {
                seat: skill.seat,
                target: skill.target,
                kind: skill.kind,
            },
        }
    }

    pub(crate) fn equipment_target(&self) -> Option<&SeatId> {
        match self {
            Self::Equip { seat, .. } => Some(seat),
            _ => None,
        }
    }

    pub(crate) fn response_uses_card(&self) -> Option<bool> {
        match self {
            Self::Response(response) => Some(response.context == response::ResponseContext::Use),
            _ => None,
        }
    }

    pub(crate) fn response(seat: SeatId, kind: CardKind, ignore_armor: bool) -> Self {
        Self::Response(response::Response::new(seat, kind, ignore_armor))
    }

    pub(crate) fn respond(seat: SeatId, kind: CardKind) -> Self {
        Self::Response(response::Response::respond(seat, kind))
    }

    pub(crate) fn counter_response(seat: SeatId, context: CounterContext) -> Self {
        Self::Response(response::Response::counter(seat, context))
    }
}

fn branch(id: u32, label: &str, cards: Vec<CardId>, count: usize) -> ChoiceBranch {
    ChoiceBranch {
        id,
        label: label.into(),
        cards,
        min_cards: count,
        max_cards: count,
        targets: vec![],
        min_targets: 0,
        max_targets: 0,
    }
}

impl Game {
    /// Registered equipment in the slot used by `slot_of`. Card IDs refer to the actual materials.
    pub fn equipment_registrations(&self, seat: SeatId, slot_of: CardKind) -> Vec<Card> {
        let (Ok(player), Some(wanted)) = (self.seat(seat), slot(slot_of)) else {
            return Vec::new();
        };
        player
            .registered_equipment()
            .copied()
            .filter(|card| slot(card.kind) == Some(wanted))
            .collect()
    }

    pub(crate) fn resolve_equipment(
        &mut self,
        event: EventRef,
        flow: Flow,
        child: Option<Outcome<Resolution>>,
    ) {
        match flow {
            Flow::Equip {
                seat,
                card,
                step,
                direct,
            } => self.resolve_equip(seat, card, step, direct, child),
            Flow::Attack(attack) => self.resolve_attack(attack, child),
            Flow::Target(target) => self.resolve_sha_target(target, child),
            Flow::Response(response) => self.resolve_response(response, child),
            Flow::Payment(payment) => self.resolve_response_payment(payment, child),
            Flow::Skill(skill) => self.resolve_equipment_skill(event, skill, child),
        }
    }

    pub(crate) fn equipment_uses(&self, player: &Seat, card: Card) -> Option<Vec<PlannedUse>> {
        slot(card.kind)?;
        let seat = player.id;
        Some(vec![PlannedUse {
            targets: vec![seat],
            program: Program::Equipment(Flow::Equip {
                seat,
                card,
                step: EquipStep::Pay,
                direct: false,
            }),
        }])
    }

    pub(crate) fn equip_from_hand_program(
        &self,
        seat: SeatId,
        id: CardId,
    ) -> Result<Program, RuleError> {
        let player = self.seat(seat)?;
        if !self.is_in(seat) {
            return Err(RuleError::InactiveSeat { seat });
        }
        let card = player
            .hand
            .iter()
            .copied()
            .find(|card| card.id == id && is_equipment(card.kind))
            .ok_or(RuleError::InvalidCard)?;
        Ok(Program::Equipment(Flow::Equip {
            seat,
            card,
            step: EquipStep::DirectBefore,
            direct: true,
        }))
    }

    fn resolve_equip(
        &mut self,
        seat: SeatId,
        card: Card,
        step: EquipStep,
        direct: bool,
        child: Option<Outcome<Resolution>>,
    ) {
        let program = |step| {
            Program::Equipment(Flow::Equip {
                seat,
                card,
                step,
                direct,
            })
        };
        if matches!(child, Some(Outcome::Cancelled))
            && matches!(
                step,
                EquipStep::DirectBegin
                    | EquipStep::DirectLose
                    | EquipStep::DirectAfter
                    | EquipStep::DirectFinish
            )
        {
            self.scheduler
                .replace_current(program(EquipStep::DirectCancelled));
            self.queue_timing("equipCancelled", Some(seat), None, None, 1, &[card]);
            return;
        }
        match step {
            EquipStep::DirectBefore
            | EquipStep::DirectBegin
            | EquipStep::DirectEnd
            | EquipStep::DirectAfter => {
                let (timing, next) = match step {
                    EquipStep::DirectBefore => ("equipBefore", EquipStep::DirectBegin),
                    EquipStep::DirectBegin => ("equipBegin", EquipStep::DirectLose),
                    EquipStep::DirectEnd => ("equipEnd", EquipStep::DirectAfter),
                    EquipStep::DirectAfter => ("equipAfter", EquipStep::DirectFinish),
                    _ => unreachable!(),
                };
                self.scheduler.replace_current(program(next));
                self.queue_timing(timing, Some(seat), None, None, 1, &[card]);
            }
            EquipStep::DirectLose => {
                if !self.is_in(seat)
                    || !self
                        .seat(seat)
                        .is_ok_and(|player| player.hand.contains(&card))
                {
                    self.scheduler
                        .replace_current(program(EquipStep::DirectEnd));
                    return;
                }
                self.scheduler.replace_current(program(EquipStep::Replace));
                self.scheduler
                    .next(self.lose_to_own_special(seat, card, Zone::Hand));
            }
            EquipStep::Pay => {
                self.queue_use_costs(seat, &[card]);
                self.scheduler.replace_current(program(EquipStep::Timing));
            }
            EquipStep::Timing => {
                self.scheduler.replace_current(program(EquipStep::Replace));
                self.queue_use_card2(seat, card.kind, &[card]);
            }
            EquipStep::Replace => {
                let material_available = if direct {
                    self.transferring.contains(&card)
                } else {
                    self.resolving.contains(&card)
                };
                if direct && (!material_available || !self.is_in(seat)) {
                    self.scheduler
                        .replace_current(program(EquipStep::DirectEnd));
                    if material_available {
                        self.scheduler.next(Program::JudgmentCleanup(
                            super::hero_integration::Cleanup::Cards {
                                cards: vec![card],
                                stage: super::hero_integration::DiscardStage::Before,
                            },
                        ));
                    }
                    return;
                }
                if matches!(child, Some(Outcome::Cancelled))
                    || !self.is_in(seat)
                    || !material_available
                {
                    self.scheduler
                        .after(Program::event(EventKind::Discard { card }));
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                let current = self.seat(seat).expect("the equipping seat exists");
                let old = current
                    .equipment
                    .iter()
                    .find(|old| slot(old.kind) == slot(card.kind))
                    .copied();
                self.scheduler.replace_current(program(EquipStep::Install));
                if let Some(old) = old {
                    self.scheduler.next(Program::DiscardFrom {
                        seat,
                        zone: Zone::Equipment,
                        card: old,
                    });
                }
            }
            EquipStep::Install => {
                let occupied = self.seat(seat).is_ok_and(|player| {
                    slot(card.kind).is_some_and(|wanted| has_slot(player, wanted, None))
                });
                let available = if direct {
                    self.transferring.contains(&card)
                } else {
                    self.resolving.contains(&card)
                };
                if occupied && available && self.is_in(seat) {
                    self.scheduler.next(Program::JudgmentCleanup(
                        super::hero_integration::Cleanup::Cards {
                            cards: vec![card],
                            stage: super::hero_integration::DiscardStage::Before,
                        },
                    ));
                    if direct {
                        self.scheduler
                            .replace_current(program(EquipStep::DirectEnd));
                    } else {
                        self.finish();
                    }
                    return;
                }
                if direct {
                    if let Some(index) = self.transferring.iter().position(|&c| c == card) {
                        if self.is_in(seat) {
                            let taken = self.transferring.remove(index);
                            self.seat_mut(seat).equipment.push(taken);
                        } else {
                            self.scheduler.next(Program::JudgmentCleanup(
                                super::hero_integration::Cleanup::Cards {
                                    cards: vec![card],
                                    stage: super::hero_integration::DiscardStage::Before,
                                },
                            ));
                        }
                    }
                    self.scheduler
                        .replace_current(program(EquipStep::DirectEnd));
                    return;
                }
                if !self.is_in(seat) || !available {
                    self.scheduler
                        .after(Program::event(EventKind::Discard { card }));
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                let card = self.take_resolving(card.id);
                self.seat_mut(seat).equipment.push(card);
                self.finish();
            }
            EquipStep::DirectFinish => self.finish(),
            EquipStep::DirectCancelled => self.scheduler.end(Outcome::Cancelled),
        }
    }

    pub(crate) fn weapon(&self, seat: SeatId) -> Option<Card> {
        self.seat(seat)
            .ok()?
            .registered_equipment()
            .find(|card| slot(card.kind) == Some(Slot::Weapon))
            .copied()
    }

    fn has_weapon(&self, seat: SeatId, kind: CardKind) -> bool {
        self.weapon(seat).is_some_and(|card| card.kind == kind)
    }

    pub(crate) fn in_sha_range(&self, seat: SeatId, target: SeatId) -> bool {
        self.in_sha_range_without(seat, target, None)
    }

    pub(crate) fn in_sha_range_without(
        &self,
        seat: SeatId,
        target: SeatId,
        ignored: Option<CardId>,
    ) -> bool {
        if self.has_skill(seat, "undist") || self.has_skill(target, "undist") {
            return false;
        }
        let weapon = self.weapon(seat).filter(|card| Some(card.id) != ignored);
        let range = match weapon.map(|card| card.kind) {
            Some(CardKind::Cixiong | CardKind::Qinggang) => 2,
            Some(CardKind::Qinglong | CardKind::Zhangba | CardKind::Guanshi) => 3,
            Some(CardKind::Fangtian) => 4,
            Some(CardKind::Qilin) => 5,
            _ => 1,
        };
        self.distance_without(seat, target, ignored)
            .is_ok_and(|distance| distance > 0 && distance <= range)
    }

    pub(crate) fn may_play_sha(&self, seat: SeatId) -> bool {
        self.may_play_sha_without(seat, None)
    }

    pub(crate) fn may_play_sha_without(&self, seat: SeatId, material: Option<CardId>) -> bool {
        !self
            .classic
            .as_ref()
            .expect("the table is classic")
            .sha_played
            || self.hero_unlimited_sha(seat)
            || self
                .weapon(seat)
                .is_some_and(|weapon| weapon.kind == CardKind::Zhuge && Some(weapon.id) != material)
    }

    pub(crate) fn physical_sha_uses(&self, player: &Seat, card: Card) -> Vec<PlannedUse> {
        if !self.is_in(player.id) || !self.may_play_sha(player.id) {
            return Vec::new();
        }
        let targets: Vec<_> = self
            .in_order_from(player.id)
            .into_iter()
            .filter(|&seat| {
                self.in_sha_range(player.id, seat) && self.hero_allows_target(seat, CardKind::Sha)
            })
            .collect();
        let mut selections = Vec::new();
        let extra = self.has_weapon(player.id, CardKind::Fangtian) && player.hand.len() == 1;
        for (index, first) in targets.iter().copied().enumerate() {
            selections.push(vec![first]);
            if extra {
                for (offset, second) in targets[index + 1..].iter().copied().enumerate() {
                    selections.push(vec![first, second]);
                    for third in targets[index + offset + 2..].iter().copied() {
                        selections.push(vec![first, second, third]);
                    }
                }
            }
        }
        selections
            .into_iter()
            .map(|targets| PlannedUse {
                program: self.attack_program(player.id, vec![card], targets.clone()),
                targets,
            })
            .collect()
    }

    pub(crate) fn virtual_plays(&self, seat: SeatId) -> Vec<VirtualPlay> {
        if !self.is_in(seat) {
            return Vec::new();
        }
        let mut plays = self.hero_play_conversions(seat);
        plays.retain(|play| self.card_enabled(seat, play.kind));
        let Ok(actor) = self.seat(seat) else {
            return plays;
        };
        if !self.card_enabled(seat, CardKind::Sha)
            || actor.hand.len() < 2
            || !self.has_weapon(seat, CardKind::Zhangba)
            || !self.may_play_sha(seat)
        {
            return plays;
        }
        let targets: Vec<_> = self
            .in_order_from(seat)
            .into_iter()
            .filter(|&target| {
                self.in_sha_range(seat, target) && self.hero_allows_target(target, CardKind::Sha)
            })
            .collect();
        if targets.is_empty() {
            return plays;
        }
        let mut cost = branch(
            0,
            "丈八蛇矛",
            actor.hand.iter().map(|card| card.id).collect(),
            2,
        );
        cost.targets = targets;
        cost.min_targets = 1;
        cost.max_targets = 1;
        plays.push(VirtualPlay {
            kind: CardKind::Sha,
            branch: cost,
        });
        plays
    }

    pub(crate) fn plan_virtual(
        &self,
        seat: SeatId,
        kind: CardKind,
        ids: Vec<CardId>,
        targets: Vec<SeatId>,
    ) -> Result<Program, RuleError> {
        self.checked_play_seat(seat)?;
        if !self
            .virtual_plays(seat)
            .iter()
            .any(|play| play.kind == kind && Self::matches_branch(&play.branch, &ids, &targets))
        {
            return Err(RuleError::InvalidSelection);
        }
        self.virtual_program(seat, kind, self.hero_cards_by_id(seat, &ids), targets)
    }

    pub(crate) fn virtual_program(
        &self,
        seat: SeatId,
        kind: CardKind,
        cards: Vec<Card>,
        targets: Vec<SeatId>,
    ) -> Result<Program, RuleError> {
        if kind == CardKind::Sha {
            let targets = self
                .in_order_from(seat)
                .into_iter()
                .filter(|target| targets.contains(target))
                .collect();
            Ok(self.attack_program(seat, cards, targets))
        } else {
            self.virtual_trick_program(seat, kind, cards, targets)
        }
    }

    pub(crate) fn hand_cards_by_id(&self, seat: SeatId, ids: &[CardId]) -> Vec<Card> {
        let player = self.seat(seat).expect("the actor exists");
        ids.iter()
            .map(|id| {
                *player
                    .hand
                    .iter()
                    .find(|card| card.id == *id)
                    .expect("the command validated the card")
            })
            .collect()
    }

    pub(crate) fn sha_use_branches(&self, seat: SeatId, targets: &[SeatId]) -> Vec<ChoiceBranch> {
        if !self.card_enabled(seat, CardKind::Sha) {
            return vec![branch(0, "放弃", vec![], 0)];
        }
        self.sha_response_branches(seat, targets)
    }

    pub(crate) fn sha_response_branches(
        &self,
        seat: SeatId,
        targets: &[SeatId],
    ) -> Vec<ChoiceBranch> {
        let mut branches = vec![branch(0, "放弃", vec![], 0)];
        if !self.is_in(seat)
            || !targets
                .iter()
                .all(|&target| self.accepts_sha_target(seat, target))
        {
            return branches;
        }
        let actor = self.seat(seat).expect("the responding seat exists");
        let sha: Vec<_> = actor
            .hand
            .iter()
            .filter(|card| card.kind == CardKind::Sha)
            .map(|card| card.id)
            .collect();
        if !sha.is_empty() {
            branches.push(branch(1, "使用杀", sha, 1));
        }
        if self.has_weapon(seat, CardKind::Zhangba) && actor.hand.len() >= 2 {
            branches.push(branch(
                2,
                "丈八蛇矛",
                actor.hand.iter().map(|card| card.id).collect(),
                2,
            ));
        }
        branches.extend(
            self.hero_response_conversions(seat, CardKind::Sha)
                .into_iter()
                .filter_map(|mut branch| {
                    branch.cards.retain(|card| {
                        targets
                            .iter()
                            .all(|&target| self.in_sha_range_without(seat, target, Some(*card)))
                    });
                    (!branch.cards.is_empty()).then_some(branch)
                }),
        );
        branches
    }

    fn choose_equipment(
        &mut self,
        seat: SeatId,
        target: Option<SeatId>,
        branches: Vec<ChoiceBranch>,
    ) {
        self.scheduler.next(Program::Choose(ChoiceRequest {
            seat,
            target,
            branches,
        }));
    }

    fn continue_equipment(&mut self, program: Flow) {
        self.scheduler.replace_current(Program::Equipment(program));
    }
}
