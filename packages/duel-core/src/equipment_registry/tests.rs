use crate::classic::tests::scene;
use crate::classic_data::Role;
use crate::equipment_registry::EquipmentArea;
use crate::*;
use std::sync::Arc;

const OWNER: SeatId = 1;
const MATERIAL: Card = Card {
    id: 1201,
    kind: CardKind::Qinggang,
};
const NEW: Card = Card {
    id: 1202,
    kind: CardKind::Zhuge,
};
const SECOND: Card = Card {
    id: 1203,
    kind: CardKind::Qilin,
};

#[derive(Clone, Copy, Debug)]
enum Operation {
    Expand,
    AddJudge,
    ConsumeJudge,
    EnableJudge,
    Recover,
    Reinstall,
    InstallNew,
    HideMaterial,
    InstallSecond,
    Disable,
}

const OPERATIONS: [Operation; 10] = [
    Operation::Expand,
    Operation::AddJudge,
    Operation::ConsumeJudge,
    Operation::EnableJudge,
    Operation::Recover,
    Operation::Reinstall,
    Operation::InstallNew,
    Operation::HideMaterial,
    Operation::InstallSecond,
    Operation::Disable,
];

struct Table {
    game: Game,
    skills: Vec<u64>,
    log: Vec<Command>,
}

impl Table {
    fn new() -> Self {
        let mut game = scene(
            &[
                Role::Lord,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Loyalist,
                Role::Rebel,
                Role::Renegade,
            ],
            0,
        );
        game.seat_mut(OWNER).hand = vec![MATERIAL, NEW, SECOND];
        for card in [MATERIAL, NEW, SECOND] {
            game.mark_known_to(card.id, 0);
        }
        let skills = OPERATIONS
            .iter()
            .map(|&op| {
                game.register_active_skill(ActiveSkillRegistration {
                    owner: 0,
                    skill: format!("registry_fixture_{}", op as usize),
                    limited: false,
                    choice: Arc::new(|_, _| {
                        Some(ChoiceBranch {
                            id: 0,
                            label: "fixture".into(),
                            cards: vec![],
                            min_cards: 0,
                            max_cards: 0,
                            targets: vec![],
                            min_targets: 0,
                            max_targets: 0,
                        })
                    }),
                    effect: Arc::new(move |ctx, _, _| {
                        match op {
                            Operation::Expand => {
                                ctx.expand_equip(OWNER, vec![EquipmentSlot::Weapon])
                            }
                            Operation::AddJudge => {
                                ctx.add_judge(OWNER, MATERIAL.id, CardKind::Lebu)
                            }
                            Operation::ConsumeJudge => ctx.disable_judge(OWNER),
                            Operation::EnableJudge => ctx.enable_judge(OWNER),
                            Operation::Recover => {
                                ctx.gain_public_pile_card(OWNER, CardZone::DiscardPile, MATERIAL.id)
                            }
                            Operation::Reinstall => ctx.equip_from_hand(OWNER, MATERIAL.id),
                            Operation::InstallNew => ctx.equip_from_hand(OWNER, NEW.id),
                            Operation::InstallSecond => ctx.equip_from_hand(OWNER, SECOND.id),
                            Operation::Disable => {
                                ctx.disable_equip(OWNER, vec![EquipmentSlot::Weapon])
                            }
                            Operation::HideMaterial => {
                                ctx.give_cards(OWNER, 2, &[MATERIAL.id], false)
                            }
                        }
                        .unwrap_or_else(|err| panic!("{op:?} failed: {err:?}"));
                        TriggerStep::Done
                    }),
                })
                .unwrap()
            })
            .collect();
        // This ordering is also observed in the pinned virtual-equip combat fixture.
        game.register_trigger(TriggerRegistration {
            owner: OWNER,
            skill: "reinstall_pending_judgment_material".into(),
            timing: "discardBefore".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            silent: true,
            label: None,
            filter: Arc::new(|game, e, _| {
                e.player == Some(OWNER)
                    && game
                        .judgments(OWNER)
                        .any(|j| j.material == MATERIAL && j.is_attached())
            }),
            effect: Arc::new(|ctx, step, _| {
                match step {
                    0 => ctx.gain_owned_card(OWNER, OWNER, MATERIAL.id).unwrap(),
                    1 => ctx.equip_from_hand(OWNER, MATERIAL.id).unwrap(),
                    _ => return TriggerStep::Done,
                }
                TriggerStep::Continue(step + 1)
            }),
        })
        .unwrap();
        game.scheduler.start(Program::event(EventKind::Phase {
            seat: 0,
            phase: Phase::Play,
        }));
        game.run(None);
        Self {
            game,
            skills,
            log: Vec::new(),
        }
    }

