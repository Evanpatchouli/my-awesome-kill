use super::*;
use crate::classic::tests::{card, scene};
use crate::classic_data::{Role, Suit};
use crate::{
    CardFace, CardZone, Phase, Program, TriggerOrder, TriggerRegistration, TriggerRole, TriggerStep,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn material_position(game: &Game, card: Card) -> Option<&'static str> {
    match game
        .card_location(card.id)
        .map(|place| (place.owner, place.zone))
    {
        Some((Some(1), Some(CardZone::Judgment))) => Some("j"),
        Some((Some(0), Some(CardZone::Hand))) => Some("A:h"),
        Some((Some(1), Some(CardZone::Hand))) => Some("B:h"),
        Some((Some(2), Some(CardZone::Hand))) => Some("C:h"),
        Some((_, Some(CardZone::Ordering))) => Some("o"),
        Some((_, Some(CardZone::Special))) => Some("s"),
        Some((_, Some(CardZone::DiscardPile))) => Some("d"),
        _ => None,
    }
}

fn snapshot(game: &Game, material: Card) -> Value {
    let public = game.view(None).unwrap();
    let seat = &public.seats[1];
    let visible_names = if seat.logical_judges.is_empty() {
        seat.judges
            .iter()
            .map(|card| {
                seat.judge_assignments
                    .iter()
                    .find(|entry| entry.id == card.id)
                    .map_or(card.kind, |entry| entry.kind)
            })
            .collect::<Vec<_>>()
    } else {
        assert!(seat.logical_judges.iter().all(|entry| entry
            .material
            .is_none_or(|id| seat.judges.iter().any(|card| card.id == id))));
        seat.logical_judges.iter().map(|entry| entry.kind).collect()
    };
    let entries = game.judgments(1).collect::<Vec<_>>();
    for entry in &entries {
        assert!(entry.is_virtual());
        assert_eq!(entry.material, material);
        assert_eq!(
            entry.is_attached(),
            material_position(game, material) == Some("j")
        );
    }
    let visibility = [0, 1, 2].map(|viewer| {
        game.view(Some(viewer))
            .unwrap()
            .card_faces
            .iter()
            .any(|face| face.id == material.id)
    });
    assert_eq!(
        public
            .seats
            .iter()
            .map(|seat| {
                game.seat(seat.id)
                    .unwrap()
                    .hand
                    .iter()
                    .chain(&seat.equipment)
                    .chain(&seat.judges)
                    .filter(|&&card| card == material)
                    .count()
            })
            .sum::<usize>()
            + public
                .resolving
                .iter()
                .filter(|&&card| card == material)
                .count()
            + game
                .special_cards()
                .iter()
                .filter(|&&card| card == material)
                .count()
            + public
                .discard
                .iter()
                .filter(|&&card| card == material)
                .count(),
        1,
    );
    json!({
        "material": material_position(game, material),
        "visible": visibility,
        "judges": entries.iter().map(|entry| entry.kind).collect::<Vec<_>>(),
        "rawJudges": visible_names,
        "canAddLebu": game.can_add_judge(1, CardKind::Lebu),
        "disabled": seat.judge_disabled,
        "placeholder": usize::from(seat.judge_disabled_marker),
    })
}

fn oracle_snapshot(state: &Value) -> Value {
    let names = |field: &str| {
        state[field]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| &entry["name"])
            .collect::<Vec<_>>()
    };
    json!({
        "material": state["material"],
        "visible": state["visible"],
        "judges": names("judges"),
        "rawJudges": names("rawJudges"),
        "canAddLebu": state["canAddLebu"],
        "disabled": state["disabled"],
        "placeholder": state["placeholder"],
    })
}

fn run(game: &mut Game, program: Program) {
    run_choosing(game, program, |_, choice| {
        panic!("unexpected choice {choice:?}")
    });
}

fn run_choosing(
    game: &mut Game,
    program: Program,
    mut choose: impl FnMut(&Game, &crate::ChoiceView) -> crate::Selection,
) {
    game.scheduler.start(program);
    let mut answer = None;
    while let Some(work) = game.scheduler.poll() {
        if let crate::scheduler::Work::Run(event, program, child) = work {
            game.resolve(event, program, child, &mut answer);
        }
        if let Some(decision) = game.decision.take() {
            let crate::Decision::Cards(choice) = decision else {
                panic!("unexpected play prompt");
            };
            let selection = if choice.counter.is_some() {
                crate::Selection {
                    branch: 0,
                    cards: vec![],
                    targets: vec![],
                }
            } else {
                choose(game, &choice)
            };
            answer = Some(crate::Answer::Select(selection));
        }
    }
}

