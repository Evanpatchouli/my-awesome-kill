use super::{branch, slot, Flow, Slot};
use crate::scheduler::{EventRef, Outcome};
use crate::{Card, CardId, CardKind, DrawReason, Game, Program, Resolution, SeatId, Zone};

#[derive(Clone, Copy)]
enum Step {
    Start,
    Decide,
    Opponent,
}

#[derive(Clone, Copy)]
pub(crate) struct Skill {
    pub(super) seat: SeatId,
    pub(super) target: SeatId,
    pub(super) kind: CardKind,
    step: Step,
}

impl Skill {
    pub(super) fn new(seat: SeatId, target: SeatId, kind: CardKind) -> Self {
        Self {
            seat,
            target,
            kind,
            step: Step::Start,
        }
    }
}

impl Game {
    pub(super) fn miss_skill(&self, seat: SeatId, target: SeatId) -> Option<CardKind> {
        match self.weapon(seat)?.kind {
            CardKind::Qinglong if self.sha_use_branches(seat, &[target]).len() > 1 => {
                Some(CardKind::Qinglong)
            }
            CardKind::Guanshi if self.axe_cost_candidates(seat).len() >= 2 => {
                Some(CardKind::Guanshi)
            }
            _ => None,
        }
    }

    pub(super) fn target_horses(&self, target: SeatId) -> Vec<Card> {
        self.seat(target)
            .expect("the target exists")
            .equipment
            .iter()
            .filter(|card| {
                matches!(
                    slot(card.kind),
                    Some(Slot::AttackHorse | Slot::DefenseHorse)
                )
            })
            .copied()
            .collect()
    }

    fn axe_cost_candidates(&self, seat: SeatId) -> Vec<CardId> {
        let player = self.seat(seat).expect("the attacking seat exists");
        player
            .hand
            .iter()
            .chain(
                player
                    .equipment
                    .iter()
                    .filter(|card| card.kind != CardKind::Guanshi),
            )
            .map(|card| card.id)
            .collect()
    }

    pub(super) fn resolve_equipment_skill(
        &mut self,
        _event: EventRef,
        mut skill: Skill,
        child: Option<Outcome<Resolution>>,
    ) {
        let seat = skill.seat;
        let target = skill.target;
        match (skill.step, child) {
            (Step::Start, None) => {
                let branches = match skill.kind {
                    CardKind::Cixiong => vec![
                        branch(0, "放弃", vec![], 0),
                        branch(1, "发动雌雄双股剑", vec![], 0),
                    ],
                    CardKind::Guanshi => vec![
                        branch(0, "放弃", vec![], 0),
                        branch(1, "贯石斧：弃置两张牌", self.axe_cost_candidates(seat), 2),
                    ],
                    CardKind::Qinglong => self.sha_use_branches(seat, &[target]),
                    CardKind::Qilin => vec![
                        branch(0, "放弃", vec![], 0),
                        branch(
                            1,
                            "麒麟弓：弃置一张坐骑",
                            self.target_horses(target)
                                .iter()
                                .map(|card| card.id)
                                .collect(),
                            1,
                        ),
                    ],
                    _ => unreachable!("the equipment skill must have a decision"),
                };
                skill.step = Step::Decide;
                self.continue_equipment(Flow::Skill(skill));
                self.choose_equipment(seat, Some(target), branches);
            }
            (Step::Decide, Some(Outcome::Finished(Resolution::Selected(selection)))) => {
                if selection.branch == 0 {
                    self.finish();
                    return;
                }
                match skill.kind {
                    CardKind::Cixiong => {
                        let cards: Vec<_> = self
                            .seat(target)
                            .expect("the target exists")
                            .hand
                            .iter()
                            .map(|card| card.id)
                            .collect();
                        if cards.is_empty() {
                            self.queue_draw(seat, 1, DrawReason::CardEffect);
                            self.finish();
                        } else {
                            skill.step = Step::Opponent;
                            self.continue_equipment(Flow::Skill(skill));
                            self.choose_equipment(
                                target,
                                Some(seat),
                                vec![
                                    branch(0, "让对方摸一张牌", vec![], 0),
                                    branch(1, "弃置一张手牌", cards, 1),
                                ],
                            );
                        }
                    }
                    CardKind::Guanshi => {
                        let costs = selection
                            .cards
                            .into_iter()
                            .map(|id| {
                                self.hero_card_zone(seat, id)
                                    .expect("the selected cost is held")
                            })
                            .collect();
                        self.queue_discard_batch(seat, costs);
                        self.scheduler.end(Outcome::Finished(Resolution::ForcedHit));
                    }
                    CardKind::Qinglong => {
                        let cards = self.hero_cards_by_id(seat, &selection.cards);
                        self.scheduler
                            .next(self.attack_program(seat, cards, vec![target]));
                        self.finish();
                    }
                    CardKind::Qilin => {
                        self.queue_equipment_discard(target, selection.cards[0]);
                        self.finish();
                    }
                    _ => unreachable!("a skill choice must name an equipment skill"),
                }
            }
            (Step::Opponent, Some(Outcome::Finished(Resolution::Selected(selection)))) => {
                if selection.branch == 0 {
                    self.queue_draw(seat, 1, DrawReason::CardEffect);
                } else {
                    self.queue_equipment_discard(target, selection.cards[0]);
                }
                self.finish();
            }
            _ => unreachable!("equipment skills only resume after their own choices"),
        }
    }

    fn queue_equipment_discard(&mut self, seat: SeatId, id: CardId) {
        let player = self.seat(seat).expect("the cost owner exists");
        let (zone, card) = if let Some(card) = player.hand.iter().find(|card| card.id == id) {
            (Zone::Hand, *card)
        } else {
            let card = player
                .equipment
                .iter()
                .find(|card| card.id == id)
                .expect("the choice admitted this card");
            (Zone::Equipment, *card)
        };
        self.scheduler
            .next(Program::DiscardFrom { seat, zone, card });
    }
}
