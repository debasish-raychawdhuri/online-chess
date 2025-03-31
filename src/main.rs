use actix::*;
use actix_files as fs;
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
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
}

// Message sent from client to server
#[derive(Serialize, Deserialize, Debug, Clone)]
struct ClientMessage {
    action: String,
    game_id: Option<String>,
    move_from: Option<String>,
    move_to: Option<String>,
    color_preference: Option<String>,
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
}

// Last move information
#[derive(Serialize, Deserialize, Debug, Clone)]
struct LastMove {
    from: String,
    to: String,
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
                info!("Received ping from {}", self.id);
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                info!("Received pong from {}", self.id);
            }
            Ok(ws::Message::Text(text)) => {
                info!("Received text message from {}: {}", self.id, text);
                self.handle_message(text.to_string(), ctx);
            }
            Ok(ws::Message::Binary(bin)) => {
                info!("Received binary message from {}: {} bytes", self.id, bin.len());
            }
            Ok(ws::Message::Close(reason)) => {
                info!("Received close message from {}: {:?}", self.id, reason);
                ctx.close(reason);
            }
            _ => {
                info!("Received unknown message type from {}", self.id);
            }
        }
    }
}

impl ChessWebSocket {
    fn handle_message(&mut self, text: impl AsRef<str>, ctx: &mut ws::WebsocketContext<Self>) {
        info!("Received message: {}", text.as_ref());
        match serde_json::from_str::<ClientMessage>(text.as_ref()) {
            Ok(client_msg) => {
                info!("Parsed client message: {:?}", client_msg);
                match client_msg.action.as_str() {
                    "create" => {
                        info!("Processing create action");
                        self.handle_create(ctx);
                    }
                    "join" => {
                        info!("Processing join action with game_id: {:?}", client_msg.game_id);
                        self.handle_join(client_msg, ctx);
                    }
                    "move" => {
                        info!("Move action received: from: {:?}, to: {:?}", client_msg.move_from, client_msg.move_to);
                        if let (Some(from), Some(to)) = (client_msg.move_from.clone(), client_msg.move_to.clone()) {
                            self.handle_move(from, to, ctx);
                        } else {
                            info!("Move action missing from or to");
                            let error_msg = ServerMessage {
                                message_type: "error".to_string(),
                                game_id: if self.game_id.is_empty() { None } else { Some(self.game_id.clone()) },
                                fen: None,
                                color: None,
                                error: Some("Move requires from and to positions".to_string()),
                                available_moves: None,
                                last_move: None,
                                game_status: None,
                            };
                            ctx.text(serde_json::to_string(&error_msg).unwrap());
                        }
                    }
                    "get_moves" => {
                        if let Some(from) = client_msg.move_from {
                            self.handle_get_moves(from, ctx);
                        } else {
                            let error_msg = ServerMessage {
                                message_type: "error".to_string(),
                                game_id: Some(self.game_id.clone()),
                                fen: None,
                                color: None,
                                error: Some("Get moves requires from position".to_string()),
                                available_moves: None,
                                last_move: None,
                                game_status: None,
                            };
                            ctx.text(serde_json::to_string(&error_msg).unwrap());
                        }
                    }
                    _ => {
                        info!("Unknown action: {}", client_msg.action);
                        let error_msg = ServerMessage {
                            message_type: "error".to_string(),
                            game_id: if self.game_id.is_empty() { None } else { Some(self.game_id.clone()) },
                            fen: None,
                            color: None,
                            error: Some(format!("Unknown action: {}", client_msg.action)),
                            available_moves: None,
                            last_move: None,
                            game_status: None,
                        };
                        ctx.text(serde_json::to_string(&error_msg).unwrap());
                    }
                }
            }
            Err(e) => {
                info!("Error parsing client message: {}", e);
                let error_msg = ServerMessage {
                    message_type: "error".to_string(),
                    game_id: if self.game_id.is_empty() { None } else { Some(self.game_id.clone()) },
                    fen: None,
                    color: None,
                    error: Some(format!("Invalid message format: {}", e)),
                    available_moves: None,
                    last_move: None,
                    game_status: None,
                };
                ctx.text(serde_json::to_string(&error_msg).unwrap());
            }
        }
    }

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

