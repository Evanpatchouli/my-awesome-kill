use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};

use crate::{
    classic_data::{self, General, SkillDef, Suit},
    CardKind, DrawReason, EventKind, RuleError, SeatId,
};

pub const MAX_CONTENT_BYTES: usize = 128 * 1024;
const MAX_COPIES: usize = 128;
const KEY_BYTES: usize = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CardKey([u8; KEY_BYTES]);

impl CardKey {
    pub fn new(value: &str) -> Option<Self> {
        if value.len() > KEY_BYTES || !valid_id(value) {
            return None;
        }
        let mut bytes = [0; KEY_BYTES];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Some(Self(bytes))
    }

    pub fn as_str(&self) -> &str {
        let length = self
            .0
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(KEY_BYTES);
        std::str::from_utf8(&self.0[..length]).expect("content keys contain only ASCII")
    }
}

impl Serialize for CardKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CardKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).ok_or_else(|| {
            serde::de::Error::custom("expected a namespaced card ID of at most 24 ASCII bytes")
        })
    }
}

impl fmt::Display for CardKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentPack {
    pub schema: u16,
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub cards: Vec<CardDefinition>,
    #[serde(default)]
    pub deck: Vec<DeckEntry>,
    #[serde(default)]
    pub generals: Vec<GeneralDefinition>,
    #[serde(default)]
    pub skills: Vec<SkillDefinition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardDefinition {
    pub id: CardKey,
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub category: CardCategory,
    pub target: TargetRule,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CardCategory {
    Trick,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetRule {
    #[serde(rename = "self")]
    SelfOnly,
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum Effect {
    Draw { count: u16 },
    Recover { count: u16 },
}

impl Effect {
    pub(crate) fn event(self, source: SeatId, target: SeatId, reason: DrawReason) -> EventKind {
        match self {
            Self::Draw { count } => EventKind::Draw {
                seat: target,
                amount: count,
                reason,
            },
            Self::Recover { count } => EventKind::Recover {
                seat: source,
                target,
                amount: count,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeckEntry {
    pub card: CardKey,
    pub suit: Suit,
    pub rank: u8,
    pub copies: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneralDefinition {
    pub id: String,
    pub name: String,
    pub faction: String,
    pub gender: String,
    pub max_hp: u16,
    #[serde(default)]
    pub skills: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub timing: SkillTiming,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillTiming {
    PlayOnce,
    DrawAfter,
    DamageAfter,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPreview {
    pub cards: Vec<CardDefinition>,
    pub generals: Vec<General>,
}

#[derive(Default)]
pub(crate) struct Catalog {
    pub(crate) cards: BTreeMap<CardKey, CardDefinition>,
    pub(crate) generals: Vec<General>,
}

impl Catalog {
    pub(crate) fn card(&self, kind: CardKind) -> Option<&CardDefinition> {
        match kind {
            CardKind::Wuzhong => Some(wuzhong()),
            CardKind::Content(key) => self.cards.get(&key),
            _ => None,
        }
    }

    pub(crate) fn preview(&self) -> Option<ContentPreview> {
        (!self.cards.is_empty() || !self.generals.is_empty()).then(|| ContentPreview {
            cards: self.cards.values().cloned().collect(),
            generals: self.generals.clone(),
        })
    }
}

fn wuzhong() -> &'static CardDefinition {
    static DEFINITION: OnceLock<CardDefinition> = OnceLock::new();
    DEFINITION.get_or_init(|| {
        serde_json::from_str(include_str!("../data/wuzhong.json"))
            .expect("valid bundled draw trick definition")
    })
}

impl ContentPack {
    pub fn parse(raw: &str) -> Result<Self, RuleError> {
        if raw.len() > MAX_CONTENT_BYTES {
            return Err(invalid("文件超过 128 KiB"));
        }
        serde_json::from_str(raw).map_err(|error| invalid(error.to_string()))
    }

    pub fn preview(&self) -> Result<ContentPreview, RuleError> {
        let catalog = self.compile()?;
        Ok(ContentPreview {
            cards: self.cards.clone(),
            generals: catalog.generals,
        })
    }

    pub(crate) fn compile(&self) -> Result<Catalog, RuleError> {
        if self.schema != 1
            || self.id == "classic"
            || !valid_segment(&self.id)
            || self.id.len() > 16
            || !valid_text(&self.version, 32)
        {
            return Err(invalid("需要 schema 1、有效的包 ID 和版本"));
        }
        if self.cards.len() > 32
            || self.skills.len() > 32
            || self.generals.len() > 32
            || self.deck.len() > MAX_COPIES
        {
            return Err(invalid("牌种、武将、技能或牌堆条目过多"));
        }
        let in_pack = |value: &str| {
            valid_id(value)
                && value.len() <= 64
                && value
                    .split_once(':')
                    .is_some_and(|(prefix, _)| prefix == self.id)
        };
        let mut catalog = Catalog::default();
        for card in &self.cards {
            if !in_pack(card.id.as_str())
                || !valid_text(&card.name, 24)
                || !valid_text(&card.description, 240)
                || !valid_effects(&card.effects)
                || catalog.cards.insert(card.id, card.clone()).is_some()
            {
                return Err(invalid(format!("牌种 {} 的定义无效或重复", card.id)));
            }
        }
        let mut copy_count = 0;
        for entry in &self.deck {
            copy_count += usize::from(entry.copies);
            if !catalog.cards.contains_key(&entry.card)
                || !(1..=13).contains(&entry.rank)
                || entry.copies == 0
                || copy_count > MAX_COPIES
            {
                return Err(invalid(format!("牌堆条目 {} 无效或总数量超限", entry.card)));
            }
        }
        let native_skills: BTreeMap<_, _> = classic_data::roster()
            .iter()
            .flat_map(|general| &general.skills)
            .map(|skill| (skill.id.as_str(), skill))
            .collect();
        let mut skills = BTreeMap::new();
        for skill in &self.skills {
            if !in_pack(&skill.id)
                || !valid_text(&skill.name, 24)
                || !valid_text(&skill.description, 240)
                || !valid_effects(&skill.effects)
                || skills.insert(skill.id.as_str(), skill).is_some()
            {
                return Err(invalid(format!("技能 {} 的定义无效或重复", skill.id)));
            }
        }
        let mut ids = BTreeSet::new();
        for general in &self.generals {
            if !in_pack(&general.id)
                || !ids.insert(&general.id)
                || !valid_text(&general.name, 24)
                || !matches!(general.faction.as_str(), "wei" | "shu" | "wu" | "qun")
                || !matches!(general.gender.as_str(), "male" | "female")
                || !(1..=8).contains(&general.max_hp)
                || general.skills.len() > 8
            {
                return Err(invalid(format!("武将 {} 的定义无效或重复", general.id)));
            }
            let mut seen = BTreeSet::new();
            let mut definitions = Vec::new();
            for id in &general.skills {
                if !seen.insert(id) {
                    return Err(invalid(format!("武将 {} 重复引用技能 {id}", general.id)));
                }
                if let Some(skill) = native_skills.get(id.as_str()) {
                    definitions.push((*skill).clone());
                } else if let Some(skill) = skills.get(id.as_str()) {
                    definitions.push(SkillDef {
                        id: id.clone(),
                        name: skill.name.clone(),
                        description: skill.description.clone(),
                    });
                } else {
                    return Err(invalid(format!(
                        "武将 {} 引用了不存在的技能 {id}",
                        general.id
                    )));
                }
            }
            catalog.generals.push(General {
                id: general.id.clone(),
                name: general.name.clone(),
                faction: general.faction.clone(),
                gender: general.gender.clone(),
                max_hp: general.max_hp,
                is_lord: false,
                skills: definitions,
            });
        }
        Ok(catalog)
    }
}

fn invalid(reason: impl Into<String>) -> RuleError {
    RuleError::InvalidContent {
        reason: reason.into(),
    }
}

fn valid_id(value: &str) -> bool {
    value
        .split_once(':')
        .is_some_and(|(prefix, name)| valid_segment(prefix) && valid_segment(name))
}

fn valid_segment(segment: &str) -> bool {
    segment
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_lowercase)
        && segment.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn valid_text(text: &str, limit: usize) -> bool {
    !text.trim().is_empty() && text.chars().count() <= limit && !text.chars().any(char::is_control)
}

fn valid_effects(effects: &[Effect]) -> bool {
    !effects.is_empty()
        && effects.len() <= 4
        && effects.iter().all(|effect| match effect {
            Effect::Draw { count } => (1..=8).contains(count),
            Effect::Recover { count } => (1..=4).contains(count),
        })
}
