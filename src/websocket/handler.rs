use actix::*;
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use chess::{ChessMove, Color, Game, GameResult, MoveGen, Piece, Square};
use log::{info, warn};
use std::str::FromStr;
use uuid::Uuid;

use crate::models::*;
use crate::game::utils::{color_to_string, get_game_status, has_insufficient_material};
use crate::state::AppState;

/// WebSocket handler for chess games
pub struct ChessWebSocket {
    pub id: String,
    pub app_state: web::Data<AppState>,
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
        
        {
            let connections = self.app_state.connections.lock().unwrap();
            connection_ids = match connections.get(game_id) {
                Some(ids) => ids.clone(),
                None => {
                    warn!("No connections found for game {}", game_id);
                    return;
                }
            };
            
            let sessions = self.app_state.sessions.lock().unwrap();
            sessions_copy = sessions.clone();
        }
        
        // Serialize the message once
        let message_str = match serde_json::to_string(message) {
            Ok(s) => s,
            Err(e) => {
                warn!("Error serializing message: {}", e);
                return;
            }
        };
        
        // Send the message to all connections in this game
        for conn_id in connection_ids {
            if let Some(addr) = sessions_copy.get(&conn_id) {
                info!("Sending message to connection {}", conn_id);
                addr.do_send(ChessWebSocketMessage(message_str.clone()));
            } else {
                warn!("Session not found for connection ID: {}", conn_id);
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
                warn!("Unknown message type: {}", msg.message_type);
                ctx.text("{\"error\": \"Unknown message type\"}");
            }
        }
    }

    pub fn handle_create(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        // Create a new game
        let game_id = Uuid::new_v4().to_string();
        self.game_id = game_id.clone();
        
        // Initialize the game state
        let mut games = self.app_state.games.lock().unwrap();
        games.insert(game_id.clone(), Game::new());
        
        // Add the player to the game connections
        let mut connections = self.app_state.connections.lock().unwrap();
        connections.insert(game_id.clone(), vec![self.id.clone()]);
        
        // Send a message to the client with the game ID
        let message = ServerMessage {
            message_type: "game_id".to_string(),
            data: game_id,
        };
        ctx.text(serde_json::to_string(&message).unwrap());
    }

    pub fn handle_join(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        // Get the game ID from the message
        let game_id = msg.data.clone();
        
        // Check if the game exists
        let mut games = self.app_state.games.lock().unwrap();
        if !games.contains_key(&game_id) {
            ctx.text("{\"error\": \"Game not found\"}");
            return;
        }
        
        // Add the player to the game connections
        let mut connections = self.app_state.connections.lock().unwrap();
        if let Some(connection_ids) = connections.get_mut(&game_id) {
            connection_ids.push(self.id.clone());
        } else {
            connections.insert(game_id.clone(), vec![self.id.clone()]);
        }
        
        // Send a message to the client with the game state
        let game_state = games.get(&game_id).unwrap();
        let message = ServerMessage {
            message_type: "game_state".to_string(),
            data: serde_json::to_string(game_state).unwrap(),
        };
        ctx.text(serde_json::to_string(&message).unwrap());
    }

    pub fn handle_move(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        // Get the game ID and move from the message
        let game_id = self.game_id.clone();
        let move_str = msg.data.clone();
        
        // Check if the game exists
        let mut games = self.app_state.games.lock().unwrap();
        if !games.contains_key(&game_id) {
            ctx.text("{\"error\": \"Game not found\"}");
            return;
        }
        
        // Make the move
        let game_state = games.get_mut(&game_id).unwrap();
        let move_result = game_state.make_move(move_str);
        
        // Send a message to the client with the result
        let message = ServerMessage {
            message_type: "move_result".to_string(),
            data: serde_json::to_string(&move_result).unwrap(),
        };
        ctx.text(serde_json::to_string(&message).unwrap());
    }

    pub fn handle_get_moves(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        // Get the game ID from the message
        let game_id = self.game_id.clone();
        
        // Check if the game exists
        let mut games = self.app_state.games.lock().unwrap();
        if !games.contains_key(&game_id) {
            ctx.text("{\"error\": \"Game not found\"}");
            return;
        }
        
        // Get the available moves
        let game_state = games.get(&game_id).unwrap();
        let moves = game_state.get_available_moves();
        
        // Send a message to the client with the moves
        let message = ServerMessage {
            message_type: "available_moves".to_string(),
            data: serde_json::to_string(&moves).unwrap(),
        };
        ctx.text(serde_json::to_string(&message).unwrap());
    }

    pub fn handle_time_sync(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        // Get the game ID from the message
        let game_id = self.game_id.clone();
        
        // Check if the game exists
        let mut games = self.app_state.games.lock().unwrap();
        if !games.contains_key(&game_id) {
            ctx.text("{\"error\": \"Game not found\"}");
            return;
        }
        
        // Get the current time
        let game_state = games.get(&game_id).unwrap();
        let time = game_state.get_time();
        
        // Send a message to the client with the time
        let message = ServerMessage {
            message_type: "time".to_string(),
            data: serde_json::to_string(&time).unwrap(),
        };
        ctx.text(serde_json::to_string(&message).unwrap());
    }
}

/// WebSocket connection handler
pub async fn ws_index(req: HttpRequest, stream: web::Payload, app_state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    info!("New WebSocket connection request");
    
    // Generate a unique ID for this connection
    let id = Uuid::new_v4().to_string();
    info!("Generated connection ID: {}", id);
    
    // Create a new WebSocket actor
    let ws = ChessWebSocket {
        id,
        app_state: app_state.clone(),
        game_id: String::new(),
        color: None,
    };
    
    // Start the WebSocket actor
    let resp = ws::start(ws, &req, stream)?;
    info!("WebSocket connection started");
    
    Ok(resp)
}
