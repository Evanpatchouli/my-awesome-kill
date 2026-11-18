use crate::{CardId, Game, SeatId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardZone {
    Hand,
    Equipment,
    Judgment,
    Expansion,
    DrawPile,
    Ordering,
    Special,
    DiscardPile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardLocation {
    /// A newly drawn judgment card can already have an owner before entering its physical zone.
    pub zone: Option<CardZone>,
    pub owner: Option<SeatId>,
}

impl Game {
    /// A hidden hand loss obscures old public knowledge of the remaining cards.
    pub(crate) fn obscure_remaining_hand(&mut self, owner: SeatId) {
        if !self.known_to_all.is_empty() {
            let player = self
                .seats
                .iter()
                .find(|p| p.id == owner)
                .expect("validated payer");
            for held in &player.hand {
                self.known_to_all.remove(&held.id);
            }
        }
    }

    /// Locate an actual physical card for trusted rules. Seat views handle disclosure separately.
    pub fn card_location(&self, id: CardId) -> Option<CardLocation> {
        for seat in &self.seats {
            for (zone, cards) in [
                (CardZone::Hand, &seat.hand),
                (CardZone::Equipment, &seat.equipment),
                (CardZone::Judgment, &seat.judges),
                (CardZone::Expansion, &seat.expansions),
            ] {
                if cards.iter().any(|card| card.id == id) {
                    return Some(CardLocation {
                        zone: Some(zone),
                        owner: Some(seat.id),
                    });
                }
            }
        }
        let owner = self.judging_owner(id);
        let zone = [
            (CardZone::Ordering, self.resolving.as_slice()),
            (CardZone::Special, self.transferring.as_slice()),
            (CardZone::DiscardPile, self.discard.as_slice()),
            (
                CardZone::DrawPile,
                self.classic
                    .as_ref()
                    .map_or(&[][..], |table| table.deck.as_slice()),
            ),
        ]
        .into_iter()
        .find_map(|(zone, cards)| cards.iter().any(|card| card.id == id).then_some(zone));
        (zone.is_some() || owner.is_some()).then_some(CardLocation { zone, owner })
    }
}