    fn submit(&mut self, command: Command) {
        self.game.submit(command.clone()).unwrap();
        self.log.push(command);
        for card in [MATERIAL, NEW, SECOND] {
            let held = self.game.seats.iter().flat_map(|s| {
                s.hand
                    .iter()
                    .chain(&s.equipment)
                    .chain(&s.judges)
                    .chain(&s.expansions)
            });
            let count = held
                .chain(&self.game.discard)
                .chain(&self.game.resolving)
                .chain(&self.game.transferring)
                .chain(&self.game.reserved_draws)
                .chain(&self.game.classic.as_ref().unwrap().deck)
                .filter(|&&c| c == card)
                .count();
            assert_eq!(count, 1, "physical material {card:?}");
        }
    }

    fn activate(&mut self, operation: Operation) {
        self.submit(Command {
            version: self.game.version,
            seat: 0,
            action: Action::ActivateSkill {
                registration: self.skills[operation as usize],
                cards: vec![],
                targets: vec![],
            },
        });
    }

    fn detach(&mut self) {
        self.activate(Operation::AddJudge);
        self.activate(Operation::ConsumeJudge);
        self.activate(Operation::EnableJudge);
        // The reference records the detached sword before combat. Its later
        // reinstallation, token selection, and replay are Rust regressions.
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tests/reference/virtual-equip-combat.json"
        ))
        .unwrap();
        let before = &golden["trials"]
            .as_array()
            .unwrap()
            .iter()
            .find(|trial| trial["name"] == "bagua-orphan")
            .unwrap()["before"];
        let area = &self.game.seat(OWNER).unwrap().equipment;
        assert_eq!(serde_json::json!(area.len()), before["attackerPhysical"]);
        assert_eq!(
            serde_json::json!(area.registrations().iter().any(|r| r.card == MATERIAL)),
            before["attackerVirtual"]
        );
        assert_eq!(
            serde_json::json!(self.game.has_skill(OWNER, "qinggang_skill")),
            before["qinggang"]
        );
        assert_eq!(
            if self.game.discard.contains(&MATERIAL) {
                "d"
            } else {
                "elsewhere"
            },
            before["weapon"]
        );
        self.activate(Operation::Recover);
    }

    fn choice(&self, seat: SeatId) -> ChoiceView {
        self.game.view(Some(seat)).unwrap().choice.unwrap()
    }

    fn answer(&mut self, seat: SeatId, action: Action) {
        self.submit(Command {
            version: self.game.version,
            seat,
            action,
        });
    }

    fn public_card_choice(&mut self, seat: SeatId, card: Card) {
        let choice = self.choice(seat);
        let branch = &choice.branches[0];
        assert!(branch.cards.contains(&card.id));
        self.answer(
            seat,
            Action::Select {
                event: choice.event,
                branch: branch.id,
                cards: vec![card.id],
                targets: vec![],
            },
        );
    }
}

