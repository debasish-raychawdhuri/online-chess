use actix::Addr;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::models::{GameState, ChessWebSocket};

/// Application state shared between connections
pub struct AppState {
    pub games: Mutex<HashMap<String, GameState>>,
    pub connections: Mutex<HashMap<String, Vec<String>>>,
    pub sessions: Mutex<HashMap<String, Addr<ChessWebSocket>>>,
}
