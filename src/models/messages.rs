use actix::Message;
use serde::{Deserialize, Serialize};

/// Message sent from client to server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientMessage {
    pub message_type: String,
    pub game_id: Option<String>,
    pub move_from: Option<String>,
    pub move_to: Option<String>,
    pub color_preference: Option<String>,
    pub start_time_minutes: Option<u64>,
    pub increment_seconds: Option<u64>,
    pub promote_to: Option<String>,
}

/// Message sent from server to client
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerMessage {
    pub message_type: String,
    pub game_id: Option<String>,
    pub fen: Option<String>,
    pub color: Option<String>,
    pub error: Option<String>,
    pub available_moves: Option<Vec<String>>,
    pub last_move: Option<LastMove>,
    pub game_status: Option<String>,
    pub white_time_ms: Option<u64>,
    pub black_time_ms: Option<u64>,
    pub increment_ms: Option<u64>,
    pub active_color: Option<String>,
}

/// Last move information
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LastMove {
    pub from: String,
    pub to: String,
}

/// Message type for WebSocket communication
#[derive(Message)]
#[rtype(result = "()")]
pub struct ChessWebSocketMessage(pub String);
