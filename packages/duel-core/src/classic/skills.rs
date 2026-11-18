mod active;
mod modifiers;
#[cfg(test)]
mod tests;
mod triggers;
pub(crate) use triggers::Queue as Dispatch;

use crate::scheduler::{EventRef, Outcome};
use crate::{
    Card, CardId, CardKind, ChoiceBranch, ChoiceRequest, ChoiceView, EventKind, Game, Program,
    Resolution, SeatId, Selection, Zone,
};
use serde::{Deserialize, Serialize};

macro_rules! skill_ids {
    ($($name:ident => $id:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
        pub enum SkillId {
            $(#[serde(rename = $id)] $name),+
        }

        impl SkillId {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$name => $id),+ }
            }
        }
    };
}

skill_ids! {
    Biyue => "biyue", Fanjian => "fanjian", Fankui => "fankui", Ganglie => "ganglie",
    Guanxing => "guanxing", Guicai => "guicai", Guose => "guose", Hujia => "hujia",
    Jianxiong => "jianxiong", Jieyin => "jieyin", Jijiang => "jijiang", Jijiu => "jijiu",
    Jiuyuan => "jiuyuan", Jizhi => "jizhi", Keji => "keji", Kongcheng => "kongcheng",
    Kurou => "kurou", Lianying => "lianying", Lijian => "lijian", Liuli => "liuli",
    Longdan => "longdan", Luoshen => "luoshen", Luoyi => "luoyi", Mashu => "mashu",
    Paoxiao => "paoxiao", Qianxun => "qianxun", Qicai => "qicai", Qingguo => "qingguo",
    Qingnang => "qingnang", Qixi => "qixi", Rende => "rende", Tiandu => "tiandu",
    Tieji => "tieji", Tuxi => "tuxi", Wusheng => "wusheng", Wushuang => "wushuang",
    Xiaoji => "xiaoji", Yiji => "yiji", Yingzi => "yingzi", Zhiheng => "zhiheng",
}

#[derive(Default)]
pub(crate) struct HeroTurn {
    used: Vec<SkillId>,
    rende_given: usize,
    played_sha: bool,
    pub(super) recast_attempted: bool,
    preview: Vec<Card>,
}

impl HeroTurn {
    pub(crate) fn reset_play_phase(&mut self) {
        self.used.clear();
        self.rende_given = 0;
        self.recast_attempted = false;
    }
}

#[derive(Clone)]
pub(crate) enum HeroFlow {
    Trigger(triggers::Trigger),
    Activation { seat: SeatId, skill: SkillId },
    Active(active::Active),
    LordRequest(active::LordRequest),
}

impl HeroFlow {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::Trigger(trigger) => trigger.kind(),
            Self::Activation { seat, skill } => EventKind::Skill {
                seat: *seat,
                skill: *skill,
            },
            Self::Active(active) => active.kind(),
            Self::LordRequest(request) => request.kind(),
        }
    }
}

