use super::*;

/// Cards physically lost by a single batch lose event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LossRecord {
    pub event: EventId,
    pub seat: SeatId,
    pub stockcards: Vec<Card>,
    pub cards: Vec<(Zone, Card)>,
}

#[derive(Clone)]
pub(crate) struct BatchMovement {
    seat: SeatId,
    kind: BatchKind,
    stage: MoveStage,
    cards: Arc<[Card]>,
    judgment_discard_targets: Arc<[JudgmentDiscardTarget]>,
    force_dead: bool,
    committed: Vec<Card>,
    losses: Vec<(Zone, Card)>,
    stranded: bool,
}

#[derive(Clone, Copy)]
enum BatchKind {
    Discard,
    JudgeDiscard,
    Lose {
        destination: BatchDestination,
        from_judgment_discard: bool,
    },
    Draw {
        amount: u16,
        reason: DrawReason,
    },
    GainPile(CardZone),
    GainDrawn,
    GainGive {
        from: SeatId,
        visible: bool,
    },
}

#[derive(Clone, Copy)]
enum BatchDestination {
    Discard,
    Transfer { to: SeatId, visible: bool },
}

#[derive(Clone, Copy)]
enum JudgmentDiscardTarget {
    Judgment(Judgment),
    Held(Card),
}

impl JudgmentDiscardTarget {
    fn material(self) -> Card {
        match self {
            Self::Judgment(entry) => entry.material,
            Self::Held(card) => card,
        }
    }
}

impl BatchMovement {
    fn new(seat: SeatId, kind: BatchKind, cards: Arc<[Card]>) -> Self {
        Self {
            seat,
            kind,
            cards,
            judgment_discard_targets: Arc::from([]),
            force_dead: false,
            stage: MoveStage::Before,
            committed: Vec::new(),
            losses: Vec::new(),
            stranded: false,
        }
    }
    pub(super) fn discard(seat: SeatId, cards: Arc<[Card]>) -> Self {
        Self::new(seat, BatchKind::Discard, cards)
    }
    pub(super) fn draw(seat: SeatId, amount: u16, reason: DrawReason) -> Self {
        Self::new(seat, BatchKind::Draw { amount, reason }, Arc::from([]))
    }
    pub(super) fn gain_from_pile(seat: SeatId, pile: CardZone, cards: Arc<[Card]>) -> Self {
        Self::new(seat, BatchKind::GainPile(pile), cards)
    }
    pub(super) fn give(from: SeatId, to: SeatId, cards: Arc<[Card]>, visible: bool) -> Self {
        Self::new(to, BatchKind::GainGive { from, visible }, cards)
    }
    fn source(&self) -> Option<SeatId> {
        match self.kind {
            BatchKind::Draw { .. } => Some(self.seat),
            BatchKind::GainGive { from, .. } => Some(from),
            _ => None,
        }
    }
    pub(super) fn event_kind(&self) -> EventKind {
        let seat = self.seat;
        let amount = self.cards.len();
        let finished = matches!(
            self.stage,
            MoveStage::Finish | MoveStage::FinishCancelled(_)
        );
        match self.kind {
            BatchKind::Discard => EventKind::LoseToDiscardpile { seat, amount },
            BatchKind::JudgeDiscard => EventKind::DiscardGroup { seat, amount },
            BatchKind::Lose { .. } => EventKind::Lose {
                seat,
                amount: if finished {
                    self.committed.len()
                } else {
                    amount
                },
            },
            BatchKind::Draw { amount, reason } => EventKind::Draw {
                seat,
                amount: if finished {
                    self.committed.len() as u16
                } else {
                    amount
                },
                reason,
            },
            kind => EventKind::Gain {
                seat,
                source: self.source(),
                amount: if finished {
                    self.committed.len()
                } else {
                    amount
                },
                card: match (kind, self.committed.as_slice()) {
                    (
                        BatchKind::GainPile(_) | BatchKind::GainGive { visible: true, .. },
                        [card],
                    ) if finished => Some(*card),
                    _ => None,
                },
            },
        }
    }
    fn hook(&self, stage: MoveStage) -> &'static str {
        let names = match self.kind {
            BatchKind::Discard => [
                "loseToDiscardpileBefore",
                "loseToDiscardpileBegin",
                "loseToDiscardpileEnd",
                "loseToDiscardpileAfter",
                "loseToDiscardpileCancelled",
            ],
            BatchKind::JudgeDiscard => [
                "discardBefore",
                "discardBegin",
                "discardEnd",
                "discardAfter",
                "discardCancelled",
            ],
            BatchKind::Lose { .. } => [
                "loseBefore",
                "loseBegin",
                "loseEnd",
                "loseAfter",
                "loseCancelled",
            ],
            BatchKind::Draw { .. } => [
                "drawBefore",
                "drawBegin",
                "drawEnd",
                "drawAfter",
                "drawCancelled",
            ],
            _ => [
                "gainBefore",
                "gainBegin",
                "gainEnd",
                "gainAfter",
                "gainCancelled",
            ],
        };
        match stage {
            MoveStage::Before => names[0],
            MoveStage::Begin => names[1],
            MoveStage::End => names[2],
            MoveStage::After => names[3],
            MoveStage::Cancel(_) => names[4],
            _ => unreachable!(),
        }
    }
}

