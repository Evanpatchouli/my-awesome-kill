use super::*;
use crate::{ChoiceKind, ReplayLog, ReplaySetup, TemporarySkill};

const NAME: &str = "qinggang2";
const HOLDER: SeatId = 0;
const ATTACKER: SeatId = 1;
const JIEDAO: Card = Card {
    id: 6311,
    kind: CardKind::Jiedao,
};
const SHA: Card = Card {
    id: 6312,
    kind: CardKind::Sha,
};
const SWORD: Card = Card {
    id: 6313,
    kind: CardKind::Qinggang,
};
const CONTENT: &str = "active-ownership-fixture-v1";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Window {
    Normal,
    BeforeAcceptance,
    AfterAcceptance,
}

fn fixture_setup() -> ReplaySetup {
    ReplaySetup::Seeded {
        seed: 5,
        human: HOLDER,
        hero: "guanyu".into(),
        seats: 8,
    }
}

fn registration(owner: SeatId, name: &str, window: Window) -> ActiveSkillRegistration {
    ActiveSkillRegistration {
        owner,
        skill: name.into(),
        limited: false,
        choice: Arc::new(|_, _| {
            Some(ChoiceBranch {
                id: 0,
                label: "主动技".into(),
                cards: vec![],
                targets: vec![],
                min_cards: 0,
                max_cards: 0,
                min_targets: 0,
                max_targets: 0,
            })
        }),
        effect: Arc::new(move |ctx, step, _| {
            ctx.add_skill_mark(owner, "effect_steps", None, 1).unwrap();
            if window == Window::AfterAcceptance && step == 0 {
                ctx.use_virtual_sha(ATTACKER, HOLDER, None).unwrap();
                return TriggerStep::Continue(1);
            }
            TriggerStep::Done
        }),
    }
}

// Active registrations are host-managed handles. Upstream ordinary/additional
// skill sources motivate this lifetime contract but have no such handle API.
fn fixture(setup: &ReplaySetup, temp: bool, window: Window) -> Result<Game, RuleError> {
    let mut game = setup.start()?;
    for seat in &mut game.seats {
        seat.hand.clear();
        seat.hero = None;
    }
    game.seat_mut(HOLDER).hand.push(JIEDAO);
    game.seat_mut(ATTACKER).hand.push(SHA);
    game.seat_mut(ATTACKER).equipment.install_test(SWORD);
    if temp {
        game.grant_temporary(HOLDER, TemporarySkill::until_removed(NAME))?;
    }
    let first = game.register_active_skill(registration(HOLDER, NAME, window))?;
    game.register_active_skill(registration(HOLDER, NAME, Window::Normal))?;
    game.register_active_skill(registration(HOLDER, "other_active", Window::Normal))?;
    game.register_active_skill(registration(2, NAME, Window::Normal))?;
    if window == Window::BeforeAcceptance {
        let mut before = trigger(|ctx, _, _| {
            ctx.use_virtual_sha(ATTACKER, HOLDER, None).unwrap();
            TriggerStep::Done
        });
        before.filter = Arc::new(move |game, event, _| {
            matches!(game.event_kind(event.subject),
            Some(EventKind::ActivateSkill { registration, .. }) if registration == first)
        });
        game.register_trigger(before)?;
    }
    Ok(game)
}

struct Table {
    game: Game,
    commands: Vec<Command>,
    same: Vec<ActiveSkillId>,
}

impl Table {
    fn new(temp: bool, window: Window) -> Self {
        let game = fixture(&fixture_setup(), temp, window).unwrap();
        let same: Vec<_> = game
            .active_plays(HOLDER)
            .into_iter()
            .filter(|play| play.skill == NAME)
            .map(|play| play.registration)
            .collect();
        assert_eq!(same.len(), 2);
        Self {
            game,
            commands: vec![],
            same,
        }
    }

    fn submit(&mut self, seat: SeatId, action: Action) {
        let cmd = Command {
            version: self.game.version,
            seat,
            action,
        };
        self.game.submit(cmd.clone()).unwrap();
        self.commands.push(cmd);
    }

    fn start_physical_attack(&mut self) {
        self.submit(
            HOLDER,
            Action::PlayCard {
                card: JIEDAO.id,
                targets: vec![ATTACKER, HOLDER],
            },
        );
        loop {
            let seat = self.game.view(None).unwrap().waiting_for.unwrap();
            let choice = self.game.view(Some(seat)).unwrap().choice.unwrap();
            if choice.counter.is_some() {
                self.submit(
                    seat,
                    Action::Select {
                        event: choice.event,
                        branch: 0,
                        cards: vec![],
                        targets: vec![],
                    },
                );
                continue;
            }
            assert_eq!(seat, ATTACKER);
            let branch = choice
                .branches
                .iter()
                .find(|branch| branch.cards.contains(&SHA.id))
                .unwrap();
            self.submit(
                seat,
                Action::Select {
                    event: choice.event,
                    branch: branch.id,
                    cards: vec![SHA.id],
                    targets: vec![],
                },
            );
            break;
        }
        assert!(self.mark_shown());
    }

