use crate::{Card, CardId, CardKind, Game, JudgmentAssignment, JudgmentEntryView, SeatId, Zone};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Identifies an active judgment for trusted rules.
pub enum JudgmentKey {
    /// The physical card is also the judgment.
    Native(CardId),
    /// A judgment with an identity independent of its original material.
    Virtual(u64),
}

#[derive(Clone, Copy, Debug)]
/// A snapshot of an active judgment for trusted rules.
pub struct Judgment {
    /// Identifies the logical judgment during its lifetime.
    pub key: JudgmentKey,
    /// The original physical card, which may already be in a different zone or seat.
    pub material: Card,
    /// The effective delayed-trick identity.
    pub kind: CardKind,
    attached: bool,
}

impl Judgment {
    fn native(card: Card) -> Self {
        Self {
            key: JudgmentKey::Native(card.id),
            material: card,
            kind: card.kind,
            attached: true,
        }
    }

    pub fn is_virtual(self) -> bool {
        matches!(self.key, JudgmentKey::Virtual(_))
    }

    /// Whether the original material still occupies this judgment's slot.
    pub fn is_attached(self) -> bool {
        self.attached
    }
}

#[derive(Default)]
pub(crate) struct Registry {
    entries: Vec<(SeatId, Judgment)>,
    next_virtual_id: u64,
}

impl Registry {
    fn place(&mut self, seat: SeatId, card: Card, kind: CardKind) {
        let mut entry = Judgment::native(card);
        if card.kind != kind {
            entry.key = JudgmentKey::Virtual(self.next_virtual_id);
            entry.kind = kind;
            self.next_virtual_id += 1;
        }
        self.entries.push((seat, entry));
    }

    pub(crate) fn material_left(&mut self, seat: SeatId, id: CardId) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|&(owner, entry)| owner == seat && entry.attached && entry.material.id == id)
        {
            let entry = &mut self.entries[index].1;
            if entry.is_virtual() {
                entry.attached = false;
            } else {
                self.entries.remove(index);
            }
        }
    }
}

impl Game {
    /// Active judgments in placement order, in a snapshot for trusted rules.
    pub fn judgments(&self, seat: SeatId) -> impl DoubleEndedIterator<Item = Judgment> {
        let judgments = self
            .judgment_registry
            .entries
            .iter()
            .filter_map(|&(owner, entry)| (owner == seat).then_some(entry))
            .collect::<Vec<_>>();
        #[cfg(test)]
        let judgments = {
            let mut judgments = judgments;
            if let Ok(player) = self.seat(seat) {
                // Some setup fixtures directly place native delayed cards into the physical zone.
                for &card in &player.judges {
                    if !judgments
                        .iter()
                        .any(|entry| entry.attached && entry.material == card)
                    {
                        judgments.push(Judgment::native(card));
                    }
                }
            }
            judgments
        };
        judgments.into_iter()
    }

    pub(crate) fn contains_judgment(&self, seat: SeatId, entry: Judgment) -> bool {
        self.judgment_entry(seat, entry.key).is_some()
    }

    pub(crate) fn can_discard_judgment_snapshot(&self, seat: SeatId, entry: Judgment) -> bool {
        if entry.is_virtual() {
            self.contains_judgment(seat, entry)
        } else {
            self.held_card_location(entry.material.id)
                .is_some_and(|(owner, _, _)| owner == seat)
        }
    }

    pub(crate) fn take_judgment_for_discard(
        &mut self,
        seat: SeatId,
        entry: Judgment,
    ) -> Option<(Zone, Card)> {
        if entry.is_virtual() {
            self.take_judgment(seat, entry)
                .map(|card| (Zone::Judgment, card))
        } else {
            let (owner, zone, card) = self.held_card_location(entry.material.id)?;
            (owner == seat).then(|| (zone, self.take_card(seat, zone, card.id)))
        }
    }

    pub(crate) fn judgment_entry(&self, seat: SeatId, key: JudgmentKey) -> Option<Judgment> {
        self.judgments(seat).find(|entry| entry.key == key)
    }

    pub(crate) fn is_virtual_judgment_material(&self, seat: SeatId, id: CardId) -> bool {
        self.judgments(seat)
            .any(|entry| entry.attached && entry.material.id == id && entry.is_virtual())
    }

    pub(crate) fn judgment_assignments(&self, seat: SeatId) -> Vec<JudgmentAssignment> {
        self.judgment_registry
            .entries
            .iter()
            .filter_map(|&(owner, entry)| {
                (owner == seat && entry.attached && entry.is_virtual()).then_some(
                    JudgmentAssignment {
                        id: entry.material.id,
                        kind: entry.kind,
                    },
                )
            })
            .collect()
    }

    pub(crate) fn logical_judgment_view(&self, seat: SeatId) -> Vec<JudgmentEntryView> {
        if !self
            .judgment_registry
            .entries
            .iter()
            .any(|&(owner, entry)| owner == seat && !entry.attached)
        {
            return Vec::new();
        }
        self.judgments(seat)
            .map(|entry| JudgmentEntryView {
                kind: entry.kind,
                material: entry.attached.then_some(entry.material.id),
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn judgment_kind(&self, card: Card) -> CardKind {
        self.held_card_location(card.id)
            .and_then(|(seat, _, _)| {
                self.judgments(seat)
                    .find(|entry| entry.attached && entry.material == card)
                    .map(|entry| entry.kind)
            })
            .unwrap_or(card.kind)
    }

    pub(crate) fn put_in_judgment(&mut self, seat: SeatId, card: Card, kind: CardKind) {
        self.judgment_registry.place(seat, card, kind);
        self.seat_mut(seat).judges.push(card);
    }

    pub(crate) fn take_judgment(&mut self, seat: SeatId, entry: Judgment) -> Option<Card> {
        if !entry.is_virtual() {
            return self
                .seat(seat)
                .is_ok_and(|player| player.judges.contains(&entry.material))
                .then(|| self.take_card(seat, Zone::Judgment, entry.material.id));
        }
        let index = self
            .judgment_registry
            .entries
            .iter()
            .position(|&(owner, current)| owner == seat && current.key == entry.key)?;
        self.judgment_registry.entries.remove(index);
        if let Some((owner, zone, _)) = self.held_card_location(entry.material.id) {
            let material = if zone == Zone::Hand {
                // The loss was of the public judgment. Knowledge of a separate hand
                // card changes only when an effect selects it directly from that hand.
                Self::remove_card(&mut self.seat_mut(owner).hand, entry.material.id)
            } else {
                self.take_card(owner, zone, entry.material.id)
            };
            if zone == Zone::Equipment {
                // Consuming the judgment selects its wrapper, leaving a distinct equipment registration.
                self.seat_mut(owner).detached_equipment.push(material);
            }
            Some(material)
        } else {
            self.take_unowned_card(entry.material)
        }
    }
}