fn observe_row(timing: &str) -> bool {
    timing.starts_with("gain")
        || timing.starts_with("phaseJudge")
        || timing.starts_with("disableJudge")
        || matches!(timing, "judge" | "judgeEnd")
        || timing.starts_with("discard")
        || timing.starts_with("lose")
}

#[test]
fn virtual_judgment_gain_and_late_loss_match_fixed_reference() {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/virtual-judge-probe.json"
    ))
    .unwrap();
    for trial in oracle["trials"].as_array().unwrap().iter().filter(|trial| {
        let name = trial["name"].as_str().unwrap();
        name.ends_with("phaseJudge") || name.ends_with("disableJudge")
    }) {
        let name = trial["name"].as_str().unwrap();
        let cancelled = name.starts_with("cancel-inner:");
        let owner = if name.contains("B:") { 1 } else { 2 };
        let material = card(701, CardKind::Qinggang);
        let revealed = card(702, CardKind::Sha);
        let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist], 1);
        let table = game.classic.as_mut().unwrap();
        table.deck.push(revealed);
        table.faces.extend([
            CardFace {
                id: material.id,
                suit: Suit::Spade,
                rank: 6,
            },
            CardFace {
                id: revealed.id,
                suit: Suit::Heart,
                rank: 9,
            },
        ]);
        game.put_in_judgment(1, material, CardKind::Lebu);
        let gain = game.gain_owned_card_program(1, owner, material.id).unwrap();
        if cancelled {
            run(&mut game, gain.clone());
            assert_eq!(
                snapshot(&game, material),
                oracle_snapshot(&trial["before"]),
                "{name}"
            );
        } else {
            assert_eq!(
                snapshot(&game, material),
                oracle_snapshot(&trial["beforeGain"]),
                "{name}"
            );
        }
        let rows = Arc::new(Mutex::new(Vec::new()));
        let observed = rows.clone();
        game.timing_observer = Some(Arc::new(move |game, event| {
            if observe_row(&event.timing) {
                observed
                    .lock()
                    .unwrap()
                    .push(json!([event.timing, snapshot(game, material)]));
            }
        }));
        if !cancelled {
            run(&mut game, gain);
            assert_eq!(
                snapshot(&game, material),
                oracle_snapshot(&trial["afterGain"]),
                "{name}"
            );
        } else {
            game.register_trigger(TriggerRegistration {
                owner: 0,
                skill: "cancel_judgment_loss".into(),
                timing: "loseBefore".into(),
                role: TriggerRole::Global,
                order: TriggerOrder::First,
                priority: 0,
                label: None,
                silent: true,
                filter: Arc::new(|_, _, _| true),
                effect: Arc::new(|_, _, _| TriggerStep::CancelSubject),
            })
            .unwrap();
        }
        let operation = if name.ends_with("phaseJudge") {
            Program::Phase(crate::classic::phases::Flow::new(
                1,
                Phase::Judgment,
                None,
                0,
            ))
        } else {
            Program::JudgeArea(crate::classic::judge_area::Flow::new(1, Some(0), true))
        };
        run(&mut game, operation);
        let end = if cancelled {
            &trial["after"]
        } else {
            &trial["afterNext"]
        };
        assert_eq!(snapshot(&game, material), oracle_snapshot(end), "{name}");
        assert_eq!(
            material_position(&game, revealed),
            end["revealed"].as_str(),
            "{name}"
        );
        let expected = oracle["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                row[0] == name && row[1] == "request" && observe_row(row[2].as_str().unwrap())
            })
            .map(|row| json!([row[2], oracle_snapshot(&row[8])]))
            .collect::<Vec<_>>();
        let actual = rows.lock().unwrap();
        assert_eq!(actual.len(), expected.len(), "{name} actual: {actual:?}");
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(*actual, expected, "{name}");
        }
    }
}

