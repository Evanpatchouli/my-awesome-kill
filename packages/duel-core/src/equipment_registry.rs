use crate::classic::equipment::slot;
use crate::{Card, CardId, CardKind, SeatId};
use std::ops::Deref;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EquipmentRegistrationKey {
    pub(crate) seat: SeatId,
    pub(crate) serial: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct EquipmentRegistration {
    pub(crate) key: EquipmentRegistrationKey,
    pub(crate) card: Card,
    pub(crate) attached: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct EquipmentArea {
    owner: SeatId,
    physical: Vec<Card>,
    entries: Vec<EquipmentRegistration>,
    next_serial: u64,
    next_hook: u64,
    qinggang_hook: Option<u64>,
    cixiong_hook: Option<u64>,
}
impl EquipmentArea {
    pub(crate) fn hook(&self, skill: &str) -> Option<u64> {
        match skill {
            "qinggang_skill" => self.qinggang_hook,
            "cixiong_skill" => self.cixiong_hook,
            _ => None,
        }
    }
    pub(crate) fn register_hook(&mut self, skill: &str) {
        if self.hook(skill).is_some() || !matches!(skill, "qinggang_skill" | "cixiong_skill") {
            return;
        }
        let serial = self.next_hook;
        self.next_hook = self
            .next_hook
            .checked_add(1)
            .expect("equipment hook serial exhausted");
        match skill {
            "qinggang_skill" => self.qinggang_hook = Some(serial),
            "cixiong_skill" => self.cixiong_hook = Some(serial),
            _ => unreachable!(),
        }
    }
    pub(crate) fn remove_hook(&mut self, skill: &str) {
        match skill {
            "qinggang_skill" => self.qinggang_hook = None,
            "cixiong_skill" => self.cixiong_hook = None,
            _ => {}
        }
    }
    pub(crate) fn as_slice(&self) -> &[Card] {
        &self.physical
    }
    pub(crate) fn new(owner: SeatId) -> Self {
        Self {
            owner,
            physical: Vec::new(),
            entries: Vec::new(),
            next_serial: 0,
            next_hook: 0,
            qinggang_hook: None,
            cixiong_hook: None,
        }
    }
    pub(crate) fn registrations(&self) -> &[EquipmentRegistration] {
        &self.entries
    }
    pub(crate) fn attached(&self, key: EquipmentRegistrationKey) -> Option<Card> {
        self.entries
            .iter()
            .find(|entry| entry.key == key && entry.attached)
            .map(|entry| entry.card)
    }
    pub(crate) fn install(&mut self, card: Card) {
        if let Some(skill) = sha_equipment_skill(card.kind) {
            self.register_hook(skill);
        }
        let key = EquipmentRegistrationKey {
            seat: self.owner,
            serial: self.next_serial,
        };
        self.next_serial = self
            .next_serial
            .checked_add(1)
            .expect("equipment registration serial exhausted");
        let wanted = slot(card.kind);
        let index = self
            .physical
            .iter()
            .position(|old| slot(old.kind) >= wanted)
            .unwrap_or(self.physical.len());
        self.physical.insert(index, card);
        let index = self
            .entries
            .iter()
            .position(|entry| slot(entry.card.kind) > wanted)
            .unwrap_or(self.entries.len());
        self.entries.insert(
            index,
            EquipmentRegistration {
                key,
                card,
                attached: true,
            },
        );
    }
    pub(crate) fn take(&mut self, id: CardId, preserve: bool) -> Card {
        let index = self
            .physical
            .iter()
            .position(|card| card.id == id)
            .expect("validated equipment");
        let card = self.physical.remove(index);
        let index = self
            .entries
            .iter()
            .position(|entry| entry.attached && entry.card.id == id)
            .expect("equipment registration");
        if preserve {
            self.entries[index].attached = false;
        } else {
            self.entries.remove(index);
        }
        card
    }
    #[cfg(test)]
    pub(crate) fn install_test(&mut self, card: Card) {
        self.install(card);
    }
    #[cfg(test)]
    pub(crate) fn set_test(&mut self, cards: Vec<Card>) {
        self.physical.clear();
        self.entries.clear();
        self.qinggang_hook = None;
        self.cixiong_hook = None;
        for &card in &cards {
            self.install(card);
        }
        self.physical = cards;
    }
    #[cfg(test)]
    pub(crate) fn clear_test(&mut self) {
        self.physical.clear();
        self.entries.clear();
        self.qinggang_hook = None;
        self.cixiong_hook = None;
    }
    #[cfg(test)]
    pub(crate) fn detach_test(&mut self, card: Card) {
        self.install(card);
        self.take(card.id, true);
    }
}
impl<'a> IntoIterator for &'a EquipmentArea {
    type Item = &'a Card;
    type IntoIter = std::slice::Iter<'a, Card>;
    fn into_iter(self) -> Self::IntoIter {
        self.physical.iter()
    }
}
impl Deref for EquipmentArea {
    type Target = [Card];
    fn deref(&self) -> &[Card] {
        &self.physical
    }
}
impl<const N: usize> PartialEq<[Card; N]> for EquipmentArea {
    fn eq(&self, other: &[Card; N]) -> bool {
        self.physical == other
    }
}
impl PartialEq<Vec<Card>> for EquipmentArea {
    fn eq(&self, other: &Vec<Card>) -> bool {
        self.physical == *other
    }
}

pub(crate) fn sha_equipment_skill(kind: CardKind) -> Option<&'static str> {
    match kind {
        CardKind::Qinggang => Some("qinggang_skill"),
        CardKind::Cixiong => Some("cixiong_skill"),
        _ => None,
    }
}