#[derive(Clone)]
pub(crate) enum HeroMoment {
    Preparation {
        seat: SeatId,
    },
    Draw {
        seat: SeatId,
        amount: u16,
    },
    Discard {
        seat: SeatId,
    },
    End {
        seat: SeatId,
    },
    UseTrick {
        seat: SeatId,
        kind: CardKind,
        cards: Vec<Card>,
    },
    DamageAfter {
        source: Option<SeatId>,
        target: SeatId,
        amount: u16,
        cards: Vec<Card>,
    },
    JudgeBefore {
        seat: SeatId,
        card: Card,
    },
    JudgeAfter {
        seat: SeatId,
        card: Card,
        keep_black: bool,
    },
    ShaRedirect {
        seat: SeatId,
        target: SeatId,
        excluded: Vec<SeatId>,
    },
    ShaTarget {
        seat: SeatId,
        target: SeatId,
    },
    LostCards {
        seat: SeatId,
        cards: Vec<(Zone, Card)>,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum HeroAdjustment {
    None,
    Draw(u16),
    Skip,
    Judgment(Card),
    Redirect(SeatId),
    DirectHit,
}

fn branch(
    id: u32,
    label: &str,
    cards: Vec<CardId>,
    card_count: (usize, usize),
    targets: Vec<SeatId>,
    target_count: (usize, usize),
) -> ChoiceBranch {
    ChoiceBranch {
        id,
        label: label.into(),
        cards,
        min_cards: card_count.0,
        max_cards: card_count.1,
        targets,
        min_targets: target_count.0,
        max_targets: target_count.1,
    }
}

fn option(id: u32, label: &str) -> ChoiceBranch {
    branch(id, label, vec![], (0, 0), vec![], (0, 0))
}

fn selection(child: Option<Outcome<Resolution>>) -> Option<Selection> {
    match child {
        Some(Outcome::Finished(Resolution::Selected(result))) => Some(result),
        _ => None,
    }
}

impl Game {
    pub(crate) fn resolve_hero(
        &mut self,
        _event: EventRef,
        flow: HeroFlow,
        child: Option<Outcome<Resolution>>,
    ) {
        match flow {
            HeroFlow::Trigger(trigger) => self.resolve_hero_trigger(trigger, child),
            HeroFlow::Activation { .. } => self.finish(),
            HeroFlow::Active(active) => self.resolve_hero_active(active, child),
            HeroFlow::LordRequest(request) => self.resolve_hero_lord_request(request, child),
        }
    }

    fn finish_hero(&mut self, adjustment: HeroAdjustment) {
        self.scheduler
            .end(Outcome::Finished(Resolution::Hero(adjustment)));
    }

    fn choose_hero(&mut self, seat: SeatId, target: Option<SeatId>, branches: Vec<ChoiceBranch>) {
        self.scheduler.next(Program::Choose(ChoiceRequest {
            seat,
            target,
            branches,
        }));
    }

    fn choose_hero_yes(&mut self, seat: SeatId, target: Option<SeatId>, name: &str) {
        self.choose_hero(seat, target, vec![option(0, "放弃"), option(1, name)]);
    }

    pub(crate) fn hero_card_zone(&self, seat: SeatId, id: CardId) -> Option<(Zone, Card)> {
        let player = self.seat(seat).ok()?;
        player
            .hand
            .iter()
            .find(|card| card.id == id)
            .copied()
            .map(|card| (Zone::Hand, card))
            .or_else(|| {
                player
                    .equipment
                    .iter()
                    .find(|card| card.id == id)
                    .copied()
                    .map(|card| (Zone::Equipment, card))
            })
    }

    pub(crate) fn hero_cards_by_id(&self, seat: SeatId, ids: &[CardId]) -> Vec<Card> {
        ids.iter()
            .map(|&id| {
                self.hero_card_zone(seat, id)
                    .expect("the selection validated the card")
                    .1
            })
            .collect()
    }

    fn hero_owned_card_ids(&self, seat: SeatId, equipment: bool) -> Vec<CardId> {
        let owner = self.seat(seat).expect("the owner exists");
        let equipment = if equipment {
            owner.equipment.as_slice()
        } else {
            &[]
        };
        owner
            .hand
            .iter()
            .chain(equipment)
            .map(|card| card.id)
            .collect()
    }

    fn hero_discard_cards(&mut self, seat: SeatId, ids: &[CardId]) {
        let cards = ids
            .iter()
            .map(|&id| {
                self.hero_card_zone(seat, id)
                    .expect("the selection validated the cost")
            })
            .collect();
        self.queue_discard_batch(seat, cards);
    }

    fn hero_transfer_card(&mut self, seat: SeatId, zone: Zone, id: CardId, target: SeatId) {
        let card = self.take_card(seat, zone, id);
        self.put_in_hand(target, card);
        self.queue_hero_losses(seat, vec![(zone, card)]);
    }

    pub(crate) fn hero_private_choice_cards(&self, choice: &ChoiceView) -> Vec<Card> {
        let Some(classic) = &self.classic else {
            return Vec::new();
        };
        classic
            .hero_turn
            .preview
            .iter()
            .filter(|card| {
                choice
                    .branches
                    .iter()
                    .any(|branch| branch.cards.contains(&card.id))
            })
            .copied()
            .collect()
    }
}