impl Game {
    pub(crate) fn judgment_discard_program(&self, seat: SeatId, entries: Vec<Judgment>) -> Program {
        self.judgment_discard(seat, entries, Vec::new(), false)
    }

    pub(crate) fn death_judgment_discard_program(
        &self,
        seat: SeatId,
        entries: Vec<Judgment>,
        held: Vec<Card>,
    ) -> Program {
        self.judgment_discard(seat, entries, held, true)
    }

    fn judgment_discard(
        &self,
        seat: SeatId,
        entries: Vec<Judgment>,
        held: Vec<Card>,
        force_dead: bool,
    ) -> Program {
        let targets = held
            .into_iter()
            .map(JudgmentDiscardTarget::Held)
            .chain(entries.into_iter().map(JudgmentDiscardTarget::Judgment))
            .collect::<Vec<_>>();
        let cards = targets
            .iter()
            .map(|target| target.material())
            .collect::<Vec<_>>();
        let mut discard = BatchMovement::new(seat, BatchKind::JudgeDiscard, Arc::from(cards));
        discard.judgment_discard_targets = Arc::from(targets);
        discard.force_dead = force_dead;
        Program::Recast(Flow::BatchMovement(discard))
    }

    fn batch_remove_transfer(&mut self, card: Card) -> Option<Card> {
        self.transferring
            .iter()
            .position(|c| *c == card)
            .map(|i| self.transferring.remove(i))
    }
    fn batch_restore_drawn(&mut self, cards: &[Card]) {
        for &card in cards.iter().rev() {
            let card = Self::remove_card(&mut self.reserved_draws, card.id);
            self.classic.as_mut().unwrap().deck.push(card);
        }
    }
    pub(super) fn resolve_batch_movement(
        &mut self,
        event: EventRef,
        mut movement: BatchMovement,
        mut child: Option<Outcome<Resolution>>,
    ) {
        use MoveStage::*;
        let seat = movement.seat;
        if matches!(child, Some(Outcome::Cancelled))
            && !matches!(movement.stage, FinishCancelled(_))
        {
            movement.stage = Cancel(if matches!(movement.stage, ChildMoved) {
                MoveCancellation::InvalidMovement
            } else {
                MoveCancellation::Trigger
            });
        }
        loop {
            let stage = movement.stage;
            let queued = match stage {
                Before | Begin | End | After | Cancel(_) => {
                    movement.stage = match stage {
                        Before => Begin,
                        Begin => Move,
                        End => After,
                        After => Finish,
                        Cancel(reason) => FinishCancelled(reason),
                        _ => unreachable!(),
                    };
                    if matches!(stage, Cancel(_))
                        && movement.committed.is_empty()
                        && matches!(movement.kind, BatchKind::GainDrawn)
                    {
                        self.batch_restore_drawn(&movement.cards);
                    }
                    let settled = !movement.stranded
                        && matches!(stage, End | After)
                        && matches!(
                            movement.kind,
                            BatchKind::Lose { .. }
                                | BatchKind::GainPile(_)
                                | BatchKind::GainDrawn
                                | BatchKind::GainGive { .. }
                        );
                    let cards = if settled {
                        movement.committed.as_slice()
                    } else {
                        movement.cards.as_ref()
                    };
                    let amount = if matches!(movement.kind, BatchKind::Draw { .. }) && !settled {
                        match movement.kind {
                            BatchKind::Draw { amount, .. } => amount,
                            _ => unreachable!(),
                        }
                    } else {
                        cards.len() as u16
                    };
                    if matches!(stage, After)
                        && matches!(movement.kind, BatchKind::Lose { .. })
                        && self.queue_hero_losses(seat, movement.losses.clone())
                    {
                        true
                    } else {
                        self.queue_timing(
                            movement.hook(stage),
                            Some(seat),
                            movement.source(),
                            None,
                            amount,
                            cards,
                        )
                    }
                }
                Move => {
                    let forced_judgment_loss = matches!(
                        movement.kind,
                        BatchKind::Lose {
                            from_judgment_discard: true,
                            ..
                        }
                    ) && !self.is_out(seat);
                    if !self.is_in(seat)
                        && !forced_judgment_loss
                        && !(movement.force_dead && !self.is_alive(seat))
                    {
                        if matches!(movement.kind, BatchKind::JudgeDiscard) {
                            movement.stage = End;
                            continue;
                        }
                        movement.stage = Cancel(MoveCancellation::InvalidMovement);
                        continue;
                    }
                    let next = match movement.kind {
                        BatchKind::Discard => Some(BatchMovement::new(
                            seat,
                            BatchKind::Lose {
                                destination: BatchDestination::Discard,
                                from_judgment_discard: false,
                            },
                            movement.cards.clone(),
                        )),
                        BatchKind::JudgeDiscard => {
                            let targets = movement
                                .judgment_discard_targets
                                .iter()
                                .copied()
                                .filter(|&target| match target {
                                    JudgmentDiscardTarget::Judgment(entry) => {
                                        self.can_discard_judgment_snapshot(seat, entry)
                                    }
                                    JudgmentDiscardTarget::Held(card) => self
                                        .held_card_location(card.id)
                                        .is_some_and(|(owner, _, _)| owner == seat),
                                })
                                .collect::<Vec<_>>();
                            if targets.is_empty() {
                                movement.stage = Discarded;
                                continue;
                            }
                            let cards = targets
                                .iter()
                                .map(|target| target.material())
                                .collect::<Vec<_>>();
                            let mut loss = BatchMovement::new(
                                seat,
                                BatchKind::Lose {
                                    destination: BatchDestination::Discard,
                                    from_judgment_discard: true,
                                },
                                Arc::from(cards),
                            );
                            loss.judgment_discard_targets = Arc::from(targets);
                            Some(loss)
                        }
                        BatchKind::Draw { amount, .. } => {
                            match self.draw_from_piles(amount as usize) {
                                Some(cards) => {
                                    self.reserved_draws.extend(cards.iter().copied());
                                    Some(BatchMovement::new(
                                        seat,
                                        BatchKind::GainDrawn,
                                        Arc::from(cards),
                                    ))
                                }
                                None => {
                                    movement.stage = Cancel(MoveCancellation::InvalidMovement);
                                    continue;
                                }
                            }
                        }
                        BatchKind::GainGive { from, visible } => Some(BatchMovement::new(
                            from,
                            BatchKind::Lose {
                                destination: BatchDestination::Transfer { to: seat, visible },
                                from_judgment_discard: false,
                            },
                            movement.cards.clone(),
                        )),
                        BatchKind::Lose {
                            destination,
                            from_judgment_discard,
                        } => {
                            if matches!(destination, BatchDestination::Transfer { to, .. } if !self.is_in(to))
                            {
                                movement.stage = Cancel(MoveCancellation::InvalidMovement);
                                continue;
                            }
                            if from_judgment_discard {
                                for &target in movement.judgment_discard_targets.iter() {
                                    let taken = match target {
                                        JudgmentDiscardTarget::Judgment(entry) => {
                                            self.take_judgment_for_discard(seat, entry)
                                        }
                                        JudgmentDiscardTarget::Held(card) => self
                                            .held_card_location(card.id)
                                            .and_then(|(owner, zone, _)| {
                                                (owner == seat).then(|| {
                                                    (zone, self.take_card(seat, zone, card.id))
                                                })
                                            }),
                                    };
                                    if let Some((zone, taken)) = taken {
                                        self.discard.push(taken);
                                        if !movement.committed.contains(&taken) {
                                            movement.committed.push(taken);
                                            movement.losses.push((zone, taken));
                                        }
                                    }
                                }
                                self.loss_history.push(LossRecord {
                                    event: event.id,
                                    seat,
                                    stockcards: movement.cards.to_vec(),
                                    cards: movement.losses.clone(),
                                });
                                movement.stage = End;
                                continue;
                            }
                            let mut hidden_hand = false;
                            for &card in movement.cards.iter() {
                                let zone = self.seat(seat).ok().and_then(|s| {
                                    if s.hand.contains(&card) {
                                        Some(Zone::Hand)
                                    } else if s.equipment.contains(&card) {
                                        Some(Zone::Equipment)
                                    } else {
                                        None
                                    }
                                });
                                if let Some(zone) = zone {
                                    let taken = self.take_card(seat, zone, card.id);
                                    if matches!(
                                        destination,
                                        BatchDestination::Transfer { visible: false, .. }
                                    ) && matches!(zone, Zone::Hand)
                                    {
                                        hidden_hand = true;
                                    }
                                    match destination {
                                        BatchDestination::Discard => self.discard.push(taken),
                                        BatchDestination::Transfer { visible, .. } => {
                                            if visible || zone == Zone::Equipment {
                                                self.known_to_all.insert(taken.id);
                                            }
                                            self.transferring.push(taken);
                                        }
                                    }
                                    movement.committed.push(taken);
                                    movement.losses.push((zone, taken));
                                }
                            }
                            if hidden_hand {
                                self.obscure_remaining_hand(seat);
                            }
                            self.loss_history.push(LossRecord {
                                event: event.id,
                                seat,
                                stockcards: movement.cards.to_vec(),
                                cards: movement.losses.clone(),
                            });
                            movement.stage = End;
                            None
                        }
                        BatchKind::GainPile(source) => {
                            let pile = match source {
                                CardZone::DrawPile => &mut self.classic.as_mut().unwrap().deck,
                                CardZone::DiscardPile => &mut self.discard,
                                _ => unreachable!("a public gain has a pile source"),
                            };
                            let valid = movement.cards.iter().all(|card| pile.contains(card));
                            if !valid {
                                movement.stage = Cancel(MoveCancellation::InvalidMovement);
                                continue;
                            }
                            for &card in movement.cards.iter() {
                                let taken = Self::remove_card(pile, card.id);
                                self.known_to_all.insert(taken.id);
                                movement.committed.push(taken);
                            }
                            self.seat_mut(seat)
                                .hand
                                .splice(..0, movement.committed.iter().copied());
                            movement.stage = End;
                            None
                        }
                        BatchKind::GainDrawn => {
                            for &card in movement.cards.iter() {
                                let taken = Self::remove_card(&mut self.reserved_draws, card.id);
                                movement.committed.push(taken);
                            }
                            self.seat_mut(seat)
                                .hand
                                .splice(..0, movement.committed.iter().copied());
                            movement.stage = End;
                            None
                        }
                    };
                    if let Some(next) = next {
                        movement.stage = ChildMoved;
                        self.scheduler
                            .next(Program::Recast(Flow::BatchMovement(next)));
                        true
                    } else {
                        false
                    }
                }
                ChildMoved => {
                    let Some(Outcome::Finished(Resolution::BatchMove { cards, .. })) = child.take()
                    else {
                        movement.stage = Cancel(MoveCancellation::InvalidMovement);
                        continue;
                    };
                    if let BatchKind::GainGive { from, .. } = movement.kind {
                        // The gain's recipient must still be participating.
                        // The earlier loss already committed the material to special.
                        if !self.is_in(seat) {
                            movement.stranded = true;
                            movement.stage = End;
                            continue;
                        }
                        for card in cards {
                            if let Some(taken) = self.batch_remove_transfer(card) {
                                if !self.known_to_all.contains(&taken.id) {
                                    self.mark_known_to(taken.id, from);
                                }
                                movement.committed.push(taken);
                            }
                        }
                        self.seat_mut(seat)
                            .hand
                            .splice(..0, movement.committed.iter().copied());
                    } else {
                        movement.committed = cards;
                    }
                    movement.stage =
                        if matches!(movement.kind, BatchKind::Discard | BatchKind::JudgeDiscard) {
                            Discarded
                        } else {
                            End
                        };
                    false
                }
                Discarded => {
                    movement.stage = End;
                    self.queue_timing(
                        if matches!(movement.kind, BatchKind::JudgeDiscard) {
                            "discard"
                        } else {
                            "loseToDiscardpile"
                        },
                        Some(seat),
                        None,
                        None,
                        movement.cards.len() as u16,
                        &movement.cards,
                    )
                }
                Finish | FinishCancelled(_) => {
                    self.scheduler
                        .replace_current(Program::Recast(Flow::BatchMovement(movement.clone())));
                    self.scheduler.end(Outcome::Finished(Resolution::BatchMove {
                        event: event.id,
                        cards: movement.committed,
                        cancelled: matches!(stage, FinishCancelled(_)),
                    }));
                    return;
                }
            };
            if queued {
                self.scheduler
                    .replace_current(Program::Recast(Flow::BatchMovement(movement)));
                return;
            }
        }
    }
}
