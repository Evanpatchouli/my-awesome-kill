pub(crate) mod attack;
mod response;
mod skill;

use crate::classic::PlannedUse;
use crate::equipment_registry::EquipmentRegistrationKey;
use crate::scheduler::{EventRef, Outcome};
use crate::CounterContext;
use crate::{
    Answer, Card, CardId, CardKind, ChoiceBranch, ChoiceRequest, EquipmentSlot, EquipmentSlotState,
    EventKind, Game, Program, Resolution, RuleError, Seat, SeatId, VirtualPlay, Zone,
};

#[derive(Clone, Copy)]
enum ButtonStage {
    Before,
    Begin,
    Ask,
    End,
    After,
    Finish,
    Cancel,
    FinishCancelled,
}
#[derive(Clone)]
pub(crate) struct ButtonFlow {
    pub(crate) seat: SeatId,
    pub(crate) links: Vec<CardId>,
    choices: Vec<CardId>,
    equipment_options: Option<Vec<(u32, EquipmentRegistrationKey, crate::EquipmentButton)>>,
    physical_keys: Vec<(CardId, EquipmentRegistrationKey)>,
    selected_buttons: Vec<u32>,
    count: usize,
    stage: ButtonStage,
}
impl ButtonFlow {
    pub(crate) fn registration_links(&self) -> &[u32] {
        &self.selected_buttons
    }
    fn new(seat: SeatId, choices: Vec<CardId>, count: usize) -> Self {
        Self {
            seat,
            choices,
            equipment_options: None,
            physical_keys: Vec::new(),
            selected_buttons: Vec::new(),
            count,
            links: Vec::new(),
            stage: ButtonStage::Before,
        }
    }
    fn physical_registrations(
        seat: SeatId,
        cards: &[Card],
        entries: &[crate::equipment_registry::EquipmentRegistration],
        count: usize,
    ) -> Self {
        let mut flow = Self::new(seat, cards.iter().map(|card| card.id).collect(), count);
        flow.physical_keys = cards
            .iter()
            .filter_map(|card| {
                entries
                    .iter()
                    .find(|entry| entry.attached && entry.card.id == card.id)
                    .map(|entry| (card.id, entry.key))
            })
            .collect();
        flow
    }
    fn registrations(
        seat: SeatId,
        entries: &[crate::equipment_registry::EquipmentRegistration],
        count: usize,
    ) -> Self {
        let options = entries
            .iter()
            .enumerate()
            .map(|(id, entry)| {
                let id = id as u32;
                (
                    id,
                    entry.key,
                    crate::EquipmentButton {
                        id,
                        kind: entry.card.kind,
                        card: entry.attached.then_some(entry.card.id),
                    },
                )
            })
            .collect();
        Self {
            seat,
            choices: Vec::new(),
            equipment_options: Some(options),
            physical_keys: Vec::new(),
            selected_buttons: Vec::new(),
            count,
            links: Vec::new(),
            stage: ButtonStage::Before,
        }
    }
}
impl Game {
    pub(crate) fn resolve_choose_button(
        &mut self,
        event: EventRef,
        mut flow: ButtonFlow,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        use ButtonStage::*;
        if matches!(child, Some(Outcome::Cancelled)) && !matches!(flow.stage, FinishCancelled) {
            flow.stage = Cancel;
        }
        loop {
            let (timing, next) = match flow.stage {
                Before => ("chooseButtonBefore", Begin),
                Begin => ("chooseButtonBegin", Ask),
                Ask if !self.is_in(flow.seat) => {
                    flow.stage = Finish;
                    continue;
                }
                Ask => match answer.take() {
                    Some(Answer::Select(selection)) => {
                        flow.links = selection.cards;
                        flow.stage = End;
                        continue;
                    }
                    Some(Answer::SelectEquipmentButtons(buttons)) => {
                        flow.selected_buttons = buttons;
                        flow.stage = End;
                        continue;
                    }
                    None => {
                        self.ask_selection(
                            event,
                            ChoiceRequest {
                                seat: flow.seat,
                                target: None,
                                branches: vec![branch(
                                    0,
                                    "选择装备",
                                    flow.choices.clone(),
                                    flow.count,
                                )],
                            },
                            None,
                        );
                        if let Some(options) = &flow.equipment_options {
                            if let Some(crate::Decision::Cards(choice)) = self.decision.as_mut() {
                                choice.branches.clear();
                                choice.equipment_buttons = Some(crate::EquipmentButtons {
                                    required: flow.count,
                                    options: options
                                        .iter()
                                        .map(|(_, _, option)| option.clone())
                                        .collect(),
                                });
                            }
                        }
                        self.scheduler.replace_current(Program::ChooseButton(flow));
                        return;
                    }
                    _ => unreachable!("a button choice receives a selection"),
                },
                End => ("chooseButtonEnd", After),
                After => ("chooseButtonAfter", Finish),
                Cancel => ("chooseButtonCancelled", FinishCancelled),
                Finish | FinishCancelled => {
                    self.scheduler
                        .end(Outcome::Finished(Resolution::ButtonSelection {
                            cards: flow.links.clone(),
                            keys: flow
                                .equipment_options
                                .as_ref()
                                .map(|options| {
                                    options
                                        .iter()
                                        .filter(|(id, _, _)| flow.selected_buttons.contains(id))
                                        .map(|(_, key, _)| *key)
                                        .collect()
                                })
                                .unwrap_or_else(|| {
                                    flow.physical_keys
                                        .iter()
                                        .filter(|(id, _)| flow.links.contains(id))
                                        .map(|(_, key)| *key)
                                        .collect()
                                }),
                            cancelled: matches!(flow.stage, FinishCancelled),
                        }));
                    return;
                }
            };
            flow.stage = next;
            self.scheduler
                .replace_current(Program::ChooseButton(flow.clone()));
            if self.queue_timing(timing, Some(flow.seat), None, None, 0, &[]) {
                return;
            }
        }
    }
}