#[test]
fn a_new_judgment_does_not_reattach_an_old_wrapper_of_the_same_material() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist], 0);
    let material = card(701, CardKind::Sha);
    game.put_in_judgment(1, material, CardKind::Lebu);
    let existing = game.judgments(1).next().unwrap();
    let taken = game.take_card(1, Zone::Judgment, material.id);
    game.put_in_judgment(1, taken, CardKind::Shandian);

    let judgments = game.judgments(1).collect::<Vec<_>>();
    assert_eq!(judgments.len(), 2);
    assert_eq!(judgments[0].key, existing.key);
    assert!(!judgments[0].is_attached());
    assert_ne!(judgments[1].key, existing.key);
    assert!(judgments[1].is_attached());
    let public = game.view(None).unwrap();
    assert_eq!(public.seats[1].logical_judges[0].kind, CardKind::Lebu);
    assert_eq!(public.seats[1].logical_judges[0].material, None);
    assert_eq!(
        public.seats[1].logical_judges[1].material,
        Some(material.id)
    );
    assert!(game.take_judgment(1, existing).is_some());
    let remaining = game.judgments(1).next().unwrap();
    assert_eq!(remaining.key, judgments[1].key);
    assert!(!remaining.is_attached());
}

fn trick_snapshot(game: &Game, material: Card, trick: Option<Card>) -> Value {
    let snap = snapshot(game, material);
    json!({
        "material": snap["material"],
        "known": ([0, 1, 2].map(|viewer| game.is_known_by(material.id, Some(viewer)))),
        "virtual": snap["judges"],
        "raw": snap["rawJudges"],
        "aliveB": game.is_alive(1),
        "trick": trick.and_then(|card| material_position(game, card)),
    })
}

fn oracle_trick_snapshot(state: &Value) -> Value {
    let names = |field: &str| {
        state[field]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| &entry["name"])
            .collect::<Vec<_>>()
    };
    json!({
        "material": state["material"],
        "known": state["known"],
        "virtual": names("virtual"),
        "raw": names("raw"),
        "aliveB": state["aliveB"],
        "trick": state["trick"],
    })
}

#[test]
fn ordinary_tricks_select_a_single_public_wrapper_and_death_follows_its_material() {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/virtual-judge-tricks.json"
    ))
    .unwrap();
    for trial in oracle["trials"].as_array().unwrap() {
        let name = trial["name"].as_str().unwrap();
        let owner = if name.ends_with(":B") { 1 } else { 2 };
        let kind = match name.split_once(':').unwrap().0 {
            "guohe" => Some(CardKind::Guohe),
            "shunshou" => Some(CardKind::Shunshou),
            "death" => None,
            _ => unreachable!(),
        };
        let material = card(710, CardKind::Qinggang);
        let trick = kind.map(|kind| card(711, kind));
        let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist, Role::Rebel], 0);
        game.classic.as_mut().unwrap().faces.push(CardFace {
            id: material.id,
            suit: Suit::Spade,
            rank: 6,
        });
        game.put_in_judgment(1, material, CardKind::Lebu);
        if let Some(trick) = trick {
            game.seat_mut(0).hand.push(trick);
        }
        let gain = game.gain_owned_card_program(1, owner, material.id).unwrap();
        run(&mut game, gain);
        assert_eq!(
            trick_snapshot(&game, material, trick),
            oracle_trick_snapshot(&trial["before"]),
            "{name}"
        );
        let mut chose_wrapper = false;
        let operation = if let Some(trick) = trick {
            game.trick_uses(game.seat(0).unwrap(), trick)
                .unwrap()
                .into_iter()
                .find(|use_| use_.targets == [1])
                .expect("wrapper makes B a legal target")
                .program
        } else {
            game.death_program(1, None)
        };
        run_choosing(&mut game, operation, |_, choice| {
            assert!(choice
                .branches
                .iter()
                .all(|branch| !branch.cards.contains(&material.id)));
            assert_eq!(
                choice.branches.iter().any(|branch| branch.id == 0),
                owner == 1
            );
            let wrapper = choice
                .branches
                .iter()
                .find(|branch| branch.label == "乐不思蜀")
                .unwrap();
            chose_wrapper = true;
            crate::Selection {
                branch: wrapper.id,
                cards: vec![],
                targets: vec![],
            }
        });
        assert_eq!(chose_wrapper, trick.is_some());
        assert_eq!(
            trick_snapshot(&game, material, trick),
            oracle_trick_snapshot(&trial["after"]),
            "{name}"
        );
    }
}

