use super::{branch, SkillId};
use crate::classic_data::{Role, Suit};
use crate::{Card, CardId, CardKind, ChoiceBranch, Game, Phase, Seat, SeatId, VirtualPlay};

impl Game {
    pub(crate) fn has_lord_skill(&self, seat: SeatId, skill: &str) -> bool {
        self.has_skill(seat, skill)
            && self
                .seat(seat)
                .is_ok_and(|player| player.role == Some(Role::Lord))
    }

    pub(crate) fn hero_distance_modifier(&self, from: &Seat) -> i32 {
        -i32::from(self.seat_has_skill(from, SkillId::Mashu.as_str()))
    }

    pub(crate) fn hero_allows_target(&self, target: SeatId, kind: CardKind) -> bool {
        use CardKind::*;
        if matches!(kind, Shunshou | Lebu) && self.has_skill(target, SkillId::Qianxun.as_str()) {
            return false;
        }
        !matches!(kind, Sha | Juedou)
            || !self.has_skill(target, SkillId::Kongcheng.as_str())
            || self
                .seat(target)
                .is_ok_and(|player| !player.hand.is_empty())
    }

    pub(crate) fn hero_trick_unlimited_range(&self, seat: SeatId) -> bool {
        self.has_skill(seat, SkillId::Qicai.as_str())
    }

    pub(crate) fn hero_unlimited_sha(&self, seat: SeatId) -> bool {
        self.has_skill(seat, SkillId::Paoxiao.as_str())
    }

    pub(crate) fn hero_response_count(&self, opponent: SeatId, kind: CardKind) -> u8 {
        1 + u8::from(
            matches!(kind, CardKind::Sha | CardKind::Shan)
                && self.has_skill(opponent, SkillId::Wushuang.as_str()),
        )
    }

    pub(crate) fn hero_tao_bonus(&self, seat: SeatId, target: SeatId) -> u16 {
        u16::from(
            seat != target
                && self.has_lord_skill(target, SkillId::Jiuyuan.as_str())
                && self.hero_faction_is(seat, "wu"),
        )
    }

    pub(crate) fn note_hero_sha_play(&mut self, seat: SeatId) {
        if let Some(state) = &mut self.classic {
            if state
                .turn
                .is_some_and(|turn| turn.seat == seat && turn.phase == Phase::Play)
            {
                state.hero_turn.played_sha = true;
            }
        }
    }

    pub(super) fn hero_faction_is(&self, seat: SeatId, faction: &str) -> bool {
        self.seat(seat)
            .ok()
            .and_then(|seat| self.general(seat))
            .is_some_and(|hero| hero.faction == faction)
    }

    pub(super) fn hero_card_is_red(&self, card: Card) -> bool {
        self.face(card.id)
            .is_some_and(|face| matches!(face.suit, Suit::Heart | Suit::Diamond))
    }

    pub(super) fn hero_card_is_black(&self, card: Card) -> bool {
        self.face(card.id)
            .is_some_and(|face| matches!(face.suit, Suit::Spade | Suit::Club))
    }

    fn hero_conversion_costs(
        &self,
        seat: SeatId,
        kind: CardKind,
        responding: bool,
    ) -> Vec<ChoiceBranch> {
        use CardKind::*;
        let Ok(player) = self.seat(seat) else {
            return Vec::new();
        };
        let hand = player.hand.iter();
        let owned = player.hand.iter().chain(&player.equipment);
        let mut branches = Vec::new();
        let mut conversion = |id, skill, name, cards: Vec<CardId>| {
            if self.has_skill(seat, skill) && !cards.is_empty() {
                branches.push(branch(id, name, cards, (1, 1), vec![], (0, 0)));
            }
        };
        match kind {
            Sha => {
                conversion(
                    100,
                    "wusheng",
                    "武圣",
                    owned
                        .filter(|&&card| card.kind != Sha && self.hero_card_is_red(card))
                        .map(|card| card.id)
                        .collect(),
                );
                conversion(
                    101,
                    "longdan",
                    "龙胆",
                    hand.filter(|card| card.kind == Shan)
                        .map(|card| card.id)
                        .collect(),
                );
            }
            Shan if responding => {
                conversion(
                    102,
                    "longdan",
                    "龙胆",
                    hand.clone()
                        .filter(|card| card.kind == Sha)
                        .map(|card| card.id)
                        .collect(),
                );
                conversion(
                    103,
                    "qingguo",
                    "倾国",
                    hand.filter(|&&card| card.kind != Shan && self.hero_card_is_black(card))
                        .map(|card| card.id)
                        .collect(),
                );
            }
            Tao if responding && self.active_seat != seat => {
                conversion(
                    104,
                    "jijiu",
                    "急救",
                    owned
                        .filter(|&&card| card.kind != Tao && self.hero_card_is_red(card))
                        .map(|card| card.id)
                        .collect(),
                );
            }
            Guohe if !responding => {
                conversion(
                    105,
                    "qixi",
                    "奇袭",
                    owned
                        .filter(|&&card| card.kind != Guohe && self.hero_card_is_black(card))
                        .map(|card| card.id)
                        .collect(),
                );
            }
            Lebu if !responding => {
                conversion(
                    106,
                    "guose",
                    "国色",
                    owned
                        .filter(|card| {
                            card.kind != Lebu
                                && self
                                    .face(card.id)
                                    .is_some_and(|face| face.suit == Suit::Diamond)
                        })
                        .map(|card| card.id)
                        .collect(),
                );
            }
            _ => {}
        }
        branches
    }

