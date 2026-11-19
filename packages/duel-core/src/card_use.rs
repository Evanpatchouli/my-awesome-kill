use crate::classic::equipment;
use crate::{Card, CardKind, EventId, EventKind, Game, Program, RuleError, SeatId, TriggerEvent};

impl Program {
    fn use_targets(&self) -> Option<&[SeatId]> {
        match self {
            Self::Trick(flow) => flow.use_targets(),
            Self::BasicUse(flow) => flow.use_targets(),
            Self::Equipment(equipment::Flow::Attack(flow)) => Some(flow.use_targets()),
            Self::Equipment(flow) => flow
                .equipment_target()
                .map(std::slice::from_ref)
                .or_else(|| matches!(flow, equipment::Flow::Payment(_)).then_some(&[][..])),
            _ => None,
        }
    }

    fn editable_targets(&mut self) -> Option<&mut Vec<SeatId>> {
        match self {
            Self::Trick(flow) => flow.editable_targets(),
            Self::BasicUse(flow) => flow.editable_targets(),
            Self::Equipment(equipment::Flow::Attack(flow)) => flow.editable_targets(),
            _ => None,
        }
    }

    fn card_user(&self) -> Option<(SeatId, CardKind)> {
        match self.kind() {
            EventKind::UseSha { seat, .. } | EventKind::UseShaTargets { seat, .. } => {
                Some((seat, CardKind::Sha))
            }
            EventKind::UseShan { seat, .. } => Some((seat, CardKind::Shan)),
            EventKind::UseTao { seat, .. } => Some((seat, CardKind::Tao)),
            EventKind::Equip { seat, card }
            | EventKind::UseTrick { seat, card, .. }
            | EventKind::UseResponseCard { seat, card } => Some((seat, card.kind)),
            EventKind::UseVirtual { seat, kind } => Some((seat, kind)),
            _ => None,
        }
    }
}

impl Game {
    /// Current primary targets of an active use. A borrowed-sword victim is a dependent choice.
    pub fn use_targets(&self, subject: EventId) -> Option<&[SeatId]> {
        let (frame, program) = self.scheduler.active_frame(subject)?;
        if matches!(program, Program::TargetTiming(_)) {
            self.scheduler.active_frame(frame.parent?)?.1.use_targets()
        } else {
            program.use_targets()
        }
    }

    /// Legal additions to this use without its ordinary distance or initial target-count limit.
    pub fn additional_use_targets(&self, subject: EventId) -> Vec<SeatId> {
        let Some((_, program)) = self.scheduler.active_frame(subject) else {
            return Vec::new();
        };
        let Some((owner, kind)) = program.card_user() else {
            return Vec::new();
        };
        let Some(targets) = program.use_targets() else {
            return Vec::new();
        };
        self.in_order_from(owner)
            .into_iter()
            .filter(|&target| {
                !targets.contains(&target) && self.allows_additional_target(owner, kind, target)
            })
            .collect()
    }

    pub fn removable_use_targets(&self, subject: EventId) -> Vec<SeatId> {
        self.use_targets(subject)
            .filter(|seats| seats.len() > 1)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&id| self.is_in(id))
            .collect()
    }

    fn allows_additional_target(&self, owner: SeatId, kind: CardKind, target: SeatId) -> bool {
        use CardKind::*;
        let Ok(player) = self.seat(target) else {
            return false;
        };
        if !self.is_in(target) || !self.hero_allows_target(target, kind) {
            return false;
        }
        match kind {
            Sha | Juedou | Nanman | Wanjian => target != owner,
            Tao => player.hp < player.max_hp,
            Tiesuo | Wuzhong | Wugu | Taoyuan => true,
            Content(_) => self.content.card(kind).is_some_and(|definition| {
                definition.target == crate::content::TargetRule::Other && target != owner
            }),
            Shunshou | Guohe => {
                target != owner
                    && !(player.hand.is_empty()
                        && player.equipment.is_empty()
                        && player.judges.is_empty())
            }
            Jiedao => {
                target != owner
                    && self.weapon(target).is_some()
                    && !self.jiedao_victims(target).is_empty()
            }
            _ => false,
        }
    }

    pub(crate) fn jiedao_victims(&self, holder: SeatId) -> Vec<SeatId> {
        self.in_order_from(holder)
            .into_iter()
            .filter(|&id| self.accepts_sha_target(holder, id))
            .collect()
    }

    pub(crate) fn accepts_sha_target(&self, seat: SeatId, target: SeatId) -> bool {
        seat != target
            && self.is_in(seat)
            && self.is_in(target)
            && self.in_sha_range(seat, target)
            && self.hero_allows_target(target, CardKind::Sha)
    }

    pub(crate) fn sort_card_targets(&self, targets: &mut [SeatId]) {
        if targets.len() > 1 {
            let order = self
                .seat_order_from(self.active_seat)
                .expect("the turn has a seat");
            targets.sort_by_key(|id| order.iter().position(|seat| seat == id));
        }
    }

    pub(crate) fn edit_use_target(
        &mut self,
        event: &TriggerEvent,
        target: SeatId,
        adding: bool,
    ) -> Result<(), RuleError> {
        if event.timing != "useCard2" {
            return Err(RuleError::WrongEvent);
        }
        let candidates = if adding {
            self.additional_use_targets(event.subject)
        } else {
            self.removable_use_targets(event.subject)
        };
        let targets = self
            .scheduler
            .active_program_mut(event.subject)
            .and_then(Program::editable_targets)
            .ok_or(RuleError::WrongEvent)?;
        if !candidates.contains(&target) {
            return Err(RuleError::InvalidTarget);
        }
        if adding {
            targets.push(target);
        } else {
            targets.retain(|&id| id != target);
        }
        Ok(())
    }

    pub(crate) fn queue_use_card2(&mut self, seat: SeatId, kind: CardKind, cards: &[Card]) -> bool {
        if !self.wants_timing("useCard2") {
            return false;
        }
        let mut event = self.subject_timing("useCard2", Some(seat), None, None, 0, cards);
        event.card_kind = Some(kind);
        if !self.begin_timing_request(&event) {
            return false;
        }
        self.scheduler
            .next(Program::Dispatch(self.dispatch_for_timing(event)));
        true
    }
}
