use super::*;
use crate::TemporarySkill;
use serde_json::Value;

const QINGGANG: &str = "qinggang_skill";
const CIXIONG: &str = "cixiong_skill";
const SWORD: Card = Card {
    id: 4101,
    kind: CardKind::Qinggang,
};
const TWIN: Card = Card {
    id: 4102,
    kind: CardKind::Cixiong,
};
const SECOND_SWORD: Card = Card {
    id: 4103,
    kind: CardKind::Qinggang,
};
const SHA: Card = Card {
    id: 4104,
    kind: CardKind::Sha,
};
const COST: Card = Card {
    id: 4105,
    kind: CardKind::Tao,
};
const TRACKED: [Card; 5] = [SWORD, TWIN, SECOND_SWORD, SHA, COST];

#[derive(Clone, Copy, Debug)]
enum Op {
    Expand,
    Grant,
    Revoke,
    EquipTwin,
    EquipSword,
    EquipSecond,
    DiscardSword,
    RecoverSword,
}

const OPS: [Op; 8] = [
    Op::Expand,
    Op::Grant,
    Op::Revoke,
    Op::EquipTwin,
    Op::EquipSword,
    Op::EquipSecond,
    Op::DiscardSword,
    Op::RecoverSword,
];

struct Table {
    game: Game,
    skills: Vec<u64>,
    log: Vec<Command>,
    effects: Vec<&'static str>,
    bot: bool,
}

fn reference() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tests/reference/sha-target-equipment.json"
    ))
    .unwrap()
}

impl Table {
    fn new(bot: bool) -> Self {
        let mut game = scene(
            &[
                Role::Lord,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Rebel,
            ],
            0,
        );
        for (seat, hero) in [(0, "liubei"), (1, "diaochan")] {
            game.seat_mut(seat).hero = classic_data::roster().iter().find(|h| h.id == hero);
        }
        game.classic.as_mut().unwrap().human = if bot { 7 } else { 0 };
        game.seat_mut(0)
            .hand
            .extend([SWORD, TWIN, SECOND_SWORD, SHA]);
        game.seat_mut(1).hand.push(COST);
        let skills = OPS
            .iter()
            .map(|&op| {
                game.register_active_skill(ActiveSkillRegistration {
                    owner: 0,
                    skill: format!("hook_fixture_{op:?}"),
                    limited: false,
                    choice: Arc::new(|_, _| {
                        Some(ChoiceBranch {
                            id: 0,
                            label: "fixture".into(),
                            cards: vec![],
                            targets: vec![],
                            min_cards: 0,
                            max_cards: 0,
                            min_targets: 0,
                            max_targets: 0,
                        })
                    }),
                    effect: Arc::new(move |ctx, _, _| {
                        match op {
                            Op::Expand => ctx
                                .expand_equip(0, vec![crate::EquipmentSlot::Weapon; 2])
                                .unwrap(),
                            Op::Grant => ctx
                                .grant_temporary(0, TemporarySkill::until_removed(QINGGANG))
                                .unwrap(),
                            Op::Revoke => {
                                ctx.remove_temporary(0, QINGGANG);
                            }
                            Op::EquipTwin => ctx.equip_from_hand(0, TWIN.id).unwrap(),
                            Op::EquipSword => ctx.equip_from_hand(0, SWORD.id).unwrap(),
                            Op::EquipSecond => ctx.equip_from_hand(0, SECOND_SWORD.id).unwrap(),
                            Op::DiscardSword => ctx.discard_held(0, &[SWORD.id]).unwrap(),
                            Op::RecoverSword => ctx
                                .gain_public_pile_card(0, crate::CardZone::DiscardPile, SWORD.id)
                                .unwrap(),
                        }
                        TriggerStep::Done
                    }),
                })
                .unwrap()
            })
            .collect();
        play_phase(&mut game);
        Self {
            game,
            skills,
            log: Vec::new(),
            effects: Vec::new(),
            bot,
        }
    }

    fn submit(&mut self, cmd: Command) {
        self.game.submit(cmd.clone()).unwrap();
        self.log.push(cmd);
        multi_weapon_conservation(&self.game, &TRACKED);
        for event in &self.game.events {
            if let EventKind::EquipmentEffect {
                seat: 0,
                target: 1,
                kind,
            } = event.kind
            {
                if event.status == EventStatus::Started {
                    self.effects.push(match kind {
                        CardKind::Qinggang => QINGGANG,
                        CardKind::Cixiong => CIXIONG,
                        _ => panic!("unexpected effect {kind:?}"),
                    });
                }
            }
        }
    }

    fn act(&mut self, seat: SeatId, action: Action) {
        self.submit(Command {
            version: self.game.version,
            seat,
            action,
        });
    }

    fn apply(&mut self, ops: &[Op]) {
        for &op in ops {
            self.act(
                0,
                Action::ActivateSkill {
                    registration: self.skills[op as usize],
                    cards: vec![],
                    targets: vec![],
                },
            );
        }
    }

