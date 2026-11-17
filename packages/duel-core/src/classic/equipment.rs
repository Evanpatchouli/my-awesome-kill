mod attack;
mod response;
mod skill;

use crate::classic::PlannedUse;
use crate::scheduler::{EventRef, Outcome};
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

pub(crate) fn has_slot(seat: &Seat, wanted: Slot) -> bool {
    seat.equipment
        .iter()
        .any(|card| slot(card.kind) == Some(wanted))
}

#[derive(Clone)]
pub(crate) enum Flow {
    Equip { seat: SeatId, card: Card },
    Attack(attack::Attack),
    Target(attack::Target),
    ShaDamage(attack::ShaDamage),
    Response(response::Response),
    Payment(response::Payment),
    Skill(skill::Skill),
}

impl Flow {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::Equip { seat, card } => EventKind::Equip {
                seat: *seat,
                card: *card,
            },
            Self::Attack(attack) => attack.event(),
            Self::Target(attack) => EventKind::ShaEffect {
                seat: attack.seat,
                target: attack.target,
            },
            Self::ShaDamage(damage) => EventKind::Damage {
                source: Some(damage.seat),
                target: damage.target,
                amount: 1,
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

    pub(crate) fn response(seat: SeatId, kind: CardKind, ignore_armor: bool) -> Self {
        Self::Response(response::Response::new(seat, kind, ignore_armor))
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
    pub(crate) fn resolve_equipment(
        &mut self,
        event: EventRef,
        flow: Flow,
        child: Option<Outcome<Resolution>>,
    ) {
        match flow {
            Flow::Equip { seat, card } => self.resolve_equip(seat, card, child.is_none()),
            Flow::Attack(attack) => self.resolve_attack(attack),
            Flow::Target(target) => self.resolve_sha_target(target, child),
            Flow::ShaDamage(damage) => self.resolve_sha_damage(damage, child),
            Flow::Response(response) => self.resolve_response(response, child),
            Flow::Payment(payment) => self.resolve_response_payment(payment),
            Flow::Skill(skill) => self.resolve_equipment_skill(event, skill, child),
        }
    }

    pub(crate) fn equipment_uses(&self, player: &Seat, card: Card) -> Option<Vec<PlannedUse>> {
        slot(card.kind)?;
        let seat = player.id;
        Some(vec![PlannedUse {
            targets: vec![seat],
            program: Program::Equipment(Flow::Equip { seat, card }),
        }])
    }

    fn resolve_equip(&mut self, seat: SeatId, card: Card, first: bool) {
        if first {
            self.move_to_resolving(seat, card.id);
            let current = self.seat(seat).expect("the equipping seat exists");
            let old = current
                .equipment
                .iter()
                .find(|old| slot(old.kind) == slot(card.kind))
                .copied();
            if let Some(old) = old {
                self.scheduler.next(Program::DiscardFrom {
                    seat,
                    zone: Zone::Equipment,
                    card: old,
                });
                return;
            }
        }
        let card = self.take_resolving(card.id);
        self.seat_mut(seat).equipment.push(card);
        self.finish();
    }

    pub(crate) fn weapon(&self, seat: SeatId) -> Option<Card> {
        self.seat(seat)
            .ok()?
            .equipment
            .iter()
            .find(|card| slot(card.kind) == Some(Slot::Weapon))
            .copied()
    }

    fn has_weapon(&self, seat: SeatId, kind: CardKind) -> bool {
        self.weapon(seat).is_some_and(|card| card.kind == kind)
    }

    pub(crate) fn in_sha_range(&self, seat: SeatId, target: SeatId) -> bool {
        let range = match self.weapon(seat).map(|card| card.kind) {
            Some(CardKind::Cixiong | CardKind::Qinggang) => 2,
            Some(CardKind::Qinglong | CardKind::Zhangba | CardKind::Guanshi) => 3,
            Some(CardKind::Fangtian) => 4,
            Some(CardKind::Qilin) => 5,
            _ => 1,
        };
        self.distance(seat, target)
            .is_ok_and(|distance| distance > 0 && distance <= range)
    }

    pub(crate) fn may_play_sha(&self, seat: SeatId) -> bool {
        !self
            .classic
            .as_ref()
            .expect("the table is classic")
            .sha_played
            || self.has_weapon(seat, CardKind::Zhuge)
    }

    pub(crate) fn physical_sha_uses(&self, player: &Seat, card: Card) -> Vec<PlannedUse> {
        if !self.may_play_sha(player.id) {
            return Vec::new();
        }
        let targets: Vec<_> = self
            .living_order_from(player.id)
            .into_iter()
            .filter(|&seat| self.in_sha_range(player.id, seat))
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
        let Ok(actor) = self.seat(seat) else {
            return vec![];
        };
        if actor.hand.len() < 2
            || !self.has_weapon(seat, CardKind::Zhangba)
            || !self.may_play_sha(seat)
        {
            return vec![];
        }
        let targets: Vec<_> = self
            .living_order_from(seat)
            .into_iter()
            .filter(|&target| self.in_sha_range(seat, target))
            .collect();
        if targets.is_empty() {
            return vec![];
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
        vec![VirtualPlay {
            kind: CardKind::Sha,
            branch: cost,
        }]
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
        Ok(self.attack_program(seat, self.hand_cards_by_id(seat, &ids), targets))
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
        let mut branches = vec![branch(0, "放弃", vec![], 0)];
        if !targets
            .iter()
            .all(|&target| self.in_sha_range(seat, target))
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