    fn handle_create(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        info!("Creating a new game for player {}", self.id);
        
        // Create a new game with a unique ID
        let game_id = Uuid::new_v4().to_string();
        info!("Generated new game ID: {}", game_id);
        
        // If the user is already in a game, remove them from that game first
        if !self.game_id.is_empty() {
            info!("Player {} is already in game {}. Removing from that game first", self.id, self.game_id);
            
            // Remove from connections list
            let mut connections = self.app_state.connections.lock().unwrap();
            if let Some(connection_ids) = connections.get_mut(&self.game_id) {
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
            drop(connections);
            drop(games);
        }
        
        // Update this connection's game ID and color
        self.game_id = game_id.clone();
        self.color = Some(Color::White);

        // Create a new chess game
        let mut games = self.app_state.games.lock().unwrap();
        let game = Game::new();
        let game_status = get_game_status(&game);
        
        // Store the game state
        games.insert(
            game_id.clone(),
            GameState {
                game,
                white_player: Some(self.id.clone()),
                black_player: None,
            },
        );
        info!("Created new game {} with player {} as white", game_id, self.id);

        // Add this connection to the game's connections list
        let mut connections = self.app_state.connections.lock().unwrap();
        connections.insert(game_id.clone(), vec![self.id.clone()]);
        info!("Added player {} to connections for game {}", self.id, game_id);

        // Debug: Log all available games after creation
        info!("Available games after creation: {:?}", games.keys().collect::<Vec<_>>());

        // Send the game created message back to the client
        let msg = ServerMessage {
            message_type: "game_created".to_string(),
            game_id: Some(game_id),
            fen: Some(games.get(&self.game_id).unwrap().game.current_position().to_string()),
            color: Some("white".to_string()),
            error: None,
            available_moves: None,
            last_move: None,
            game_status: Some(game_status),
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
                let old_game_id = self.game_id.clone();
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
                    };
                    ctx.text(serde_json::to_string(&error_msg).unwrap());
                    return;
                };
                
                // Update this connection's game ID and color
                self.game_id = game_id.clone();
                self.color = Some(player_color);
                
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
                let game_status = get_game_status(&game_state.game);
                
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
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
        }
    }

    fn handle_move(&mut self, from: String, to: String, ctx: &mut ws::WebsocketContext<Self>) {
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
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
            return;
        }

        let mut games = self.app_state.games.lock().unwrap();
        if let Some(game_state) = games.get_mut(&self.game_id) {
            // Check if it's this player's turn
            let current_turn = game_state.game.side_to_move();
            if self.color != Some(current_turn) {
                let error_msg = ServerMessage {
                    message_type: "error".to_string(),
                    game_id: Some(self.game_id.clone()),
                    fen: None,
                    color: None,
                    error: Some("Not your turn".to_string()),
                    available_moves: None,
                    last_move: None,
                    game_status: None,
                };
                ctx.text(serde_json::to_string(&error_msg).unwrap());
                return;
            }

            // Parse the move
            let from_square = Square::from_str(&from.to_lowercase()).unwrap();
            let to_square = Square::from_str(&to.to_lowercase()).unwrap();

            // Create the chess move (handling promotion to queen by default)
            let chess_move = ChessMove::new(from_square, to_square, None);

            // Try to apply the move
            if game_state.game.make_move(chess_move) {
                // Move was successful
                let game_status = get_game_status(&game_state.game);
                let last_move = LastMove {
                    from,
                    to,
                };

                let msg = ServerMessage {
                    message_type: "move_made".to_string(),
                    game_id: Some(self.game_id.clone()),
                    fen: Some(game_state.game.current_position().to_string()),
                    color: None,
                    error: None,
                    available_moves: None,
                    last_move: Some(last_move),
                    game_status: Some(game_status),
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
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
        }
    }

    fn handle_get_moves(&mut self, from: String, ctx: &mut ws::WebsocketContext<Self>) {
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
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
            return;
        }

        let games = self.app_state.games.lock().unwrap();
        if let Some(game_state) = games.get(&self.game_id) {
            // Parse the from square
            let from_square = Square::from_str(&from.to_lowercase()).unwrap();
            let board = game_state.game.current_position();

            // Check if there's a piece at the square
            if let Some(piece) = board.piece_on(from_square) {
                // Check if the piece belongs to the player
                let piece_color = board.color_on(from_square).unwrap();
                if self.color != Some(piece_color) {
                    let error_msg = ServerMessage {
                        message_type: "error".to_string(),
                        game_id: Some(self.game_id.clone()),
                        fen: None,
                        color: None,
                        error: Some("Not your piece".to_string()),
                        available_moves: None,
                        last_move: None,
                        game_status: None,
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
            };
            ctx.text(serde_json::to_string(&error_msg).unwrap());
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

fn get_game_status(game: &Game) -> String {
    match game.result() {
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
