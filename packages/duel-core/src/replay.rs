//! Explicit, private command logs for deterministic reconstruction by a trusted host.
use serde::{Deserialize, Serialize};

use crate::{Command, Game, GameSetup, RuleError, SeatId};

#[cfg(test)]
mod tests;

const SCHEMA: u16 = 1;

/// A built-in starting state. Custom registrations belong in a host-provided factory.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ReplaySetup {
    Prescribed {
        setup: GameSetup,
    },
    Seeded {
        seed: u32,
        human: SeatId,
        hero: String,
        seats: usize,
    },
    PrescribedCombinedMounts {
        setup: GameSetup,
    },
    SeededCombinedMounts {
        seed: u32,
        human: SeatId,
        hero: String,
        seats: usize,
    },
}

impl ReplaySetup {
    pub fn start(&self) -> Result<Game, RuleError> {
        match self {
            Self::Prescribed { setup } => Game::new(setup.clone()),
            Self::Seeded {
                seed,
                human,
                hero,
                seats,
            } => Game::classic(*seed, *human, hero, *seats),
            Self::PrescribedCombinedMounts { setup } => {
                Game::new_with_mount_slots(setup.clone(), crate::MountSlots::Combined)
            }
            Self::SeededCombinedMounts {
                seed,
                human,
                hero,
                seats,
            } => Game::classic_with_mount_slots(
                *seed,
                *human,
                hero,
                *seats,
                crate::MountSlots::Combined,
            ),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayLog {
    pub schema: u16,
    pub setup: ReplaySetup,
    /// A host-defined identifier for the content and its rules revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub commands: Vec<Command>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ReplayError {
    UnsupportedSchema { received: u16 },
    ContentMismatch,
    OutOfRange { requested: usize, total: usize },
    Setup(RuleError),
    Command { index: usize, error: RuleError },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchema { received } => {
                write!(f, "unsupported replay schema {received}")
            }
            Self::ContentMismatch => {
                f.write_str("the replay requires its original content factory")
            }
            Self::OutOfRange { requested, total } => {
                write!(f, "replay prefix {requested} exceeds {total} commands")
            }
            Self::Setup(error) => write!(f, "replay setup failed: {error}"),
            Self::Command { index, error } => write!(f, "replay command {index} failed: {error}"),
        }
    }
}

impl std::error::Error for ReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Setup(error) | Self::Command { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl ReplayLog {
    pub fn restore(&self) -> Result<Game, ReplayError> {
        self.restore_at(self.commands.len())
    }

    pub fn restore_at(&self, accepted: usize) -> Result<Game, ReplayError> {
        self.restore_using(accepted, None, ReplaySetup::start)
    }

    /// The factory must construct an independent game with the same extensions and
    /// registrations as the original, installed in the same deterministic order.
    pub fn restore_with(
        &self,
        accepted: usize,
        content: &str,
        factory: impl FnOnce(&ReplaySetup) -> Result<Game, RuleError>,
    ) -> Result<Game, ReplayError> {
        self.restore_using(accepted, Some(content), factory)
    }

    fn restore_using(
        &self,
        accepted: usize,
        content: Option<&str>,
        factory: impl FnOnce(&ReplaySetup) -> Result<Game, RuleError>,
    ) -> Result<Game, ReplayError> {
        if self.schema != SCHEMA {
            return Err(ReplayError::UnsupportedSchema {
                received: self.schema,
            });
        }
        if accepted > self.commands.len() {
            return Err(ReplayError::OutOfRange {
                requested: accepted,
                total: self.commands.len(),
            });
        }
        if self.content.as_deref() != content {
            return Err(ReplayError::ContentMismatch);
        }
        let mut game = factory(&self.setup).map_err(ReplayError::Setup)?;
        for (index, command) in self.commands.iter().take(accepted).enumerate() {
            game.submit(command.clone())
                .map_err(|error| ReplayError::Command { index, error })?;
        }
        Ok(game)
    }
}

/// Owns a game and records only commands accepted through this wrapper.
pub struct RecordedGame {
    game: Game,
    log: ReplayLog,
}

impl RecordedGame {
    pub fn new(setup: ReplaySetup) -> Result<Self, RuleError> {
        let game = setup.start()?;
        Ok(Self::from_game(setup, None, game))
    }

    pub fn with_factory(
        setup: ReplaySetup,
        content: &str,
        factory: impl FnOnce(&ReplaySetup) -> Result<Game, RuleError>,
    ) -> Result<Self, RuleError> {
        let game = factory(&setup)?;
        Ok(Self::from_game(setup, Some(content.to_owned()), game))
    }

    fn from_game(setup: ReplaySetup, content: Option<String>, game: Game) -> Self {
        Self {
            game,
            log: ReplayLog {
                schema: SCHEMA,
                setup,
                content,
                commands: vec![],
            },
        }
    }

    pub fn game(&self) -> &Game {
        &self.game
    }
    pub fn log(&self) -> &ReplayLog {
        &self.log
    }

    pub fn submit(&mut self, command: Command) -> Result<(), RuleError> {
        self.game.submit(command.clone())?;
        self.log.commands.push(command);
        Ok(())
    }

    pub fn bot(&mut self) -> Result<bool, RuleError> {
        let Some(command) = self.game.bot_command() else {
            return Ok(false);
        };
        self.submit(command)?;
        Ok(true)
    }
}