#[test]
fn reused_material_keeps_distinct_registration_choices_and_replays() {
    for use_ai in [false, true] {
        let mut table = Table::new();
        table.activate(Operation::Expand);
        table.detach();
        let first = table.game.seat(OWNER).unwrap().equipment.registrations()[0].key;
        table.activate(Operation::Reinstall);
        let entries = table.game.seat(OWNER).unwrap().equipment.registrations();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, first);
        assert_ne!(entries[1].key, first);
        assert!(!entries[0].attached && entries[1].attached);
        assert_eq!(table.game.seat(OWNER).unwrap().equipment, [MATERIAL]);
        table.activate(Operation::InstallNew);
        let choice = table.choice(OWNER);
        for viewer in (0..8).filter(|&seat| seat != OWNER).map(Some).chain([None]) {
            assert!(table.game.view(viewer).unwrap().choice.is_none());
        }
        let buttons = choice.equipment_buttons.as_ref().unwrap();
        assert_eq!(buttons.required, 1);
        assert_eq!(buttons.options.len(), 2);
        let detached = &buttons.options[0];
        let attached = &buttons.options[1];
        assert_eq!((detached.kind, detached.card), (MATERIAL.kind, None));
        assert!(serde_json::to_value(detached)
            .unwrap()
            .get("card")
            .is_none());
        assert_eq!(attached.card, Some(MATERIAL.id));
        assert_ne!(detached.id, attached.id);
        for (seat, ids) in [
            (OWNER, vec![u32::MAX]),
            (OWNER, vec![attached.id, attached.id]),
            (0, vec![attached.id]),
        ] {
            assert!(table
                .game
                .submit(Command {
                    version: table.game.version,
                    seat,
                    action: Action::SelectEquipmentButtons {
                        event: choice.event,
                        buttons: ids
                    }
                })
                .is_err());
            assert_eq!(table.choice(OWNER), choice);
        }
        let loss_start = table.game.loss_history().len();
        if use_ai {
            let bot = table.game.bot_command().unwrap();
            assert_eq!(
                bot.action,
                Action::SelectEquipmentButtons {
                    event: choice.event,
                    buttons: vec![detached.id]
                }
            );
            table.submit(bot);
        } else {
            table.answer(
                OWNER,
                Action::SelectEquipmentButtons {
                    event: choice.event,
                    buttons: vec![attached.id],
                },
            );
        }
        let area = &table.game.seat(OWNER).unwrap().equipment;
        assert_eq!(area.registrations()[0].key, first);
        assert!(!area.registrations()[0].attached);
        let old_losses = table.game.loss_history()[loss_start..]
            .iter()
            .filter(|loss| loss.cards.contains(&(Zone::Equipment, MATERIAL)))
            .count();
        assert_eq!(old_losses, usize::from(!use_ai));
        assert_eq!(area.as_slice(), if use_ai { &[MATERIAL] } else { &[NEW] });
        assert!(table
            .game
            .discard
            .contains(if use_ai { &NEW } else { &MATERIAL }));

        let mut replay = Table::new();
        let log: Vec<Command> =
            serde_json::from_slice(&serde_json::to_vec(&table.log).unwrap()).unwrap();
        for command in log {
            replay.submit(command);
        }
        for viewer in (0..8).map(Some).chain([None]) {
            assert_eq!(
                table.game.view(viewer).unwrap(),
                replay.game.view(viewer).unwrap()
            );
        }
    }
}

#[test]
fn multiple_detached_registrations_keep_foreign_hand_material_private() {
    let mut table = Table::new();
    table.activate(Operation::Expand);
    table.detach();
    table.detach();
    // The face-down transfer obscures the remaining hand to the active seat.
    // The holder may still install their own hand card.
    table
        .game
        .register_trigger(TriggerRegistration {
            owner: OWNER,
            skill: "install_after_hidden_transfer".into(),
            timing: "gainAfter".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, event, _| event.player == Some(2)),
            effect: Arc::new(|ctx, _, _| {
                ctx.equip_from_hand(OWNER, NEW.id).unwrap();
                TriggerStep::Done
            }),
        })
        .unwrap();
    table.activate(Operation::HideMaterial);
    assert!(table.game.seat(2).unwrap().hand.contains(&MATERIAL));
    assert!(!table.game.is_known_by(MATERIAL.id, Some(3)));
    let choice = table.choice(OWNER);
    let options = &choice.equipment_buttons.as_ref().unwrap().options;
    assert_eq!(options.len(), 2);
    assert_ne!(options[0].id, options[1].id);
    assert!(options
        .iter()
        .all(|b| b.card.is_none() && b.kind == MATERIAL.kind));
    table.answer(
        OWNER,
        Action::SelectEquipmentButtons {
            event: choice.event,
            buttons: vec![options[1].id],
        },
    );
    assert!(table.game.seat(OWNER).unwrap().equipment.is_empty());
    assert!(table.game.discard.contains(&NEW));
    assert!(table.game.seat(2).unwrap().hand.contains(&MATERIAL));
}

