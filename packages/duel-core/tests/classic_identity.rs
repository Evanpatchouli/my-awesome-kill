use std::collections::HashSet;

use duel_core::classic_data::{self, Role};
use duel_core::{
    Action, CardKind, ChoiceKind, Command, DrawReason, EventKind, EventStatus, Game, GameView,
    Phase, RuleError,
};

fn command(view: &GameView, action: Action) -> Command {
    Command {
        version: view.version,
        seat: view.waiting_for.unwrap(),
        action,
    }
}

fn discard_action(view: &GameView) -> Action {
    let choice = view.choice.as_ref().unwrap();
    Action::Discard {
        event: choice.event,
        cards: choice.cards[..choice.min].to_vec(),
    }
}

#[test]
fn real_opening_deals_a_unique_roster_and_separates_each_viewer() {
    let game = Game::classic(0, 2, "liubei", 6).unwrap();
    let public = game.view(None).unwrap();
    let lord = public.active_seat;
    assert_eq!(public.version, 0);
    assert_eq!(public.human_seat, Some(2));
    assert_eq!(public.deck_count, 104 - 6 * 4 - 2);
    assert_eq!(public.waiting_for, Some(lord));
    assert_eq!(public.turn.unwrap().phase, Phase::Play);
    assert_eq!(public.turn.unwrap().round, 1);
    assert_eq!(public.turn.unwrap().number, 1);
    assert!(public.actions.is_empty());
    assert!(public.card_faces.is_empty());

    let mut generals = HashSet::new();
    let mut cards = HashSet::new();
    for seat in &public.seats {
        assert!(generals.insert(seat.hero.clone().unwrap()));
        assert!(seat.hand.is_none());
        assert_eq!(seat.role, (seat.id == lord).then_some(Role::Lord));
        let base = classic_data::roster()
            .iter()
            .find(|hero| Some(&hero.id) == seat.hero.as_ref())
            .unwrap()
            .max_hp;
        assert_eq!(seat.max_hp, base + u16::from(seat.id == lord));
        assert_eq!(seat.hand_count, if seat.id == lord { 6 } else { 4 });

        let perspective = game.view(Some(seat.id)).unwrap();
        let own = &perspective.seats[seat.id as usize];
        assert!(own.role.is_some());
        assert_eq!(perspective.card_faces.len(), seat.hand_count);
        assert_eq!(perspective.actions.is_empty(), seat.id != lord);
        for card in own.hand.as_ref().unwrap() {
            assert!(cards.insert(card.id));
            assert!(perspective.card_faces.iter().any(|face| face.id == card.id));
        }
    }
    assert_eq!(public.seats[2].hero.as_deref(), Some("liubei"));
    let opening_draws = public
        .events
        .iter()
        .filter_map(|event| match (event.status, event.kind) {
            (
                EventStatus::Started,
                EventKind::Draw {
                    seat,
                    amount: 4,
                    reason: DrawReason::Opening,
                },
            ) => Some(seat),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(opening_draws, game.seat_order_from(lord).unwrap());
    assert_eq!(
        Game::classic(0, 2, "liubei", 6)
            .unwrap()
            .view(Some(2))
            .unwrap(),
        game.view(Some(2)).unwrap()
    );
}

#[test]
fn discarding_is_versioned_and_atomic_across_a_complete_round() {
    let mut game = Game::classic(9, 0, "liubei", 4).unwrap();
    let initial = game.view(None).unwrap();
    let lord = initial.active_seat;
    let order = game.seat_order_from(lord).unwrap();
    let mut events = initial.events;

    for (index, seat) in order.iter().copied().enumerate() {
        let playing = game.view(Some(seat)).unwrap();
        assert_eq!(playing.active_seat, seat);
        assert_eq!(playing.turn.unwrap().phase, Phase::Play);
        game.submit(command(&playing, Action::EndPlay)).unwrap();

        let discarding = game.view(Some(seat)).unwrap();
        events.extend(&discarding.events);
        let choice = discarding.choice.as_ref().unwrap();
        assert_eq!(choice.kind, ChoiceKind::Discard);
        assert!(choice.min >= 2);
        assert_eq!(choice.min, choice.max);
        assert!(game.view(None).unwrap().choice.is_none());
        assert!(game
            .view(Some(order[(index + 1) % order.len()]))
            .unwrap()
            .choice
            .is_none());

        let bad = Action::Discard {
            event: choice.event,
            cards: vec![choice.cards[0]; choice.min],
        };
        assert_eq!(
            game.submit(command(&discarding, bad)),
            Err(RuleError::InvalidSelection)
        );
        assert_eq!(game.view(Some(seat)).unwrap(), discarding);
        let mut stale = command(&discarding, discard_action(&discarding));
        stale.version -= 1;
        assert_eq!(
            game.submit(stale),
            Err(RuleError::WrongVersion {
                expected: discarding.version,
                received: playing.version
            })
        );
        assert_eq!(game.view(Some(seat)).unwrap(), discarding);

        game.submit(command(&discarding, discard_action(&discarding)))
            .unwrap();
        let next = game.view(Some(seat)).unwrap();
        events.extend(&next.events);
        assert_eq!(
            next.seats[seat as usize].hand_count,
            next.seats[seat as usize].hp as usize
        );
        assert_eq!(next.active_seat, order[(index + 1) % order.len()]);
    }
    let public = game.view(None).unwrap();
    assert_eq!(public.turn.unwrap().number, 5);
    assert_eq!(public.turn.unwrap().round, 2);
    let phases = events
        .iter()
        .filter_map(|event| match (event.status, event.kind) {
            (EventStatus::Started, EventKind::Phase { seat, phase }) if seat == lord => Some(phase),
            _ => None,
        })
        .take(6)
        .collect::<Vec<_>>();
    assert_eq!(
        phases,
        [
            Phase::Preparation,
            Phase::Judgment,
            Phase::Draw,
            Phase::Play,
            Phase::Discard,
            Phase::End
        ]
    );
}

#[test]
fn only_in_range_attacks_appear_and_sha_is_limited_to_once_per_turn() {
    let mut game = (0..64)
        .find_map(|seed| {
            let game = Game::classic(seed, 0, "liubei", 4).unwrap();
            let view = game
                .view(Some(game.view(None).unwrap().active_seat))
                .unwrap();
            let hand = view.seats[view.active_seat as usize].hand.as_ref().unwrap();
            let enough = hand
                .iter()
                .filter(|card| card.kind == CardKind::Sha)
                .count()
                >= 2;
            enough.then_some(game)
        })
        .unwrap();
    let lord = game.view(None).unwrap().active_seat;
    let order = game.seat_order_from(lord).unwrap();
    let play = game.view(Some(lord)).unwrap();
    let hand = play.seats[lord as usize].hand.as_ref().unwrap();
    let sha = hand
        .iter()
        .filter(|card| card.kind == CardKind::Sha)
        .collect::<Vec<_>>();
    let play_card = |id, target| Action::PlayCard {
        card: id,
        targets: vec![target],
    };
    assert_eq!(game.distance(lord, order[2]), Ok(2));
    assert_eq!(game.distance(lord, order[3]), Ok(1));
    assert!(play.actions.contains(&play_card(sha[0].id, order[1])));
    assert!(play.actions.contains(&play_card(sha[0].id, order[3])));
    assert!(!play.actions.contains(&play_card(sha[0].id, order[2])));
    assert_eq!(
        game.submit(command(&play, play_card(sha[0].id, order[2]))),
        Err(RuleError::InvalidTarget)
    );
    assert_eq!(game.view(Some(lord)).unwrap(), play);
    game.submit(command(&play, play_card(sha[0].id, order[1])))
        .unwrap();

    let response = game.view(Some(order[1])).unwrap();
    assert_eq!(response.choice.as_ref().unwrap().kind, ChoiceKind::Shan);
    game.submit(command(
        &response,
        Action::Choose {
            event: response.choice.as_ref().unwrap().event,
            card: None,
        },
    ))
    .unwrap();
    let play = game.view(Some(lord)).unwrap();
    assert!(!play.actions.contains(&play_card(sha[1].id, order[1])));
    assert_eq!(
        game.submit(command(&play, play_card(sha[1].id, order[1]))),
        Err(RuleError::InvalidTarget)
    );
}

#[test]
fn ordinary_tao_is_available_to_a_damaged_actor_and_recovers_only_that_actor() {
    let (mut game, lord, target, sha) = (0..64)
        .find_map(|seed| {
            let game = Game::classic(seed, 0, "liubei", 2).unwrap();
            let lord = game.view(None).unwrap().active_seat;
            let target = 1 - lord;
            let view = game.view(Some(lord)).unwrap();
            let sha = view.seats[lord as usize]
                .hand
                .as_ref()?
                .iter()
                .find(|card| card.kind == CardKind::Sha)?
                .id;
            let other = game.view(Some(target)).unwrap();
            other.seats[target as usize]
                .hand
                .as_ref()?
                .iter()
                .find(|card| card.kind == CardKind::Tao)?;
            Some((game, lord, target, sha))
        })
        .unwrap();
    let initial = game.view(Some(lord)).unwrap();
    game.submit(command(
        &initial,
        Action::PlayCard {
            card: sha,
            targets: vec![target],
        },
    ))
    .unwrap();
    let defense = game.view(Some(target)).unwrap();
    game.submit(command(
        &defense,
        Action::Choose {
            event: defense.choice.as_ref().unwrap().event,
            card: None,
        },
    ))
    .unwrap();

    let playing = game.view(Some(lord)).unwrap();
    game.submit(command(&playing, Action::EndPlay)).unwrap();
    let next = game.view(Some(lord)).unwrap();
    if next.choice.is_some() {
        game.submit(command(&next, discard_action(&next))).unwrap();
    }

    let wounded = game.view(Some(target)).unwrap();
    let player = &wounded.seats[target as usize];
    let tao = player
        .hand
        .as_ref()
        .unwrap()
        .iter()
        .find(|card| card.kind == CardKind::Tao)
        .unwrap();
    let use_tao = Action::PlayCard {
        card: tao.id,
        targets: vec![target],
    };
    assert_eq!(player.hp + 1, i32::from(player.max_hp));
    assert!(wounded.actions.contains(&use_tao));
    assert_eq!(
        game.submit(command(
            &wounded,
            Action::PlayCard {
                card: tao.id,
                targets: vec![lord]
            }
        )),
        Err(RuleError::InvalidTarget)
    );
    game.submit(command(&wounded, use_tao)).unwrap();

    let healed = game.view(Some(target)).unwrap();
    assert_eq!(healed.seats[target as usize].hp, i32::from(player.max_hp));
    assert!(healed.discard.contains(tao));
    let held = healed.seats[target as usize].hand.as_ref().unwrap();
    assert!(!healed.actions.iter().any(|action| matches!(action,
        Action::PlayCard { card, .. } if held.iter().any(|held| held.id == *card && held.kind == CardKind::Tao))));
}

#[test]
fn seeded_identity_games_reach_terminal_and_conserve_physical_cards() {
    for (count, seed) in [(2, 0), (4, 12), (8, 56)] {
        let mut game = Game::classic(seed, 0, "liubei", count).unwrap();
        let mut records = 0;
        for _ in 0..1_500 {
            let public = game.view(None).unwrap();
            records += public.events.len();
            assert!(
                public.events.len() <= 60,
                "one batch grew to {} records",
                public.events.len()
            );
            assert!(serde_json::to_vec(&public).unwrap().len() < 16 * 1024);
            if public.winner.is_some() || public.drawn {
                break;
            }
            let actor = public.waiting_for.unwrap();
            let perspective = game.view(Some(actor)).unwrap();
            let chosen = game.bot_command().unwrap_or_else(|| {
                assert_eq!(actor, 0);
                let action = match perspective.choice.as_ref() {
                    Some(choice) if choice.kind == ChoiceKind::Discard => {
                        discard_action(&perspective)
                    }
                    Some(choice) if choice.kind == ChoiceKind::Content => {
                        let branch = &choice.branches[0];
                        Action::Select {
                            event: choice.event,
                            branch: branch.id,
                            cards: branch
                                .cards
                                .iter()
                                .take(branch.min_cards)
                                .copied()
                                .collect(),
                            targets: branch
                                .targets
                                .iter()
                                .take(branch.min_targets)
                                .copied()
                                .collect(),
                        }
                    }
                    Some(choice) => Action::Choose {
                        event: choice.event,
                        card: None,
                    },
                    None => Action::EndPlay,
                };
                command(&perspective, action)
            });
            game.submit(chosen).unwrap();
        }
        let ended = game.view(None).unwrap();
        assert!(
            ended.winner.is_some() || ended.drawn,
            "unfinished {count}-seat game at seed {seed}"
        );
        assert!(ended.waiting_for.is_none());
        assert!(ended.resolving.is_empty());
        assert!(ended
            .seats
            .iter()
            .all(|seat| seat.role.is_some() && seat.hand.is_none()));
        assert_eq!(
            ended.events.last().unwrap().kind,
            EventKind::GameOver {
                winner: ended.winner
            }
        );
        assert!(records > ended.events.len() * 4);

        let mut ids = HashSet::new();
        for seat in &ended.seats {
            let perspective = game.view(Some(seat.id)).unwrap();
            let own = &perspective.seats[seat.id as usize];
            for card in own
                .hand
                .iter()
                .flatten()
                .chain(&own.equipment)
                .chain(&own.judges)
            {
                assert!(ids.insert(card.id));
            }
        }
        for card in &ended.discard {
            assert!(ids.insert(card.id));
        }
        assert_eq!(ids.len() + ended.deck_count, 104);
        assert_eq!(
            game.submit(Command {
                version: ended.version,
                seat: ended.active_seat,
                action: Action::EndPlay
            }),
            Err(RuleError::GameOver)
        );
        assert_eq!(game.view(None).unwrap(), ended);
    }
}
