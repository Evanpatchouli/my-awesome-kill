use super::*;

fn hook(timing: &str, skill: &str) -> TriggerRegistration {
    TriggerRegistration {
        owner: 0,
        skill: skill.into(),
        timing: timing.into(),
        role: TriggerRole::Global,
        order: TriggerOrder::First,
        priority: 0,
        label: None,
        silent: true,
        filter: Arc::new(|_, _, _| true),
        effect: Arc::new(|_, _, _| TriggerStep::Done),
    }
}

fn give_case(
    public: bool,
    interruption: Option<&'static str>,
) -> (Game, Vec<(String, bool, bool)>) {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist], 0);
    let material = card(1, CardKind::Sha);
    let known = card(2, CardKind::Tao);
    game.seat_mut(0).hand.extend([material, known]);
    game.known_to_all.insert(known.id);
    if public {
        game.known_to_all.insert(material.id);
    }
    let mut give = hook("phaseUseBegin", "give");
    give.role = TriggerRole::Player;
    give.effect = Arc::new(move |ctx, _, _| {
        ctx.give_hand(0, 1, material.id).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(give).unwrap();
    let rows = Arc::new(Mutex::new(Vec::new()));
    for timing in ["loseBefore", "loseAfter"] {
        let out = rows.clone();
        let mut observe = hook(timing, &format!("observe_{timing}"));
        observe.filter = Arc::new(move |game, event, _| {
            event.cards.contains(&material)
                && event
                    .parent
                    .and_then(|id| game.scheduler.active_frame(id))
                    .is_some_and(|(_, p)| matches!(p.kind(), EventKind::Gain { seat: 1, .. }))
        });
        observe.effect = Arc::new(move |ctx, _, _| {
            out.lock().unwrap().push((
                ctx.event().timing.clone(),
                ctx.game().is_known_by(known.id, Some(2)),
                ctx.game().is_known_by(known.id, Some(0)),
            ));
            TriggerStep::Done
        });
        game.register_trigger(observe).unwrap();
    }
    if let Some(mode) = interruption {
        let mut interrupt = hook("loseBefore", "interrupt");
        interrupt.order = TriggerOrder::Normal;
        interrupt.filter = Arc::new(move |game, event, _| {
            event.cards.contains(&material)
                && event
                    .parent
                    .and_then(|id| game.scheduler.active_frame(id))
                    .is_some_and(|(_, p)| matches!(p.kind(), EventKind::Gain { seat: 1, .. }))
        });
        interrupt.effect = Arc::new(move |ctx, _, _| match mode {
            "cancel" => TriggerStep::CancelSubject,
            "intercept" => {
                ctx.discard_hand(0, &[material.id]).unwrap();
                TriggerStep::Done
            }
            _ => unreachable!(),
        });
        game.register_trigger(interrupt).unwrap();
    }
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    let observed = rows.lock().unwrap().clone();
    (game, observed)
}

#[test]
fn private_give_obscures_remaining_hand_only_after_the_actual_loss() {
    let (game, rows) = give_case(false, None);
    assert_eq!(
        rows,
        [
            ("loseBefore".into(), true, true),
            ("loseAfter".into(), false, true)
        ]
    );
    assert!(!game.is_known_by(2, Some(2)));
    assert!(game.is_known_by(2, Some(0)));
    assert!(game.is_known_by(1, Some(1)));
    assert!(!game.is_known_by(1, Some(2)));
}

#[test]
fn cancelled_or_intercepted_private_give_does_not_obscure_remaining_cards() {
    for mode in ["cancel", "intercept"] {
        let (game, rows) = give_case(false, Some(mode));
        assert_eq!(rows[0], ("loseBefore".into(), true, true));
        assert!(game.is_known_by(2, Some(2)), "{mode}");
        assert!(!game.seat(1).unwrap().hand.iter().any(|c| c.id == 1));
    }
}

#[test]
fn public_give_keeps_knowledge_of_both_transferred_and_remaining_cards() {
    let (game, rows) = give_case(true, None);
    assert_eq!(
        rows,
        [
            ("loseBefore".into(), true, true),
            ("loseAfter".into(), true, true)
        ]
    );
    assert!(game.is_known_by(1, Some(2)));
    assert!(game.is_known_by(2, Some(2)));
}

#[test]
fn visible_hand_discard_keeps_other_previously_known_cards() {
    let mut game = scene(&[Role::Lord, Role::Rebel, Role::Loyalist], 0);
    let discarded = card(1, CardKind::Sha);
    let known = card(2, CardKind::Tao);
    game.seat_mut(0).hand.extend([discarded, known]);
    game.known_to_all.insert(known.id);
    let mut discard = hook("phaseUseBegin", "discard_visible");
    discard.role = TriggerRole::Player;
    discard.effect = Arc::new(move |ctx, _, _| {
        ctx.discard_hand(0, &[discarded.id]).unwrap();
        TriggerStep::Done
    });
    game.register_trigger(discard).unwrap();
    game.scheduler.start(Program::event(EventKind::Phase {
        seat: 0,
        phase: Phase::Play,
    }));
    game.run(None);
    assert!(game.discard.contains(&discarded));
    assert!(game.is_known_by(known.id, Some(2)));
}

#[test]
fn visible_recast_give_preserves_other_known_cards() {
    let mut game = play_scene();
    let material = card(1, CardKind::Sha);
    let known = card(2, CardKind::Tao);
    game.seat_mut(0).hand.extend([material, known]);
    game.known_to_all.insert(known.id);
    let mut rule = RecastRule::default();
    rule.loss = Arc::new(|_, _, _| RecastLoss::Give { to: 1 });
    game.register_recast(RecastKey::Card(material.id), rule);
    game.submit(Command {
        version: game.version,
        seat: 0,
        action: Action::Recast { card: material.id },
    })
    .unwrap();
    assert!(game.seat(1).unwrap().hand.contains(&material));
    assert!(game.is_known_by(material.id, Some(2)));
    assert!(game.is_known_by(known.id, Some(2)));
}