    fn assert_state(&self, golden: &Value, stage: &str) {
        let expected = &golden["states"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r[0] == stage)
            .unwrap()[1];
        let player = self.game.seat(0).unwrap();
        assert_eq!(
            self.game.has_skill(0, QINGGANG),
            expected["hasSkill"],
            "{stage}"
        );
        assert_eq!(
            self.game.seat_has_non_equipment_skill(player, QINGGANG),
            expected["temporary"],
            "{stage}"
        );
        assert_eq!(
            equipment::has_equipment_skill(player, QINGGANG),
            expected["equipSkill"],
            "{stage}"
        );
        let entities: Vec<_> = player
            .equipment
            .iter()
            .map(|&card| match card {
                SWORD => "order-sword",
                TWIN => "twin",
                _ => panic!("unmapped entity: {card:?}"),
            })
            .collect();
        assert_eq!(serde_json::json!(entities), expected["equips"], "{stage}");
    }

    fn attack(&mut self, order: &[&str], expected: &[&str]) {
        self.act(
            0,
            Action::PlayCard {
                card: SHA.id,
                targets: vec![1],
            },
        );
        if !order.is_empty() {
            let choice = self.game.view(Some(0)).unwrap().choice.unwrap();
            let labels: Vec<_> = choice
                .branches
                .iter()
                .map(|branch| branch.label.as_str())
                .collect();
            assert_eq!(labels, order);
            if self.bot {
                self.submit(self.game.bot_command().unwrap());
            } else {
                self.act(
                    0,
                    Action::Select {
                        event: choice.event,
                        branch: choice.branches[0].id,
                        cards: vec![],
                        targets: vec![],
                    },
                );
            }
        }
        // Resolve the optional Twin Swords effect and the target's discard through the table.
        for (seat, cards) in [(0, vec![]), (1, vec![COST.id])] {
            let choice = self.game.view(Some(seat)).unwrap().choice.unwrap();
            assert_eq!(choice.kind, ChoiceKind::Content);
            self.act(
                seat,
                Action::Select {
                    event: choice.event,
                    branch: 1,
                    cards,
                    targets: vec![],
                },
            );
        }
        let choice = self.game.view(Some(1)).unwrap().choice.unwrap();
        assert_eq!(choice.kind, ChoiceKind::Shan);
        self.act(
            1,
            Action::Choose {
                event: choice.event,
                card: None,
            },
        );
        assert_eq!(self.effects, expected);
        assert_eq!(self.game.seat(1).unwrap().hp, 3);
        assert!(self.game.discard.contains(&SHA) && self.game.discard.contains(&COST));
        let mut replay = Table::new(self.bot);
        let commands: Vec<Command> =
            serde_json::from_slice(&serde_json::to_vec(&self.log).unwrap()).unwrap();
        for command in commands {
            replay.submit(command);
        }
        assert_eq!(replay.effects, self.effects);
        for viewer in (0..8).map(Some).chain([None]) {
            assert_eq!(
                self.game.view(viewer).unwrap(),
                replay.game.view(viewer).unwrap()
            );
        }
    }

    fn attack_like_reference(&mut self, golden: &Value, trial: &str) {
        let order: Vec<_> = golden["controls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r[0] == trial)
            .into_iter()
            .flat_map(|r| r[2].as_array().unwrap())
            .map(|v| v.as_str().unwrap())
            .collect();
        let expected: Vec<_> = golden["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r[0] == trial && r[2] == "begin")
            .map(|r| r[1].as_str().unwrap())
            .collect();
        assert!(!expected.is_empty(), "missing reference trial {trial}");
        self.attack(&order, &expected);
    }
}

#[test]
fn sha_hook_order_and_sources_follow_recorded_tables() {
    use Op::*;
    let golden = reference();
    for (trial, setup, state, bot) in [
        (
            "human-order",
            vec![EquipTwin, EquipSword],
            "human-order",
            false,
        ),
        (
            "temporary-plus-equipped",
            vec![Grant, EquipTwin, EquipSword],
            "temporary-with-equipment",
            true,
        ),
        (
            "temporary-after-discard",
            vec![Grant, EquipTwin, EquipSword, DiscardSword],
            "temporary-after-discard",
            true,
        ),
        (
            "temporary-removed-while-equipped",
            vec![Grant, EquipTwin, EquipSword, Revoke],
            "probe-after-remove",
            true,
        ),
        (
            "equipment-after-temporary",
            vec![
                Grant,
                EquipTwin,
                EquipSword,
                DiscardSword,
                Revoke,
                RecoverSword,
                EquipSword,
            ],
            "equipment-after-temporary",
            true,
        ),
        (
            "equipment-then-temporary",
            vec![EquipTwin, EquipSword, Grant],
            "equipment-then-temporary",
            true,
        ),
    ] {
        let mut table = Table::new(bot);
        table.apply(&[Expand]);
        table.apply(&setup);
        table.assert_state(&golden, state);
        table.attack_like_reference(&golden, trial);
    }
}

#[test]
fn retaining_a_second_sword_keeps_the_first_hook_position() {
    // Rust regression; the reference samples the single-copy source transitions above.
    use Op::*;
    let mut table = Table::new(true);
    table.apply(&[Expand, EquipSword, EquipTwin, EquipSecond, DiscardSword]);
    assert_eq!(
        table.game.seat(0).unwrap().equipment.as_slice(),
        [SECOND_SWORD, TWIN]
    );
    table.attack(&["青釭剑", "雌雄双股剑"], &[QINGGANG, CIXIONG]);
}
