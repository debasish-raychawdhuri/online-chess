use chess::{Game, Color, GameResult};
use std::time::Instant;

/// Game state for a specific game
pub struct GameState {
    pub game: Game,
    pub white_player: Option<String>,
    pub black_player: Option<String>,
    pub white_time_ms: u64,
    pub black_time_ms: u64,
    pub increment_ms: u64,
    pub last_move_time: Option<Instant>,
    pub active_player: Option<Color>,
    pub game_result: Option<GameResult>,
}
