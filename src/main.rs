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
    game_id: String,
    color: Option<Color>,
    app_state: web::Data<AppState>,
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
        let id = self.id.clone();
        self.app_state.sessions.lock().unwrap().insert(id, ctx.address());
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        // Remove the actor from the application state
        self.app_state.sessions.lock().unwrap().remove(&self.id);

        // Remove from game connections if part of a game
        if !self.game_id.is_empty() {
            let mut connections = self.app_state.connections.lock().unwrap();
            if let Some(conn_list) = connections.get_mut(&self.game_id) {
                conn_list.retain(|id| id != &self.id);
            }
        }
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
            Ok(ws::Message::Text(text)) => {
                self.handle_message(text.to_string(), ctx);
            }
            Ok(ws::Message::Ping(msg)) => {
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => {}
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
                warn!("Error parsing message: {}", e);
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
        info!("Broadcasting message to game {}", game_id);
        let connections = self.app_state.connections.lock().unwrap();
        let sessions = self.app_state.sessions.lock().unwrap();
        
        if let Some(connection_ids) = connections.get(game_id) {
            info!("Found {} connections for game {}", connection_ids.len(), game_id);
            let msg_str = serde_json::to_string(message).unwrap();
            
            for connection_id in connection_ids {
                info!("Attempting to send to connection {}", connection_id);
                if let Some(addr) = sessions.get(connection_id) {
                    info!("Sending message to connection {}", connection_id);
                    addr.do_send(ChessWebSocketMessage(msg_str.clone()));
                } else {
                    info!("Connection {} not found in sessions", connection_id);
                }
            }
        } else {
            info!("No connections found for game {}", game_id);
        }
    }

    fn handle_create(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        let game_id = Uuid::new_v4().to_string();
        self.game_id = game_id.clone();
        
        // Assign the creator as the white player
        self.color = Some(Color::White);

        let mut games = self.app_state.games.lock().unwrap();
        let game = Game::new();
        let game_status = get_game_status(&game);
        
        games.insert(
            game_id.clone(),
            GameState {
                game,
                white_player: Some(self.id.clone()),
                black_player: None,
            },
        );

        let mut connections = self.app_state.connections.lock().unwrap();
        connections.insert(game_id.clone(), vec![self.id.clone()]);

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

        ctx.text(serde_json::to_string(&msg).unwrap());
    }

    fn handle_join(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        if let Some(game_id) = msg.game_id {
            info!("Player {} joining game {}", self.id, game_id);
            let mut games = self.app_state.games.lock().unwrap();
            
            // Debug: Log all available games
            info!("Available games: {:?}", games.keys().collect::<Vec<_>>());
            
            if let Some(game_state) = games.get_mut(&game_id) {
                // Determine player color
                let player_color = if game_state.white_player.is_none() {
                    info!("Assigning player as white");
                    game_state.white_player = Some(self.id.clone());
                    Color::White
                } else if game_state.black_player.is_none() {
                    info!("Assigning player as black");
                    game_state.black_player = Some(self.id.clone());
                    Color::Black
                } else {
                    // Game is full
                    info!("Cannot join game: Game is full");
                    let error_msg = ServerMessage {
                        message_type: "error".to_string(),
                        game_id: Some(game_id.clone()),
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

                // Add player to the connections list for this game
                let mut connections = self.app_state.connections.lock().unwrap();
                if let Some(conn_list) = connections.get_mut(&game_id) {
                    conn_list.push(self.id.clone());
                } else {
                    connections.insert(game_id.clone(), vec![self.id.clone()]);
                }

                // Set the game ID and color for this connection
                self.game_id = game_id.clone();
                self.color = Some(player_color);

                // Send game state to the player
                let msg = ServerMessage {
                    message_type: "joined".to_string(),
                    game_id: Some(game_id.clone()),
                    fen: Some(game_state.game.current_position().to_string()),
                    color: Some(color_to_string(player_color)),
                    error: None,
                    available_moves: None,
                    last_move: None,
                    game_status: Some(get_game_status(&game_state.game)),
                };
                info!("Sending joined message to player: {:?}", msg);
                
                // Send the message directly to the client
                let msg_str = serde_json::to_string(&msg).unwrap();
                ctx.text(msg_str);
                
                // Notify other players
                let update_msg = ServerMessage {
                    message_type: "player_joined".to_string(),
                    game_id: Some(game_id.clone()),
                    fen: Some(game_state.game.current_position().to_string()),
                    color: None,
                    error: None,
                    available_moves: None,
                    last_move: None,
                    game_status: Some(get_game_status(&game_state.game)),
                };
                info!("Broadcasting player_joined message to all players");
                drop(games); // Release the lock before broadcasting
                self.broadcast_to_game(&self.game_id, &update_msg);
            } else {
                info!("Cannot join game: Game not found with ID {}", game_id);
                let error_msg = ServerMessage {
                    message_type: "error".to_string(),
                    game_id: Some(game_id.clone()),
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
            info!("Cannot join game: No game ID provided");
            let error_msg = ServerMessage {
                message_type: "error".to_string(),
                game_id: None,
                fen: None,
                color: None,
                error: Some("No game ID provided".to_string()),
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
    let id = Uuid::new_v4().to_string();
    info!("New WebSocket connection: {}", id);
    
    let ws = ChessWebSocket {
        id,
        game_id: String::new(),
        color: None,
        app_state: app_state.clone(),
    };
    
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
