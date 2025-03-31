use actix::*;
use actix_files as fs;
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
use actix::prelude::*;
use chess::{ChessMove, Color, Game, GameResult, MoveGen, Square};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::str::FromStr;
use uuid::Uuid;

// Models for our application
mod models;

// WebSocket handler for chess games
struct ChessWebSocket {
    id: String,
    app_state: web::Data<AppState>,
    game_id: String,
    color: Option<Color>,
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

// Application state shared between connections
struct AppState {
    games: Mutex<HashMap<String, GameState>>,
    connections: Mutex<HashMap<String, Vec<String>>>,
    sessions: Mutex<HashMap<String, Addr<ChessWebSocket>>>,
}

// Game state for a specific game
struct GameState {
    game: Game,
    white_player: Option<String>,
    black_player: Option<String>,
    white_time_ms: u64,
    black_time_ms: u64,
    increment_ms: u64,
    last_move_time: Option<std::time::Instant>,
    active_player: Option<Color>,
    game_result: Option<GameResult>,
}

// Message sent from client to server
#[derive(Serialize, Deserialize, Debug, Clone)]
struct ClientMessage {
    message_type: String,
    game_id: Option<String>,
    move_from: Option<String>,
    move_to: Option<String>,
    color_preference: Option<String>,
    start_time_minutes: Option<u64>,
    increment_seconds: Option<u64>,
}

// Message sent from server to client
#[derive(Serialize, Deserialize, Debug, Clone)]
struct ServerMessage {
    message_type: String,
    game_id: Option<String>,
    fen: Option<String>,
    color: Option<String>,
    error: Option<String>,
    available_moves: Option<Vec<String>>,
    last_move: Option<LastMove>,
    game_status: Option<String>,
    white_time_ms: Option<u64>,
    black_time_ms: Option<u64>,
    increment_ms: Option<u64>,
    active_color: Option<String>,
}

// Last move information
#[derive(Serialize, Deserialize, Debug, Clone)]
struct LastMove {
    from: String,
    to: String,
}

// Message type for WebSocket communication
#[derive(Message)]
#[rtype(result = "()")]
struct ChessWebSocketMessage(String);

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
    fn broadcast_to_game(&self, game_id: &str, message: &ServerMessage) {
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

    fn handle_create(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        info!("Creating a new game for player {}", self.id);
        
        // Get time settings from the message or use defaults
        let start_time_minutes = msg.start_time_minutes.unwrap_or(15);
        let increment_seconds = msg.increment_seconds.unwrap_or(10);
        
        info!("Game settings: {} minutes, {} seconds increment", start_time_minutes, increment_seconds);
        
        // Create a new game with a unique ID
        let game_id = Uuid::new_v4().to_string();
        self.game_id = game_id.clone();
        
        // Set the player's color to white
        self.color = Some(Color::White);
        
        // Add the player to the connections list for this game
        let mut connections = self.app_state.connections.lock().unwrap();
        connections.entry(game_id.clone()).or_insert_with(Vec::new).push(self.id.clone());
        
        // Create the game state
        let mut games = self.app_state.games.lock().unwrap();
        games.insert(
            game_id.clone(),
            GameState {
                game: Game::new(),
                white_player: Some(self.id.clone()),
                black_player: None,
                white_time_ms: start_time_minutes * 60 * 1000,
                black_time_ms: start_time_minutes * 60 * 1000,
                increment_ms: increment_seconds * 1000,
                last_move_time: None,
                active_player: Some(Color::White),
                game_result: None,
            },
        );
        info!("Created new game {} with player {} as white", game_id, self.id);
        
        // Determine the game status
        let game_status = if games.get(&game_id).unwrap().black_player.is_none() {
            "waiting_for_opponent"
        } else {
            "in_progress"
        };
        
        // Get the FEN string from the game
        let fen = games.get(&game_id).unwrap().game.current_position().to_string();
        
        // Send a message to the client with the game information
        let msg = ServerMessage {
            message_type: "game_created".to_string(),
            game_id: Some(game_id.clone()),
            fen: Some(fen),
            color: Some("white".to_string()),
            error: None,
            available_moves: None,
            last_move: None,
            game_status: Some(game_status.to_string()),
            white_time_ms: Some(start_time_minutes * 60 * 1000),
            black_time_ms: Some(start_time_minutes * 60 * 1000),
            increment_ms: Some(increment_seconds * 1000),
            active_color: None,
        };
        
        info!("Sending game_created message to player {}", self.id);
        ctx.text(serde_json::to_string(&msg).unwrap());
    }

    fn handle_join(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        if let Some(game_id) = msg.game_id {
            info!("Player {} attempting to join game {}", self.id, game_id);
            
            // If the user is already in a game, remove them from that game first
            if !self.game_id.is_empty() {
                info!("Player {} is already in game {}. Removing from that game first", self.id, self.game_id);
                
                // Remove from connections list
                let mut connections = self.app_state.connections.lock().unwrap();
                if let Some(connection_ids) = connections.get_mut(&self.game_id) {
                    // Remove this connection from the previous game
                    connection_ids.retain(|id| id != &self.id);
                    info!("Removed player {} from game {}'s connections", self.id, self.game_id);
                }
                
                // Remove from game state if assigned a color
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
                
                // Drop locks before proceeding
                drop(connections);
                drop(games);
                
                // Clear the game ID and color from this connection
                self.game_id = String::new();
                self.color = None;
                info!("Reset game ID and color for player {}", self.id);
            }
            
            // Check if the game exists
            let mut games = self.app_state.games.lock().unwrap();
            
            // Debug: Log all available games
            info!("Available games: {:?}", games.keys().collect::<Vec<_>>());
            
            if let Some(game_state) = games.get_mut(&game_id) {
                // Determine player color
                let player_color = if game_state.white_player.is_none() {
                    info!("Assigning player {} as white in game {}", self.id, game_id);
                    game_state.white_player = Some(self.id.clone());
                    Color::White
                } else if game_state.black_player.is_none() {
                    info!("Assigning player {} as black in game {}", self.id, game_id);
                    game_state.black_player = Some(self.id.clone());
                    Color::Black
                } else {
                    // Game is full
                    info!("Cannot join game {}: Game is full", game_id);
                    let error_msg = ServerMessage {
                        message_type: "error".to_string(),
                        game_id: Some(game_id),
                        fen: None,
                        color: None,
                        error: Some("Game is full".to_string()),
                        available_moves: None,
                        last_move: None,
                        game_status: None,
                        white_time_ms: None,
                        black_time_ms: None,
                        increment_ms: None,
                        active_color: None,
                    };
                    ctx.text(serde_json::to_string(&error_msg).unwrap());
                    return;
                };
                
                // Update this connection's game ID and color
                self.game_id = game_id.clone();
                self.color = Some(player_color);
                info!("Set player {} color to {:?} in game {}", self.id, player_color, game_id);
                
                // Add player to connections list for this game
                let mut connections = self.app_state.connections.lock().unwrap();
                if let Some(connection_ids) = connections.get_mut(&game_id) {
                    if !connection_ids.contains(&self.id) {
                        connection_ids.push(self.id.clone());
                        info!("Added player {} to game {}'s connections", self.id, game_id);
                    }
                } else {
                    connections.insert(game_id.clone(), vec![self.id.clone()]);
                    info!("Created new connections entry for game {} with player {}", game_id, self.id);
                }
                
                // Get current game state
                let fen = game_state.game.current_position().to_string();
                
                // Update game status to in_progress since both players are now present
                let game_status = "in_progress".to_string();
                
                // Set the last_move_time when the second player joins to start the clock
                if game_state.black_player.is_some() && game_state.white_player.is_some() {
                    game_state.last_move_time = Some(std::time::Instant::now());
                    info!("Setting initial last_move_time as both players have joined");
                }
                
                // Send joined message to the player
                let joined_msg = ServerMessage {
                    message_type: "joined".to_string(),
                    game_id: Some(game_id.clone()),
                    fen: Some(fen.clone()),
                    color: Some(color_to_string(player_color)),
                    error: None,
                    available_moves: None,
                    last_move: None,
                    game_status: Some(game_status.clone()),
                    white_time_ms: Some(game_state.white_time_ms),
                    black_time_ms: Some(game_state.black_time_ms),
                    increment_ms: Some(game_state.increment_ms),
                    active_color: None,
                };
                
                info!("Sending joined message to player {}", self.id);
                ctx.text(serde_json::to_string(&joined_msg).unwrap());
                
                // Notify other players that someone joined
                let player_joined_msg = ServerMessage {
                    message_type: "player_joined".to_string(),
                    game_id: Some(game_id.clone()),
                    fen: Some(fen),
                    color: Some(color_to_string(player_color)),
                    error: None,
                    available_moves: None,
                    last_move: None,
                    game_status: Some(game_status),
                    white_time_ms: Some(game_state.white_time_ms),
                    black_time_ms: Some(game_state.black_time_ms),
                    increment_ms: Some(game_state.increment_ms),
                    active_color: None,
                };
                
                // Drop the locks before broadcasting
                drop(games);
                drop(connections);
                
                info!("Broadcasting player_joined message for game {}", game_id);
                self.broadcast_to_game(&game_id, &player_joined_msg);
            } else {
                // Game not found
                info!("Game {} not found", game_id);
                let error_msg = ServerMessage {
                    message_type: "error".to_string(),
                    game_id: Some(game_id),
                    fen: None,
                    color: None,
                    error: Some("Game not found".to_string()),
                    available_moves: None,
                    last_move: None,
                    game_status: None,
                    white_time_ms: None,
                    black_time_ms: None,
                    increment_ms: None,
                    active_color: None,
                };
                ctx.text(serde_json::to_string(&error_msg).unwrap());
            }
        } else {
            // No game ID provided
            info!("Join request missing game ID");
            let error_msg = ServerMessage {
                message_type: "error".to_string(),
                game_id: None,
                fen: None,
                color: None,
                error: Some("Game ID is required to join a game".to_string()),
                available_moves: None,
                last_move: None,
                game_status: None,
                white_time_ms: None,
                black_time_ms: None,
                increment_ms: None,
                active_color: None,
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
        }
    }

    fn handle_move(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        info!("Processing move from player {}", self.id);
        
        if self.game_id.is_empty() {
            let error_msg = ServerMessage {
                message_type: "error".to_string(),
                game_id: None,
                fen: None,
                color: None,
                error: Some("You are not in a game".to_string()),
                available_moves: None,
                last_move: None,
                game_status: None,
                white_time_ms: None,
                black_time_ms: None,
                increment_ms: None,
                active_color: None,
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
            return;
        }
        
        let from = msg.move_from.as_ref().unwrap_or(&"".to_string()).to_string();
        let to = msg.move_to.as_ref().unwrap_or(&"".to_string()).to_string();
        
        if from.is_empty() || to.is_empty() {
            let error_msg = ServerMessage {
                message_type: "error".to_string(),
                game_id: Some(self.game_id.clone()),
                fen: None,
                color: None,
                error: Some("Invalid move format".to_string()),
                available_moves: None,
                last_move: None,
                game_status: None,
                white_time_ms: None,
                black_time_ms: None,
                increment_ms: None,
                active_color: None,
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
            return;
        }
        
        let mut games = self.app_state.games.lock().unwrap();
        
        if let Some(game_state) = games.get_mut(&self.game_id) {
            let game = &mut game_state.game;
            
            // Check if the game has already ended due to timeout or other reasons
            if game_state.game_result.is_some() {
                let error_msg = ServerMessage {
                    message_type: "error".to_string(),
                    game_id: Some(self.game_id.clone()),
                    fen: None,
                    color: None,
                    error: Some("Game has already ended".to_string()),
                    available_moves: None,
                    last_move: None,
                    game_status: None,
                    white_time_ms: None,
                    black_time_ms: None,
                    increment_ms: None,
                    active_color: None,
                };
                ctx.text(serde_json::to_string(&error_msg).unwrap());
                return;
            }
            
            // Check if it's the player's turn
            let current_turn = game.side_to_move();
            let player_color = if game_state.white_player.as_ref() == Some(&self.id) {
                Some(Color::White)
            } else if game_state.black_player.as_ref() == Some(&self.id) {
                Some(Color::Black)
            } else {
                None
            };
            
            if player_color != Some(current_turn) {
                let error_msg = ServerMessage {
                    message_type: "error".to_string(),
                    game_id: Some(self.game_id.clone()),
                    fen: None,
                    color: None,
                    error: Some("It's not your turn".to_string()),
                    available_moves: None,
                    last_move: None,
                    game_status: None,
                    white_time_ms: None,
                    black_time_ms: None,
                    increment_ms: None,
                    active_color: None,
                };
                ctx.text(serde_json::to_string(&error_msg).unwrap());
                return;
            }
            
            // Parse the move
            let from_square = Square::from_str(&from).unwrap();
            let to_square = Square::from_str(&to).unwrap();
            
            // Check if the piece belongs to the player
            if let Some(_piece) = game.current_position().piece_on(from_square) {
                // Try to make the move
                let chess_move = ChessMove::new(from_square, to_square, None);
                
                if game.make_move(chess_move) {
                    // Update timers
                    let now = std::time::Instant::now();
                    
                    // If this is not the first move, update the time for the player who just moved
                    if let Some(last_move_time) = game_state.last_move_time {
                        let elapsed = now.duration_since(last_move_time).as_millis() as u64;
                        
                        // Update the time for the player who just moved
                        match player_color {
                            Some(Color::White) => {
                                if game_state.white_time_ms > elapsed {
                                    game_state.white_time_ms -= elapsed;
                                    // Add increment after the move
                                    game_state.white_time_ms += game_state.increment_ms;
                                } else {
                                    game_state.white_time_ms = 0;
                                    // Player lost on time - check for insufficient material
                                    if has_insufficient_material(&game.current_position()) {
                                        info!("White lost on time but opponent has insufficient material - draw");
                                        // Set game result to draw
                                        game_state.game_result = Some(GameResult::DrawDeclared);
                                    } else {
                                        info!("White lost on time");
                                        // Set game result to black wins
                                        game_state.game_result = Some(GameResult::WhiteResigns);
                                    }
                                }
                            },
                            Some(Color::Black) => {
                                if game_state.black_time_ms > elapsed {
                                    game_state.black_time_ms -= elapsed;
                                    // Add increment after the move
                                    game_state.black_time_ms += game_state.increment_ms;
                                } else {
                                    game_state.black_time_ms = 0;
                                    // Player lost on time - check for insufficient material
                                    if has_insufficient_material(&game.current_position()) {
                                        info!("Black lost on time but opponent has insufficient material - draw");
                                        // Set game result to draw
                                        game_state.game_result = Some(GameResult::DrawDeclared);
                                    } else {
                                        info!("Black lost on time");
                                        // Set game result to white wins
                                        game_state.game_result = Some(GameResult::BlackResigns);
                                    }
                                }
                            },
                            None => {}
                        }
                    }
                    
                    // Update the last move time and active player
                    game_state.last_move_time = Some(now);
                    game_state.active_player = Some(game.side_to_move());
                    
                    // Log the active player for debugging
                    info!("Active player after move: {:?}", game_state.active_player);
                    
                    // Create the last move info
                    let last_move = LastMove {
                        from,
                        to,
                    };
                    
                    // Get the updated game status
                    let game_status = get_game_status(game, game_state.game_result);
                    
                    // Create the message to broadcast
                    let msg = ServerMessage {
                        message_type: "move_made".to_string(),
                        game_id: Some(self.game_id.clone()),
                        fen: Some(game.current_position().to_string()),
                        color: None,
                        error: None,
                        available_moves: None,
                        last_move: Some(last_move),
                        game_status: Some(game_status),
                        white_time_ms: Some(game_state.white_time_ms),
                        black_time_ms: Some(game_state.black_time_ms),
                        increment_ms: Some(game_state.increment_ms),
                        active_color: None,
                    };
                    
                    self.broadcast_to_game(&self.game_id, &msg);
                } else {
                    // Move was invalid
                    let error_msg = ServerMessage {
                        message_type: "error".to_string(),
                        game_id: Some(self.game_id.clone()),
                        fen: None,
                        color: None,
                        error: Some("Invalid move".to_string()),
                        available_moves: None,
                        last_move: None,
                        game_status: None,
                        white_time_ms: None,
                        black_time_ms: None,
                        increment_ms: None,
                        active_color: None,
                    };
                    ctx.text(serde_json::to_string(&error_msg).unwrap());
                }
            } else {
                let error_msg = ServerMessage {
                    message_type: "error".to_string(),
                    game_id: Some(self.game_id.clone()),
                    fen: None,
                    color: None,
                    error: Some("No piece at the selected square".to_string()),
                    available_moves: None,
                    last_move: None,
                    game_status: None,
                    white_time_ms: None,
                    black_time_ms: None,
                    increment_ms: None,
                    active_color: None,
                };
                ctx.text(serde_json::to_string(&error_msg).unwrap());
            }
        } else {
            let error_msg = ServerMessage {
                message_type: "error".to_string(),
                game_id: Some(self.game_id.clone()),
                fen: None,
                color: None,
                error: Some("Game not found".to_string()),
                available_moves: None,
                last_move: None,
                game_status: None,
                white_time_ms: None,
                black_time_ms: None,
                increment_ms: None,
                active_color: None,
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
        }
    }

    fn handle_get_moves(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        if self.game_id.is_empty() {
            let error_msg = ServerMessage {
                message_type: "error".to_string(),
                game_id: None,
                fen: None,
                color: None,
                error: Some("Not in a game".to_string()),
                available_moves: None,
                last_move: None,
                game_status: None,
                white_time_ms: None,
                black_time_ms: None,
                increment_ms: None,
                active_color: None,
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
            return;
        }

        let from = match msg.move_from {
            Some(from) => from,
            None => {
                let error_msg = ServerMessage {
                    message_type: "error".to_string(),
                    game_id: Some(self.game_id.clone()),
                    fen: None,
                    color: None,
                    error: Some("No from square provided".to_string()),
                    available_moves: None,
                    last_move: None,
                    game_status: None,
                    white_time_ms: None,
                    black_time_ms: None,
                    increment_ms: None,
                    active_color: None,
                };
                ctx.text(serde_json::to_string(&error_msg).unwrap());
                return;
            }
        };
        
        let mut games = self.app_state.games.lock().unwrap();
        
        if let Some(game_state) = games.get_mut(&self.game_id) {
            // Parse the from square
            let from_square = Square::from_str(&from.to_lowercase()).unwrap();
            let board = game_state.game.current_position();
            
            // Check if there's a piece at the square
            if let Some(piece) = board.piece_on(from_square) {
                // Check if it's the player's turn
                let current_turn = game_state.game.side_to_move();
                let player_color = if game_state.white_player.as_ref() == Some(&self.id) {
                    Some(Color::White)
                } else if game_state.black_player.as_ref() == Some(&self.id) {
                    Some(Color::Black)
                } else {
                    None
                };
                
                info!("Turn check: current_turn={:?}, player_color={:?}, player_id={}, white_player={:?}, black_player={:?}", 
                      current_turn, player_color, self.id, game_state.white_player, game_state.black_player);
                
                if player_color != Some(current_turn) {
                    let error_msg = ServerMessage {
                        message_type: "error".to_string(),
                        game_id: Some(self.game_id.clone()),
                        fen: None,
                        color: None,
                        error: Some("Not your turn".to_string()),
                        available_moves: None,
                        last_move: None,
                        game_status: None,
                        white_time_ms: None,
                        black_time_ms: None,
                        increment_ms: None,
                        active_color: None,
                    };
                    ctx.text(serde_json::to_string(&error_msg).unwrap());
                    return;
                }
                
                // Check if the piece belongs to the player
                let piece_color = board.color_on(from_square).unwrap();
                
                info!("Piece color check: piece={:?}, piece_color={:?}, player_color={:?}, self.color={:?}", 
                      piece, piece_color, player_color, self.color);
                
                if player_color != Some(piece_color) {
                    let error_msg = ServerMessage {
                        message_type: "error".to_string(),
                        game_id: Some(self.game_id.clone()),
                        fen: None,
                        color: None,
                        error: Some("Not your piece".to_string()),
                        available_moves: None,
                        last_move: None,
                        game_status: None,
                        white_time_ms: None,
                        black_time_ms: None,
                        increment_ms: None,
                        active_color: None,
                    };
                    ctx.text(serde_json::to_string(&error_msg).unwrap());
                    return;
                }

                // Get valid moves for the piece
                let mut valid_moves = Vec::new();
                let move_gen = MoveGen::new_legal(&board);
                for chess_move in move_gen {
                    if chess_move.get_source() == from_square {
                        valid_moves.push(chess_move.get_dest().to_string());
                    }
                }

                let msg = ServerMessage {
                    message_type: "available_moves".to_string(),
                    game_id: Some(self.game_id.clone()),
                    fen: None,
                    color: None,
                    error: None,
                    available_moves: Some(valid_moves),
                    last_move: None,
                    game_status: None,
                    white_time_ms: None,
                    black_time_ms: None,
                    increment_ms: None,
                    active_color: None,
                };
                ctx.text(serde_json::to_string(&msg).unwrap());
            } else {
                let error_msg = ServerMessage {
                    message_type: "error".to_string(),
                    game_id: Some(self.game_id.clone()),
                    fen: None,
                    color: None,
                    error: Some("No piece at that square".to_string()),
                    available_moves: None,
                    last_move: None,
                    game_status: None,
                    white_time_ms: None,
                    black_time_ms: None,
                    increment_ms: None,
                    active_color: None,
                };
                ctx.text(serde_json::to_string(&error_msg).unwrap());
            }
        } else {
            let error_msg = ServerMessage {
                message_type: "error".to_string(),
                game_id: Some(self.game_id.clone()),
                fen: None,
                color: None,
                error: Some("Game not found".to_string()),
                available_moves: None,
                last_move: None,
                game_status: None,
                white_time_ms: None,
                black_time_ms: None,
                increment_ms: None,
                active_color: None,
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
        }
    }

    fn handle_time_sync(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        info!("Time sync request received from player {}", self.id);
        
        // Get the game ID from the message
        let game_id = match msg.game_id {
            Some(id) => id,
            None => {
                info!("Time sync request missing game ID");
                let error_msg = ServerMessage {
                    message_type: "error".to_string(),
                    game_id: None,
                    fen: None,
                    color: None,
                    error: Some("Game ID is required".to_string()),
                    available_moves: None,
                    last_move: None,
                    game_status: None,
                    white_time_ms: None,
                    black_time_ms: None,
                    increment_ms: None,
                    active_color: None,
                };
                ctx.text(serde_json::to_string(&error_msg).unwrap());
                return;
            }
        };
        
        // Get the game state
        let mut games = self.app_state.games.lock().unwrap();
        if let Some(game_state) = games.get_mut(&game_id) {
            // Update the time for the active player if a move has been made
            if let Some(last_move_time) = game_state.last_move_time {
                let now = std::time::Instant::now();
                let elapsed = now.duration_since(last_move_time).as_millis() as u64;
                
                // Only update the time if the game is in progress
                if game_state.white_player.is_some() && game_state.black_player.is_some() {
                    // Update the time for the active player
                    match game_state.active_player {
                        Some(Color::White) => {
                            if game_state.white_time_ms > elapsed {
                                game_state.white_time_ms -= elapsed;
                            } else {
                                game_state.white_time_ms = 0;
                                // Player lost on time - check for insufficient material
                                if has_insufficient_material(&game_state.game.current_position()) {
                                    info!("White lost on time but opponent has insufficient material - draw");
                                    // Set game result to draw
                                    game_state.game_result = Some(GameResult::DrawDeclared);
                                } else {
                                    info!("White lost on time");
                                    // Set game result to black wins
                                    game_state.game_result = Some(GameResult::WhiteResigns);
                                }
                            }
                        },
                        Some(Color::Black) => {
                            if game_state.black_time_ms > elapsed {
                                game_state.black_time_ms -= elapsed;
                            } else {
                                game_state.black_time_ms = 0;
                                // Player lost on time - check for insufficient material
                                if has_insufficient_material(&game_state.game.current_position()) {
                                    info!("Black lost on time but opponent has insufficient material - draw");
                                    // Set game result to draw
                                    game_state.game_result = Some(GameResult::DrawDeclared);
                                } else {
                                    info!("Black lost on time");
                                    // Set game result to white wins
                                    game_state.game_result = Some(GameResult::BlackResigns);
                                }
                            }
                        },
                        None => {}
                    }
                    
                    // Update the last move time
                    game_state.last_move_time = Some(now);
                }
            }
            
            // Get the active color from the current position
            let active_color = match game_state.game.side_to_move() {
                Color::White => "white",
                Color::Black => "black",
            };
            
            // Get the game status
            let game_status = get_game_status(&game_state.game, game_state.game_result);
            
            // Send the time sync response
            let time_sync_msg = ServerMessage {
                message_type: "time_sync".to_string(),
                game_id: Some(game_id.clone()),
                fen: Some(game_state.game.current_position().to_string()),
                color: None,
                error: None,
                available_moves: None,
                last_move: None,
                game_status: Some(game_status),
                white_time_ms: Some(game_state.white_time_ms),
                black_time_ms: Some(game_state.black_time_ms),
                increment_ms: Some(game_state.increment_ms),
                active_color: Some(active_color.to_string()),
            };
            
            // Drop the lock before broadcasting
            drop(games);
            
            // Broadcast the time sync response to all players in the game
            self.broadcast_to_game(&game_id, &time_sync_msg);
        } else {
            // Game not found
            info!("Game {} not found for time sync", game_id);
            let error_msg = ServerMessage {
                message_type: "error".to_string(),
                game_id: Some(game_id),
                fen: None,
                color: None,
                error: Some("Game not found".to_string()),
                available_moves: None,
                last_move: None,
                game_status: None,
                white_time_ms: None,
                black_time_ms: None,
                increment_ms: None,
                active_color: None,
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
        }
    }

    fn handle_message(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
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
}

// WebSocket connection handler
async fn ws_index(req: HttpRequest, stream: web::Payload, app_state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    info!("New WebSocket connection request");
    
    // Create a unique ID for this connection
    let id = Uuid::new_v4().to_string();
    info!("Generated WebSocket ID: {}", id);
    
    // Initialize the WebSocket actor
    let ws = ChessWebSocket {
        id: id.clone(),
        app_state: app_state.clone(),
        game_id: String::new(),
        color: None,
    };
    
    // Start the WebSocket actor
    ws::start(ws, &req, stream)
}

// HTTP handlers
async fn index() -> impl Responder {
    fs::NamedFile::open_async("./static/index.html").await.unwrap()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logger
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    
    info!("Starting chess web app server at http://127.0.0.1:8080");
    
    // Create shared application state
    let app_state = web::Data::new(AppState {
        games: Mutex::new(HashMap::new()),
        connections: Mutex::new(HashMap::new()),
        sessions: Mutex::new(HashMap::new()),
    });
    
    // Start HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(web::resource("/").to(index))
            .service(web::resource("/ws").route(web::get().to(ws_index)))
            .service(fs::Files::new("/static", "./static"))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

fn color_to_string(color: Color) -> String {
    match color {
        Color::White => "white".to_string(),
        Color::Black => "black".to_string(),
    }
}

fn get_game_status(game: &Game, game_result: Option<GameResult>) -> String {
    match game_result {
        Some(GameResult::WhiteCheckmates) => "white_wins".to_string(),
        Some(GameResult::BlackCheckmates) => "black_wins".to_string(),
        Some(GameResult::WhiteResigns) => "black_wins".to_string(),
        Some(GameResult::BlackResigns) => "white_wins".to_string(),
        Some(GameResult::Stalemate) => "draw".to_string(),
        Some(GameResult::DrawAccepted) => "draw".to_string(),
        Some(GameResult::DrawDeclared) => "draw".to_string(),
        None => {
            if game.current_position().checkers().0 > 0 {
                "check".to_string()
            } else if game.side_to_move() == Color::White {
                "white_turn".to_string()
            } else {
                "black_turn".to_string()
            }
        }
    }
}

fn has_insufficient_material(board: &chess::Board) -> bool {
    let mut white_pawns = 0;
    let mut white_knights = 0;
    let mut white_bishops = 0;
    let mut white_rooks = 0;
    let mut white_queens = 0;
    let mut black_pawns = 0;
    let mut black_knights = 0;
    let mut black_bishops = 0;
    let mut black_rooks = 0;
    let mut black_queens = 0;

    // Iterate through all possible squares on the board
    for rank in 0..8 {
        for file in 0..8 {
            let square = chess::Square::make_square(chess::Rank::from_index(rank), chess::File::from_index(file));
            if let Some(piece) = board.piece_on(square) {
                match piece {
                    chess::Piece::Pawn => {
                        if board.color_on(square) == Some(chess::Color::White) {
                            white_pawns += 1;
                        } else {
                            black_pawns += 1;
                        }
                    }
                    chess::Piece::Knight => {
                        if board.color_on(square) == Some(chess::Color::White) {
                            white_knights += 1;
                        } else {
                            black_knights += 1;
                        }
                    }
                    chess::Piece::Bishop => {
                        if board.color_on(square) == Some(chess::Color::White) {
                            white_bishops += 1;
                        } else {
                            black_bishops += 1;
                        }
                    }
                    chess::Piece::Rook => {
                        if board.color_on(square) == Some(chess::Color::White) {
                            white_rooks += 1;
                        } else {
                            black_rooks += 1;
                        }
                    }
                    chess::Piece::Queen => {
                        if board.color_on(square) == Some(chess::Color::White) {
                            white_queens += 1;
                        } else {
                            black_queens += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Check for insufficient material
    if white_pawns == 0 && white_knights == 0 && white_bishops == 0 && white_rooks == 0 && white_queens == 0 {
        // White has no pieces other than the king
        if black_pawns == 0 && black_knights == 0 && black_bishops == 0 && black_rooks == 0 && black_queens == 0 {
            // Black has no pieces other than the king
            return true;
        } else if black_pawns == 0 && black_knights == 0 && black_bishops == 0 && black_rooks == 0 && black_queens == 1 {
            // Black has only one queen
            return false;
        } else if black_pawns == 0 && black_knights == 0 && black_bishops == 0 && black_rooks == 1 && black_queens == 0 {
            // Black has only one rook
            return false;
        } else if black_pawns == 0 && black_knights == 0 && black_bishops == 1 && black_rooks == 0 && black_queens == 0 {
            // Black has only one bishop
            return true;
        } else if black_pawns == 0 && black_knights == 1 && black_bishops == 0 && black_rooks == 0 && black_queens == 0 {
            // Black has only one knight
            return true;
        }
    } else if black_pawns == 0 && black_knights == 0 && black_bishops == 0 && black_rooks == 0 && black_queens == 0 {
        // Black has no pieces other than the king
        if white_pawns == 0 && white_knights == 0 && white_bishops == 0 && white_rooks == 0 && white_queens == 0 {
            // White has no pieces other than the king
            return true;
        } else if white_pawns == 0 && white_knights == 0 && white_bishops == 0 && white_rooks == 0 && white_queens == 1 {
            // White has only one queen
            return false;
        } else if white_pawns == 0 && white_knights == 0 && white_bishops == 0 && white_rooks == 1 && white_queens == 0 {
            // White has only one rook
            return false;
        } else if white_pawns == 0 && white_knights == 0 && white_bishops == 1 && white_rooks == 0 && white_queens == 0 {
            // White has only one bishop
            return true;
        } else if white_pawns == 0 && white_knights == 1 && white_bishops == 0 && white_rooks == 0 && white_queens == 0 {
            // White has only one knight
            return true;
        }
    }

    false
}