    fn mark_shown(&self) -> bool {
        self.game.view(None).unwrap().seats[HOLDER as usize]
            .marks
            .iter()
            .any(|m| m.skill == NAME)
    }

    fn pass_shan(&mut self) {
        let choice = self.game.view(Some(HOLDER)).unwrap().choice.unwrap();
        assert_eq!(choice.kind, ChoiceKind::Shan);
        self.submit(
            HOLDER,
            Action::Choose {
                event: choice.event,
                card: None,
            },
        );
    }
}

fn activate(id: ActiveSkillId) -> Action {
    Action::ActivateSkill {
        registration: id,
        cards: vec![],
        targets: vec![],
    }
}

#[test]
fn last_qinggang_token_revokes_all_matching_grants_and_replay_preserves_the_handles() {
    let mut table = Table::new(false, Window::Normal);
    table.start_physical_attack();
    table.pass_shan();
    assert!(table.game.discard.contains(&SHA));
    assert_eq!(
        table.game.seat(ATTACKER).unwrap().equipment.as_slice(),
        [SWORD]
    );
    let log = ReplayLog {
        schema: 1,
        setup: fixture_setup(),
        content: Some(CONTENT.into()),
        commands: table.commands,
    };
    let log: ReplayLog = serde_json::from_slice(&serde_json::to_vec(&log).unwrap()).unwrap();
    let mut replay = log
        .restore_with(log.commands.len(), CONTENT, |s| {
            fixture(s, false, Window::Normal)
        })
        .unwrap();
    for viewer in (0..8).map(Some).chain([None]) {
        assert_eq!(
            table.game.view(viewer).unwrap(),
            replay.view(viewer).unwrap()
        );
    }
    for game in [&mut table.game, &mut replay] {
        assert!(!game.has_skill(HOLDER, NAME));
        assert!(!game.has_active_skill(HOLDER, NAME));
        let plays = game.active_plays(HOLDER);
        assert!(plays.iter().any(|play| play.skill == "other_active"));
        assert!(game.has_active_skill(2, NAME));
        for &id in &table.same {
            assert_eq!(game.active_skill(id).unwrap().skill, NAME);
            assert!(plays.iter().all(|play| play.registration != id));
            assert_eq!(
                game.submit(Command {
                    seat: HOLDER,
                    version: game.version,
                    action: activate(id)
                }),
                Err(RuleError::InvalidSelection)
            );
            assert!(game.unregister_active_skill(id));
            assert!(game.active_skill(id).is_none());
        }
        assert!(game.has_active_skill(2, NAME));
        assert!(game.has_active_skill(HOLDER, "other_active"));
    }
}

#[test]
fn unregistering_last_active_source_during_shan_immediately_removes_sword_mark() {
    for temp in [false, true] {
        let mut table = Table::new(temp, Window::Normal);
        table.start_physical_attack();
        assert!(table.game.unregister_active_skill(table.same[0]));
        assert!(table.game.has_skill(HOLDER, NAME));
        assert!(
            table.mark_shown(),
            "second active registration still owns this skill"
        );
        assert!(table.game.unregister_active_skill(table.same[1]));
        assert_eq!(table.game.has_skill(HOLDER, NAME), temp);
        assert_eq!(
            table.mark_shown(),
            temp,
            "remaining temporary grant: {temp}"
        );
        // Leave the original waiting decision valid even if the host unregisters its last grant.
        table.pass_shan();
        assert!(!table.mark_shown());
    }
}

#[test]
fn revoke_in_use_skill_begin_prevents_acceptance_and_effect() {
    let mut table = Table::new(false, Window::BeforeAcceptance);
    table.submit(HOLDER, activate(table.same[0]));
    assert!(table.mark_shown());
    table.pass_shan();
    assert_eq!(table.game.skill_mark_count(HOLDER, "effect_steps", None), 0);
    assert_eq!(table.game.active_skill_uses_this_phase(HOLDER, NAME), 0);
    assert!(table.game.events.iter().any(|e| e.status == crate::EventStatus::Cancelled &&
        matches!(e.kind, EventKind::ActivateSkill { registration, .. } if registration == table.same[0])));
}

#[test]
fn revoke_after_acceptance_preserves_paid_use_and_effect_continuation() {
    let mut table = Table::new(false, Window::AfterAcceptance);
    table.submit(HOLDER, activate(table.same[0]));
    assert_eq!(table.game.skill_mark_count(HOLDER, "effect_steps", None), 1);
    assert_eq!(table.game.active_skill_uses_this_phase(HOLDER, NAME), 1);
    table.pass_shan();
    assert_eq!(table.game.skill_mark_count(HOLDER, "effect_steps", None), 2);
    assert_eq!(table.game.active_skill_uses_this_phase(HOLDER, NAME), 1);
    assert!(table.game.events.iter().any(|e| e.status == crate::EventStatus::Finished &&
        matches!(e.kind, EventKind::ActivateSkill { registration, .. } if registration == table.same[0])));
    assert!(!table.game.has_active_skill(HOLDER, NAME));
}