#[test]
fn replacement_rechecks_registration_key_after_reinstalling_same_entity() {
    let mut table = Table::new();
    table.activate(Operation::Expand);
    table.activate(Operation::Reinstall);
    table.activate(Operation::InstallSecond);
    let old = table.game.seat(OWNER).unwrap().equipment.registrations()[0].key;
    table
        .game
        .register_trigger(TriggerRegistration {
            owner: OWNER,
            skill: "change_equipment_key_during_choice".into(),
            timing: "chooseButtonEnd".into(),
            role: TriggerRole::Global,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, e, _| e.button_links == [MATERIAL.id]),
            effect: Arc::new(|ctx, step, _| {
                match step {
                    0 => ctx.gain_owned_card(OWNER, OWNER, MATERIAL.id).unwrap(),
                    1 => ctx.equip_from_hand(OWNER, MATERIAL.id).unwrap(),
                    _ => return TriggerStep::Done,
                }
                TriggerStep::Continue(step + 1)
            }),
        })
        .unwrap();
    table.activate(Operation::InstallNew);
    assert_eq!(
        table.choice(OWNER).branches[0].cards,
        [MATERIAL.id, SECOND.id]
    );
    table.public_card_choice(OWNER, MATERIAL);
    let area = &table.game.seat(OWNER).unwrap().equipment;
    assert!(area.attached(old).is_none());
    assert!(area
        .registrations()
        .iter()
        .any(|r| r.key != old && r.card == MATERIAL && r.attached));
    assert!(area.contains(&SECOND) && area.contains(&MATERIAL));
    assert!(table.game.discard.contains(&NEW));
}

#[test]
fn replacement_loses_all_over_capacity_equipment_in_one_event() {
    let mut table = Table::new();
    table.activate(Operation::Expand);
    table.activate(Operation::Reinstall);
    table.activate(Operation::InstallSecond);
    table
        .game
        .register_trigger(TriggerRegistration {
            owner: OWNER,
            skill: "keep_excess_weapons".into(),
            timing: "loseToDiscardpileBefore".into(),
            role: TriggerRole::Player,
            order: TriggerOrder::First,
            priority: 0,
            label: None,
            silent: true,
            filter: Arc::new(|_, _, _| true),
            effect: Arc::new(|_, _, _| TriggerStep::CancelSubject),
        })
        .unwrap();
    table.activate(Operation::Disable);
    assert_eq!(table.choice(0).branches[0].cards, [SECOND.id, MATERIAL.id]);
    table.public_card_choice(0, MATERIAL);
    assert_eq!(
        table
            .game
            .equipment_slot_state(OWNER, EquipmentSlot::Weapon)
            .enabled,
        1
    );
    assert_eq!(table.game.seat(OWNER).unwrap().equipment.len(), 2);
    let before = table.game.loss_history().len();
    table.activate(Operation::InstallNew);
    assert_eq!(table.game.seat(OWNER).unwrap().equipment, [NEW]);
    let losses: Vec<_> = table.game.loss_history()[before..]
        .iter()
        .filter(|loss| loss.cards.iter().any(|(zone, _)| *zone == Zone::Equipment))
        .collect();
    assert_eq!(losses.len(), 1);
    assert_eq!(losses[0].stockcards, [SECOND, MATERIAL]);
    assert_eq!(
        losses[0].cards,
        [(Zone::Equipment, SECOND), (Zone::Equipment, MATERIAL)]
    );
    for card in [MATERIAL, SECOND] {
        assert!(losses[0].cards.contains(&(Zone::Equipment, card)));
        assert!(table.game.discard.contains(&card));
    }
}

#[test]
fn physical_equipment_orders_slots_then_newest_within_the_slot() {
    let armor = Card {
        id: 301,
        kind: CardKind::Bagua,
    };
    let first = Card {
        id: 302,
        kind: CardKind::Zhuge,
    };
    let second = Card {
        id: 303,
        kind: CardKind::Qinggang,
    };
    let mut area = EquipmentArea::new(OWNER);
    area.install(armor);
    area.install(first);
    area.install(second);
    assert_eq!(area.as_slice(), [second, first, armor]);
    assert_eq!(
        area.registrations()
            .iter()
            .map(|entry| entry.card)
            .collect::<Vec<_>>(),
        [first, second, armor]
    );
}