pub(crate) type Slot = EquipmentSlot;

pub(crate) fn slot(kind: CardKind) -> Option<Slot> {
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

fn is_mount(slot: Slot) -> bool {
    matches!(slot, Slot::DefenseHorse | Slot::AttackHorse)
}

pub(crate) fn count_slot(seat: &Seat, wanted: Slot, ignored: Option<CardId>) -> usize {
    seat.registered_equipment()
        .filter(|card| Some(card.id) != ignored && slot(card.kind) == Some(wanted))
        .count()
}

impl Seat {
    fn registered_equipment(&self) -> impl Iterator<Item = &Card> {
        self.equipment
            .registrations()
            .iter()
            .map(|entry| &entry.card)
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
    ApplyReplacement,
    Install,
    DirectEnd,
    DirectAfter,
    DirectFinish,
    DirectCancelled,
}

#[derive(Clone, Copy)]
enum ReplacementStep {
    Before,
    Begin,
    Select,
    Selected,
    End,
    After,
    Finish,
    Cancel,
    FinishCancelled,
}

#[derive(Clone)]
pub(crate) struct Replacement {
    seat: SeatId,
    incoming: Card,
    selected: Vec<Card>,
    step: ReplacementStep,
}

#[derive(Clone)]
pub(crate) enum Flow {
    Equip {
        seat: SeatId,
        card: Card,
        step: EquipStep,
        direct: bool,
    },
    Replace(Replacement),
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
            Self::Replace(replacement) => EventKind::ReplaceEquip {
                seat: replacement.seat,
                card_kind: replacement.incoming.kind,
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
            Flow::Replace(replacement) => self.resolve_equip_replacement(replacement, child),
            Flow::Attack(attack) => self.resolve_attack(event, attack, child),
            Flow::Target(target) => self.resolve_sha_target(target, child),
            Flow::Response(response) => self.resolve_response(response, child),
            Flow::Payment(payment) => self.resolve_response_payment(payment, child),
            Flow::Skill(skill) => self.resolve_equipment_skill(event, skill, child),
        }
    }

    pub(crate) fn equipment_uses(&self, player: &Seat, card: Card) -> Option<Vec<PlannedUse>> {
        slot(card.kind)?;
        let seat = player.id;
        if !self.can_equip(seat, card.kind, true) {
            return Some(vec![]);
        }
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
                self.known_to_all.insert(card.id);
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
                self.scheduler
                    .replace_current(program(EquipStep::ApplyReplacement));
                self.scheduler
                    .next(Program::Equipment(Flow::Replace(Replacement {
                        seat,
                        incoming: card,
                        selected: Vec::new(),
                        step: ReplacementStep::Before,
                    })));
            }
            EquipStep::ApplyReplacement => {
                self.scheduler.replace_current(program(EquipStep::Install));
                if let Some(Outcome::Finished(Resolution::EquipReplacement { cards, .. })) = child {
                    let live: Vec<_> = cards
                        .into_iter()
                        .filter_map(|old| self.held_card_location(old.id))
                        .filter(|(holder, _, material)| {
                            *holder == seat && !self.is_virtual_judgment_material(seat, material.id)
                        })
                        .collect();
                    if let [(_, zone, material)] = live.as_slice() {
                        self.scheduler
                            .next(self.equip_replacement_loss_program(seat, *zone, *material));
                    } else if !live.is_empty() {
                        use super::recast::{batch_movement::BatchMovement, Flow as RecastFlow};
                        let cards = live
                            .into_iter()
                            .map(|(_, _, card)| card)
                            .collect::<Vec<_>>();
                        self.scheduler
                            .next(Program::Recast(RecastFlow::BatchMovement(
                                BatchMovement::equipment_replacement_loss(seat, cards.into()),
                            )));
                    }
                }
            }
            EquipStep::Install => {
                let occupied = !self.can_equip(seat, card.kind, false);
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
                            self.install_equipment(seat, taken);
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
                self.install_equipment(seat, card);
                self.finish();
            }
            EquipStep::DirectFinish => self.finish(),
            EquipStep::DirectCancelled => self.scheduler.end(Outcome::Cancelled),
        }
    }

    fn resolve_equip_replacement(
        &mut self,
        mut replacement: Replacement,
        child: Option<Outcome<Resolution>>,
    ) {
        use ReplacementStep::*;
        if matches!(child, Some(Outcome::Cancelled)) && !matches!(replacement.step, FinishCancelled)
        {
            replacement.step = Cancel;
        }
        loop {
            let (timing, next) = match replacement.step {
                Before => ("replaceEquipBefore", Begin),
                Begin => ("replaceEquipBegin", Select),
                Select => {
                    if self.is_in(replacement.seat)
                        && self.can_equip(replacement.seat, replacement.incoming.kind, true)
                        && !self.can_equip(replacement.seat, replacement.incoming.kind, false)
                    {
                        let player = self
                            .seat(replacement.seat)
                            .expect("the equipping seat exists");
                        let wanted = slot(replacement.incoming.kind);
                        let combined = self.mount_slots == crate::MountSlots::Combined
                            && wanted.is_some_and(is_mount);
                        let entries: Vec<_> = player
                            .equipment
                            .registrations()
                            .iter()
                            .filter(|entry| {
                                slot(entry.card.kind) == wanted
                                    || (combined && slot(entry.card.kind).is_some_and(is_mount))
                            })
                            .cloned()
                            .collect();
                        let required =
                            entries.len().saturating_add(1).saturating_sub(if combined {
                                self.equipment_slot_state(replacement.seat, Slot::DefenseHorse)
                                    .equipable
                            } else {
                                self.equipment_slot_state(
                                    replacement.seat,
                                    wanted.expect("equipment slot"),
                                )
                                .equipable
                            });
                        if required >= entries.len() {
                            replacement.selected = player
                                .equipment
                                .iter()
                                .copied()
                                .filter(|card| {
                                    entries
                                        .iter()
                                        .any(|entry| entry.attached && entry.card == *card)
                                })
                                .collect();
                            replacement.step = End;
                            continue;
                        }
                        if entries.iter().any(|entry| !entry.attached) {
                            replacement.step = Selected;
                            self.scheduler
                                .next(Program::ChooseButton(ButtonFlow::registrations(
                                    replacement.seat,
                                    &entries,
                                    required,
                                )));
                            self.scheduler
                                .replace_current(Program::Equipment(Flow::Replace(replacement)));
                            return;
                        }
                        let choices: Vec<_> = entries
                            .iter()
                            .filter(|entry| entry.attached)
                            .map(|entry| entry.card)
                            .collect();
                        if choices.len() > 1 {
                            replacement.step = Selected;
                            self.scheduler.next(Program::ChooseButton(
                                ButtonFlow::physical_registrations(
                                    replacement.seat,
                                    &choices,
                                    &entries,
                                    required,
                                ),
                            ));
                            self.scheduler
                                .replace_current(Program::Equipment(Flow::Replace(replacement)));
                            return;
                        }
                        replacement.selected = choices;
                    }
                    replacement.step = End;
                    continue;
                }
                Selected => {
                    if let Some(Outcome::Finished(Resolution::ButtonSelection {
                        cards,
                        keys,
                        ..
                    })) = child.as_ref()
                    {
                        replacement.selected = if !keys.is_empty() {
                            self.seat(replacement.seat)
                                .ok()
                                .map(|player| {
                                    player
                                        .equipment
                                        .iter()
                                        .copied()
                                        .filter(|card| {
                                            keys.iter().any(|&key| {
                                                player.equipment.attached(key) == Some(*card)
                                            })
                                        })
                                        .collect()
                                })
                                .unwrap_or_default()
                        } else {
                            self.seat(replacement.seat)
                                .ok()
                                .map(|player| {
                                    player
                                        .equipment
                                        .iter()
                                        .copied()
                                        .filter(|card| {
                                            cards.contains(&card.id)
                                                && (slot(card.kind)
                                                    == slot(replacement.incoming.kind)
                                                    || (self.mount_slots
                                                        == crate::MountSlots::Combined
                                                        && slot(replacement.incoming.kind)
                                                            .is_some_and(is_mount)
                                                        && slot(card.kind).is_some_and(is_mount)))
                                        })
                                        .collect()
                                })
                                .unwrap_or_default()
                        };
                    }
                    replacement.step = End;
                    continue;
                }
                End => ("replaceEquipEnd", After),
                After => ("replaceEquipAfter", Finish),
                Cancel => ("replaceEquipCancelled", FinishCancelled),
                Finish | FinishCancelled => {
                    self.scheduler
                        .end(Outcome::Finished(Resolution::EquipReplacement {
                            cards: replacement.selected,
                            cancelled: matches!(replacement.step, FinishCancelled),
                        }));
                    return;
                }
            };
            replacement.step = next;
            if self.queue_timing(timing, Some(replacement.seat), None, None, 1, &[]) {
                self.scheduler
                    .replace_current(Program::Equipment(Flow::Replace(replacement)));
                return;
            }
        }
    }

    fn install_equipment(&mut self, seat: SeatId, card: Card) {
        self.seat_mut(seat).equipment.install(card);
    }

    pub(crate) fn weapon(&self, seat: SeatId) -> Option<Card> {
        self.seat(seat)
            .ok()?
            .registered_equipment()
            .find(|card| slot(card.kind) == Some(Slot::Weapon))
            .copied()
    }

    fn weapons_without(
        &self,
        seat: SeatId,
        ignored: Option<CardId>,
    ) -> impl Iterator<Item = Card> + '_ {
        self.seat(seat)
            .ok()
            .into_iter()
            .flat_map(|player| player.equipment.registrations().iter())
            .filter(move |entry| {
                slot(entry.card.kind) == Some(Slot::Weapon) && Some(entry.card.id) != ignored
            })
            .map(|entry| entry.card)
    }

    fn has_weapon(&self, seat: SeatId, kind: CardKind) -> bool {
        self.weapons_without(seat, None)
            .any(|card| card.kind == kind)
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
        let range = self
            .weapons_without(seat, ignored)
            .map(|weapon| match weapon.kind {
                CardKind::Cixiong | CardKind::Qinggang => 2,
                CardKind::Qinglong | CardKind::Zhangba | CardKind::Guanshi => 3,
                CardKind::Fangtian => 4,
                CardKind::Qilin => 5,
                _ => 1,
            })
            .max()
            .unwrap_or(1);
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
            || self.seat(seat).is_ok_and(|player| {
                if !self.seat_has_skill(player, "zhuge_skill") {
                    return false;
                }
                let mut equipped = player
                    .equipment
                    .iter()
                    .filter(|card| card.kind == CardKind::Zhuge)
                    .peekable();
                self.seat_has_non_equipment_skill(player, "zhuge_skill")
                    || equipped.peek().is_none()
                    || equipped.any(|card| Some(card.id) != material)
            })
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

const SLOTS: [EquipmentSlot; 5] = [
    EquipmentSlot::Weapon,
    EquipmentSlot::Armor,
    EquipmentSlot::DefenseHorse,
    EquipmentSlot::AttackHorse,
    EquipmentSlot::Treasure,
];
fn slot_index(slot: EquipmentSlot) -> usize {
    SLOTS.iter().position(|&s| s == slot).unwrap()
}
pub(crate) fn markers(seat: &Seat) -> Vec<EquipmentSlot> {
    SLOTS
        .iter()
        .flat_map(|&slot| std::iter::repeat(slot).take(seat.equipment_markers[slot_index(slot)]))
        .collect()
}

#[derive(Clone, Copy)]
enum AreaStage {
    Before,
    Begin,
    Apply,
    Select,
    Selected,
    Publish,
    End,
    After,
    Finish,
    Cancel,
    FinishCancelled,
}
#[derive(Clone)]
pub(crate) struct AreaFlow {
    seat: SeatId,
    source: Option<SeatId>,
    slots: Vec<EquipmentSlot>,
    cards: Vec<Card>,
    candidates: Vec<Card>,
    index: usize,
    op: AreaOp,
    stage: AreaStage,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AreaOp {
    Disable,
    Enable,
    Expand,
}
impl AreaFlow {
    pub(crate) fn slots(&self) -> &[EquipmentSlot] {
        &self.slots
    }
    pub(crate) fn new(
        seat: SeatId,
        source: Option<SeatId>,
        slots: Vec<EquipmentSlot>,
        op: AreaOp,
    ) -> Self {
        Self {
            seat,
            source,
            slots,
            cards: Vec::new(),
            candidates: Vec::new(),
            index: 0,
            op,
            stage: AreaStage::Before,
        }
    }
    pub(crate) fn kind(&self) -> EventKind {
        match self.op {
            AreaOp::Disable => EventKind::DisableEquip {
                seat: self.seat,
                source: self.source,
            },
            AreaOp::Enable => EventKind::EnableEquip {
                seat: self.seat,
                source: self.source,
            },
            AreaOp::Expand => EventKind::ExpandEquip {
                seat: self.seat,
                source: self.source,
            },
        }
    }
}
impl Game {
    pub fn equipment_slot_state(&self, seat: SeatId, slot: EquipmentSlot) -> EquipmentSlotState {
        let Ok(player) = self.seat(seat) else {
            return EquipmentSlotState::default();
        };
        if self.mount_slots == crate::MountSlots::Combined && slot == Slot::AttackHorse {
            return EquipmentSlotState::default();
        }
        let i = slot_index(slot);
        let enabled = 1 + player.equipment_expanded[i] as i64 - player.equipment_disabled[i] as i64;
        let registered = player
            .registered_equipment()
            .filter(|card| self::slot(card.kind) == Some(slot))
            .count();
        EquipmentSlotState {
            enabled,
            empty: (enabled.max(0) as usize).saturating_sub(registered),
            equipable: enabled.max(0) as usize,
            disabled: player.equipment_disabled[i],
        }
    }
    pub fn combined_mount_slot_state(&self, seat: SeatId) -> EquipmentSlotState {
        if self.seat(seat).is_err() {
            return EquipmentSlotState::default();
        }
        let canonical = self.equipment_slot_state(seat, Slot::DefenseHorse);
        EquipmentSlotState {
            enabled: 1,
            disabled: 0,
            empty: canonical.empty,
            equipable: canonical.equipable,
        }
    }
    pub fn can_equip(&self, seat: SeatId, kind: CardKind, replace: bool) -> bool {
        let Some(slot) = slot(kind) else { return false };
        if !self.is_in(seat) {
            return false;
        }
        if self.mount_slots == crate::MountSlots::Combined && is_mount(slot) {
            let state = self.equipment_slot_state(seat, Slot::DefenseHorse);
            let occupied = self
                .seat(seat)
                .unwrap()
                .registered_equipment()
                .filter(|card| self::slot(card.kind).is_some_and(is_mount))
                .count();
            return if replace {
                state.equipable > 0
            } else {
                state.equipable > occupied
            };
        }
        let state = self.equipment_slot_state(seat, slot);
        if replace {
            state.equipable > 0
        } else {
            state.empty > 0
        }
    }
    pub(crate) fn resolve_equip_area(
        &mut self,
        mut flow: AreaFlow,
        child: Option<Outcome<Resolution>>,
    ) {
        use AreaStage::*;
        if matches!(child, Some(Outcome::Cancelled)) && !matches!(flow.stage, End | FinishCancelled)
        {
            flow.stage = Cancel;
        }
        loop {
            let (disable, enable, expand, next) = match flow.stage {
                Before => (
                    "disableEquipBefore",
                    "enableEquipBefore",
                    "expandEquipBefore",
                    Begin,
                ),
                Begin => (
                    "disableEquipBegin",
                    "enableEquipBegin",
                    "expandEquipBegin",
                    Apply,
                ),
                Apply => {
                    if !self.is_in(flow.seat) {
                        flow.stage = End;
                        continue;
                    }
                    if flow.op == AreaOp::Disable {
                        flow.stage = Select;
                        continue;
                    }
                    for (i, &slot) in SLOTS.iter().enumerate() {
                        let combined = self.mount_slots == crate::MountSlots::Combined;
                        if combined && slot == Slot::AttackHorse {
                            continue;
                        }
                        let count = if combined && slot == Slot::DefenseHorse {
                            flow.slots
                                .iter()
                                .filter(|&&s| s == Slot::DefenseHorse)
                                .count()
                                .max(
                                    flow.slots
                                        .iter()
                                        .filter(|&&s| s == Slot::AttackHorse)
                                        .count(),
                                )
                        } else {
                            flow.slots.iter().filter(|&&s| s == slot).count()
                        };
                        let player = self.seat_mut(flow.seat);
                        if flow.op == AreaOp::Expand {
                            player.equipment_expanded[i] += count;
                        } else if flow.op == AreaOp::Enable {
                            let restored = count.min(player.equipment_disabled[i]);
                            player.equipment_disabled[i] -= restored;
                            player.equipment_markers[i] =
                                player.equipment_markers[i].saturating_sub(restored);
                        }
                    }
                    flow.stage = End;
                    continue;
                }
                Select => {
                    if flow.index == SLOTS.len() {
                        flow.stage = Publish;
                        continue;
                    }
                    let i = flow.index;
                    flow.index += 1;
                    let combined = self.mount_slots == crate::MountSlots::Combined;
                    if combined && SLOTS[i] == Slot::AttackHorse {
                        continue;
                    }
                    let count = if combined && SLOTS[i] == Slot::DefenseHorse {
                        flow.slots
                            .iter()
                            .filter(|&&s| s == Slot::DefenseHorse)
                            .count()
                            .max(
                                flow.slots
                                    .iter()
                                    .filter(|&&s| s == Slot::AttackHorse)
                                    .count(),
                            )
                    } else {
                        flow.slots.iter().filter(|&&slot| slot == SLOTS[i]).count()
                    };
                    if count == 0 {
                        continue;
                    }
                    let player = self.seat_mut(flow.seat);
                    if combined && SLOTS[i] == Slot::DefenseHorse {
                        player.equipment_disabled[i] += 1;
                        flow.cards.extend(
                            player
                                .equipment
                                .iter()
                                .copied()
                                .filter(|card| slot(card.kind).is_some_and(is_mount)),
                        );
                        continue;
                    }
                    let enabled = (1 + player.equipment_expanded[i])
                        .saturating_sub(player.equipment_disabled[i]);
                    let change = count.min(enabled);
                    if change == 0 {
                        continue;
                    }
                    player.equipment_disabled[i] += change;
                    let wanted = SLOTS[i];
                    let capacity =
                        self.equipment_slot_state(flow.seat, wanted).enabled.max(0) as usize;
                    let cards: Vec<_> = self
                        .seat(flow.seat)
                        .unwrap()
                        .equipment
                        .iter()
                        .copied()
                        .filter(|card| slot(card.kind) == Some(wanted))
                        .collect();
                    let excess = cards.len().saturating_sub(capacity);
                    if excess == 0 {
                        continue;
                    }
                    if capacity == 0 {
                        flow.cards.extend(cards);
                        continue;
                    }
                    let chooser = flow
                        .source
                        .filter(|&seat| self.is_in(seat))
                        .unwrap_or(flow.seat);
                    flow.candidates = cards.clone();
                    flow.stage = Selected;
                    self.scheduler.next(Program::ChooseButton(ButtonFlow::new(
                        chooser,
                        cards.iter().map(|card| card.id).collect(),
                        excess,
                    )));
                    self.scheduler.replace_current(Program::EquipArea(flow));
                    return;
                }
                Selected => {
                    if let Some(Outcome::Finished(Resolution::ButtonSelection { cards, .. })) =
                        child.as_ref()
                    {
                        for &id in cards {
                            if let Some(card) =
                                flow.candidates.iter().find(|card| card.id == id).copied()
                            {
                                flow.cards.push(card);
                            }
                        }
                    }
                    flow.stage = Select;
                    continue;
                }
                Publish => {
                    let player = self.seat_mut(flow.seat);
                    player.equipment_markers = player.equipment_disabled;
                    flow.stage = End;
                    if !flow.cards.is_empty() {
                        let cards = flow.cards.clone();
                        use super::recast::{batch_movement::BatchMovement, Flow as RecastFlow};
                        self.scheduler
                            .next(Program::Recast(RecastFlow::BatchMovement(
                                BatchMovement::equipment_discard(flow.seat, cards.into()),
                            )));
                        self.scheduler.replace_current(Program::EquipArea(flow));
                        return;
                    }
                    continue;
                }
                End => ("disableEquipEnd", "enableEquipEnd", "expandEquipEnd", After),
                After => (
                    "disableEquipAfter",
                    "enableEquipAfter",
                    "expandEquipAfter",
                    Finish,
                ),
                Cancel => (
                    "disableEquipCancelled",
                    "enableEquipCancelled",
                    "expandEquipCancelled",
                    FinishCancelled,
                ),
                Finish => {
                    self.finish();
                    return;
                }
                FinishCancelled => {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
            };
            flow.stage = next;
            if self.queue_timing(
                match flow.op {
                    AreaOp::Disable => disable,
                    AreaOp::Enable => enable,
                    AreaOp::Expand => expand,
                },
                Some(flow.seat),
                flow.source,
                None,
                0,
                &flow.cards,
            ) {
                self.scheduler.replace_current(Program::EquipArea(flow));
                return;
            }
        }
    }
}