#[test]
fn cancelling_outer_discard_keeps_the_wrapper_and_the_materials_hand_privacy() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist], 0);
    let material = card(720, CardKind::Qinggang);
    let unknown = card(721, CardKind::Sha);
    game.classic.as_mut().unwrap().faces.push(CardFace {
        id: material.id,
        suit: Suit::Spade,
        rank: 6,
    });
    game.put_in_judgment(1, material, CardKind::Lebu);
    game.seat_mut(2).hand.push(unknown);
    let gain = game.gain_owned_card_program(1, 2, material.id).unwrap();
    run(&mut game, gain);
    let give = game.give_hand_program(2, 0, unknown.id).unwrap();
    run(&mut game, give);

    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    game.register_trigger(TriggerRegistration {
        owner: 0,
        skill: "cancel_outer_judge_discard_once".into(),
        timing: "discardBefore".into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(move |_, _, _| {
            if cancelled.swap(true, std::sync::atomic::Ordering::Relaxed) {
                TriggerStep::Done
            } else {
                TriggerStep::CancelSubject
            }
        }),
    })
    .unwrap();
    let disable = Program::JudgeArea(crate::classic::judge_area::Flow::new(1, Some(0), true));
    run(&mut game, disable.clone());
    assert_eq!(material_position(&game, material), Some("C:h"));
    let observer = game.view(Some(0)).unwrap();
    assert!(observer.seats[1].judge_disabled_marker);
    assert_eq!(
        observer.seats[1].logical_judges,
        [JudgmentEntryView {
            kind: CardKind::Lebu,
            material: None
        }]
    );
    assert!(observer.seats[2].known_hand.is_empty());
    assert!(observer
        .card_faces
        .iter()
        .all(|face| face.id != material.id));

    let enable = Program::JudgeArea(crate::classic::judge_area::Flow::new(1, Some(0), false));
    run(&mut game, enable);
    assert!(!game.can_add_judge(1, CardKind::Lebu));
    run(&mut game, disable);
    assert!(game.judgments(1).next().is_none());
    assert_eq!(game.discard, [material]);
}

#[test]
fn discarding_the_raw_material_and_discarding_the_wrapper_are_distinct() {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../../../../tests/reference/virtual-judge-probe.json"
    ))
    .unwrap();
    for trial in oracle["trials"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|trial| trial["name"].as_str().unwrap().starts_with("discard:"))
    {
        let name = trial["name"].as_str().unwrap();
        let owner = if name.contains(":B:") { 1 } else { 2 };
        let material = card(730, CardKind::Qinggang);
        let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist], 0);
        game.classic.as_mut().unwrap().faces.push(CardFace {
            id: material.id,
            suit: Suit::Spade,
            rank: 6,
        });
        game.put_in_judgment(1, material, CardKind::Lebu);
        let gain = game.gain_owned_card_program(1, owner, material.id).unwrap();
        run(&mut game, gain);
        assert_eq!(
            snapshot(&game, material),
            oracle_snapshot(&trial["before"]),
            "{name}"
        );
        if name.ends_with(":wrapper") {
            let entry = game.judgments(1).next().unwrap();
            let discard = game.judgment_discard_program(1, vec![entry]);
            run(&mut game, discard);
        } else if owner == 1 {
            run(
                &mut game,
                Program::DiscardGroup {
                    seat: 1,
                    cards: vec![(Zone::Hand, material)],
                },
            );
        }
        let mut expected = oracle_snapshot(&trial["after"]);
        // The public discard view shows faces regardless of the underlying knowledge flags.
        if expected["material"] == "d" {
            assert_eq!(
                ([0, 1, 2].map(|viewer| game.is_known_by(material.id, Some(viewer)))),
                serde_json::from_value::<[bool; 3]>(expected["visible"].clone()).unwrap(),
                "{name}",
            );
            expected["visible"] = json!([true, true, true]);
        }
        assert_eq!(snapshot(&game, material), expected, "{name}");
    }
}