    pub(crate) fn hero_response_conversions(
        &self,
        seat: SeatId,
        kind: CardKind,
    ) -> Vec<ChoiceBranch> {
        self.hero_conversion_costs(seat, kind, true)
    }

    pub(crate) fn hero_play_conversions(&self, seat: SeatId) -> Vec<VirtualPlay> {
        let Ok(player) = self.seat(seat) else {
            return Vec::new();
        };
        let mut plays = Vec::new();
        for kind in [CardKind::Sha, CardKind::Guohe, CardKind::Lebu] {
            let mut costs = self.hero_conversion_costs(seat, kind, false);
            if costs.is_empty() {
                continue;
            }
            if kind == CardKind::Sha {
                plays.extend(self.hero_sha_conversion_plays(seat, costs));
                continue;
            }
            let targets: Vec<_> = self
                .trick_uses(
                    player,
                    Card {
                        id: CardId::MAX,
                        kind,
                    },
                )
                .into_iter()
                .flatten()
                .filter_map(|usage| usage.targets.first().copied())
                .collect();
            if targets.is_empty() {
                continue;
            }
            for cost in &mut costs {
                cost.targets.clone_from(&targets);
                cost.min_targets = 1;
                cost.max_targets = 1;
            }
            plays.extend(costs.into_iter().map(|branch| VirtualPlay { kind, branch }));
        }
        plays
    }

    fn hero_sha_conversion_plays(
        &self,
        seat: SeatId,
        costs: Vec<ChoiceBranch>,
    ) -> Vec<VirtualPlay> {
        let kind = CardKind::Sha;
        let player = self.seat(seat).expect("the converting seat exists");
        let fangtian = self
            .weapon(seat)
            .is_some_and(|card| card.kind == CardKind::Fangtian);
        let living = self.in_order_from(seat);
        let mut plays = Vec::new();
        for cost in costs {
            let mut compatible: Vec<ChoiceBranch> = Vec::new();
            for id in cost.cards {
                if !self.may_play_sha_without(seat, Some(id)) {
                    continue;
                }
                let targets: Vec<_> = living
                    .iter()
                    .copied()
                    .skip(1)
                    .filter(|&target| {
                        self.hero_allows_target(target, kind)
                            && self.in_sha_range_without(seat, target, Some(id))
                    })
                    .collect();
                if targets.is_empty() {
                    continue;
                }
                let last_hand_card = matches!(player.hand.as_slice(), [card] if card.id == id);
                let max_targets = if fangtian && last_hand_card {
                    3.min(targets.len())
                } else {
                    1
                };
                if let Some(group) = compatible
                    .iter_mut()
                    .find(|branch| branch.targets == targets && branch.max_targets == max_targets)
                {
                    group.cards.push(id);
                } else {
                    let branch_id = 200 + (plays.len() + compatible.len()) as u32;
                    compatible.push(branch(
                        branch_id,
                        &cost.label,
                        vec![id],
                        (1, 1),
                        targets,
                        (1, max_targets),
                    ));
                }
            }
            plays.extend(
                compatible
                    .into_iter()
                    .map(|branch| VirtualPlay { kind, branch }),
            );
        }
        plays
    }
}
