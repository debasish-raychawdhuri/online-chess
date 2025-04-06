use actix::*;
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use chess::{Color, Game, GameResult};
use log::{info, warn};
use std::time::Instant;

use crate::models::{ClientMessage, ServerMessage, GameState, ChessWebSocketMessage};

/// WebSocket handler for chess games
pub struct ChessWebSocket {
    pub id: String,
    pub app_state: web::Data<crate::models::AppState>,
    pub game_id: String,
    pub color: Option<Color>,
}

impl Actor for ChessWebSocket {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Register the actor with the application state
        let addr = ctx.address();
        self.app_state.sessions.lock().unwrap().insert(self.id.clone(), addr);
        
        // Log the connection and total active sessions
        let total_sessions = self.app_state.sessions.lock().unwrap().len();
        info!("WebSocket connection started: {}", self.id);
        info!("Total active sessions: {}", total_sessions);
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        // Remove the actor from any game it was part of
        if !self.game_id.is_empty() {
            let mut connections = self.app_state.connections.lock().unwrap();
            if let Some(connection_ids) = connections.get_mut(&self.game_id) {
                // Remove this connection from the previous game
                connection_ids.retain(|id| id != &self.id);
                info!("Removed player {} from game {}'s connections", self.id, self.game_id);
                
                // If this was the last player, we could clean up the game state
                if connection_ids.is_empty() {
                    info!("No more players in game {}. Cleaning up.", self.game_id);
                    connections.remove(&self.game_id);
                    
                    // Also remove the game state
                    let mut games = self.app_state.games.lock().unwrap();
                    games.remove(&self.game_id);
                    info!("Removed game state for {}", self.game_id);
                }
            }
            
            // Also remove player from the game state if they were assigned a color
            let mut games = self.app_state.games.lock().unwrap();
            if let Some(game_state) = games.get_mut(&self.game_id) {
                if game_state.white_player.as_ref() == Some(&self.id) {
                    info!("Removing player {} as white from game {}", self.id, self.game_id);
                    game_state.white_player = None;
                }
                if game_state.black_player.as_ref() == Some(&self.id) {
                    info!("Removing player {} as black from game {}", self.id, self.game_id);
                    game_state.black_player = None;
                }
            }
        }
        
        // Remove the actor from the sessions
        self.app_state.sessions.lock().unwrap().remove(&self.id);
        let total_sessions = self.app_state.sessions.lock().unwrap().len();
        info!("WebSocket connection closed: {}", self.id);
        info!("Total active sessions: {}", total_sessions);
        
        Running::Stop
    }
}

impl Handler<ChessWebSocketMessage> for ChessWebSocket {
    type Result = ();

    fn handle(&mut self, msg: ChessWebSocketMessage, ctx: &mut Self::Context) {
        info!("Forwarding message to client: {}", msg.0);
        ctx.text(msg.0);
    }
}

// WebSocket message handler
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ChessWebSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                // Do nothing for pong messages
            }
            Ok(ws::Message::Text(text)) => {
                info!("Received text message: {}", text);
                match serde_json::from_str::<ClientMessage>(text.as_ref()) {
                    Ok(client_msg) => {
                        info!("Parsed client message: {:?}", client_msg);
                        self.handle_message(client_msg, ctx);
                    }
                    Err(e) => {
                        warn!("Error parsing client message: {}", e);
                        ctx.text(format!("{{\"error\": \"Invalid message format: {}\"}}", e));
                    }
                }
            }
            Ok(ws::Message::Binary(_)) => {
                warn!("Binary messages are not supported");
                ctx.text("{\"error\": \"Binary messages are not supported\"}");
            }
            Ok(ws::Message::Close(reason)) => {
                info!("Connection closed: {:?}", reason);
                ctx.close(reason);
                ctx.stop();
            }
            _ => {
                ctx.stop();
            }
        }
    }
}

impl ChessWebSocket {
    pub fn broadcast_to_game(&self, game_id: &str, message: &ServerMessage) {
        info!("Broadcasting message to game {}: {:?}", game_id, message.message_type);
        
        // Get the list of connection IDs for this game and all sessions
        let connection_ids;
        let sessions_copy;
        
        // Scope the locks to minimize lock time
        {
            let connections = self.app_state.connections.lock().unwrap();
            if let Some(ids) = connections.get(game_id) {
                connection_ids = ids.clone();
            } else {
                info!("No connections found for game {}", game_id);
                return;
            }
            
            let sessions = self.app_state.sessions.lock().unwrap();
            sessions_copy = sessions.clone();
        }
        
        info!("Found {} connections for game {}", connection_ids.len(), game_id);
        
        // Serialize the message once
        let msg_str = serde_json::to_string(message).unwrap();
        
        // Send the message to each connection in the game
        for connection_id in &connection_ids {
            // Skip sending to self if it's the same message type as what we just sent
            if connection_id == &self.id && (message.message_type == "joined" || message.message_type == "game_created") {
                info!("Skipping sending to self ({})", self.id);
                continue;
            }
            
            if let Some(addr) = sessions_copy.get(connection_id) {
                info!("Sending message to player {}", connection_id);
                addr.do_send(ChessWebSocketMessage(msg_str.clone()));
            } else {
                info!("Player {} not found in sessions", connection_id);
            }
        }
    }
    
    pub fn handle_message(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        match msg.message_type.as_str() {
            "create" => self.handle_create(msg, ctx),
            "join" => self.handle_join(msg, ctx),
            "move" => self.handle_move(msg, ctx),
            "get_moves" => self.handle_get_moves(msg, ctx),
            "time_sync" => self.handle_time_sync(msg, ctx),
            _ => {
                info!("Unknown message type: {}", msg.message_type);
                ctx.text(format!("{{\"error\": \"Unknown message type: {}\"}}", msg.message_type));
            }
        }
    }
    
    pub fn handle_create(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        // Implementation will be moved from main.rs
    }
    
    pub fn handle_join(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        // Implementation will be moved from main.rs
    }
    
    pub fn handle_move(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        // Implementation will be moved from main.rs
    }
    
    pub fn handle_get_moves(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        // Implementation will be moved from main.rs
    }
    
    pub fn handle_time_sync(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        // Implementation will be moved from main.rs
    }
}
