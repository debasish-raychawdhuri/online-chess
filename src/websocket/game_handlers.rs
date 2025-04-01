use actix_web_actors::ws;
use chess::{ChessMove, Color, Game, GameResult, MoveGen, Piece, Square};
use log::{info, warn};
use std::str::FromStr;
use std::time::Instant;
use uuid::Uuid;

use crate::models::messages::{ClientMessage, ServerMessage, LastMove};
use crate::models::game_state::GameState;
use crate::game::utils::{color_to_string, get_game_status, has_insufficient_material};
use crate::websocket::handler::ChessWebSocket;

impl ChessWebSocket {
    pub fn handle_create(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        info!("Creating new game");
        
        // Parse the time control from the message
        let time_control = msg.data.parse::<u64>().unwrap_or(600000); // Default to 10 minutes
        let increment = msg.increment.unwrap_or(0); // Default to 0 seconds increment
        
        // Generate a unique game ID
        let game_id = Uuid::new_v4().to_string();
        info!("Generated game ID: {}", game_id);
        
        // Update the websocket with the game ID
        self.game_id = game_id.clone();
        
        // Determine the player's color (creator is always white)
        self.color = Some(Color::White);
        
        // Create a new game state
        let game_state = GameState {
            game: Game::new(),
            white_player: Some(self.id.clone()),
            black_player: None,
            white_time_ms: time_control,
            black_time_ms: time_control,
            increment_ms: increment,
            last_move_time: None,
            active_player: Some(Color::White), // White always starts
            game_result: None,
        };
        
        // Add the game state to the application state
        {
            let mut games = self.app_state.games.lock().unwrap();
            games.insert(game_id.clone(), game_state);
        }
        
        // Add the player to the game connections
        {
            let mut connections = self.app_state.connections.lock().unwrap();
            connections.insert(game_id.clone(), vec![self.id.clone()]);
        }
        
        // Send a response to the client
        let response = ServerMessage {
            message_type: "game_created".to_string(),
            game_id: Some(game_id.clone()),
            color: Some("white".to_string()),
            fen: Some(Game::new().current_position().to_string()),
            time_white: Some(time_control),
            time_black: Some(time_control),
            increment: Some(increment),
            status: Some("waiting".to_string()),
            last_move: None,
            available_moves: None,
            error: None,
        };
        
        info!("Sending game created response: {:?}", response);
        
        if let Ok(response_str) = serde_json::to_string(&response) {
            ctx.text(response_str);
        } else {
            warn!("Failed to serialize response");
            ctx.text("{\"error\": \"Internal server error\"}");
        }
    }
    
    pub fn handle_join(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        info!("Joining game: {}", msg.game_id.clone().unwrap_or_default());
        
        // Get the game ID from the message
        let game_id = match msg.game_id {
            Some(id) => id,
            None => {
                warn!("No game ID provided");
                ctx.text("{\"error\": \"No game ID provided\"}");
                return;
            }
        };
        
        // Update the websocket with the game ID
        self.game_id = game_id.clone();
        
        // Check if the game exists
        let mut game_state = {
            let mut games = self.app_state.games.lock().unwrap();
            match games.get_mut(&game_id) {
                Some(state) => state.clone(),
                None => {
                    warn!("Game not found: {}", game_id);
                    ctx.text("{\"error\": \"Game not found\"}");
                    return;
                }
            }
        };
        
        // Determine the player's color
        let color = if game_state.black_player.is_none() {
            // Join as black if available
            game_state.black_player = Some(self.id.clone());
            self.color = Some(Color::Black);
            "black"
        } else if game_state.white_player.is_none() {
            // Join as white if available
            game_state.white_player = Some(self.id.clone());
            self.color = Some(Color::White);
            "white"
        } else {
            // Join as spectator
            self.color = None;
            "spectator"
        };
        
        // Update the game state if joining as a player
        if color != "spectator" {
            let mut games = self.app_state.games.lock().unwrap();
            if let Some(state) = games.get_mut(&game_id) {
                if color == "black" {
                    state.black_player = Some(self.id.clone());
                } else if color == "white" {
                    state.white_player = Some(self.id.clone());
                }
                
                // If both players are now present, start the game
                if state.white_player.is_some() && state.black_player.is_some() {
                    state.last_move_time = Some(Instant::now());
                }
            }
        }
        
        // Add the player to the game connections
        {
            let mut connections = self.app_state.connections.lock().unwrap();
            if let Some(connection_ids) = connections.get_mut(&game_id) {
                if !connection_ids.contains(&self.id) {
                    connection_ids.push(self.id.clone());
                }
            } else {
                connections.insert(game_id.clone(), vec![self.id.clone()]);
            }
        }
        
        // Get the current game state
        let current_state = {
            let games = self.app_state.games.lock().unwrap();
            games.get(&game_id).unwrap().clone()
        };
        
        // Determine the game status
        let status = if current_state.white_player.is_some() && current_state.black_player.is_some() {
            get_game_status(&current_state.game, current_state.game_result)
        } else {
            "waiting".to_string()
        };
        
        // Send the current game state to the player
        let response = ServerMessage {
            message_type: "game_joined".to_string(),
            game_id: Some(game_id.clone()),
            color: Some(color.to_string()),
            fen: Some(current_state.game.current_position().to_string()),
            time_white: Some(current_state.white_time_ms),
            time_black: Some(current_state.black_time_ms),
            increment: Some(current_state.increment_ms),
            status: Some(status.clone()),
            last_move: None, // No last move when joining
            available_moves: None,
            error: None,
        };
        
        info!("Sending game joined response: {:?}", response);
        
        if let Ok(response_str) = serde_json::to_string(&response) {
            ctx.text(response_str);
        } else {
            warn!("Failed to serialize response");
            ctx.text("{\"error\": \"Internal server error\"}");
        }
        
        // Notify other players that someone has joined
        if color != "spectator" {
            let notification = ServerMessage {
                message_type: "player_joined".to_string(),
                game_id: Some(game_id.clone()),
                color: Some(color.to_string()),
                fen: Some(current_state.game.current_position().to_string()),
                time_white: Some(current_state.white_time_ms),
                time_black: Some(current_state.black_time_ms),
                increment: Some(current_state.increment_ms),
                status: Some(status),
                last_move: None,
                available_moves: None,
                error: None,
            };
            
            self.broadcast_to_game(&game_id, &notification);
        }
    }
    
    pub fn handle_move(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        info!("Processing move: {:?}", msg);
        
        // Check if the player is in a game
        if self.game_id.is_empty() {
            warn!("Player is not in a game");
            ctx.text("{\"error\": \"Not in a game\"}");
            return;
        }
        
        // Check if the player has a color assigned
        let player_color = match self.color {
            Some(color) => color,
            None => {
                warn!("Player does not have a color assigned");
                ctx.text("{\"error\": \"You are a spectator\"}");
                return;
            }
        };
        
        // Get the move from the message
        let move_str = match &msg.move_str {
            Some(m) => m.clone(),
            None => {
                warn!("No move provided");
                ctx.text("{\"error\": \"No move provided\"}");
                return;
            }
        };
        
        // Get the game state
        let mut games = self.app_state.games.lock().unwrap();
        let game_state = match games.get_mut(&self.game_id) {
            Some(state) => state,
            None => {
                warn!("Game not found: {}", self.game_id);
                ctx.text("{\"error\": \"Game not found\"}");
                return;
            }
        };
        
        // Check if the game is over
        if game_state.game_result.is_some() {
            warn!("Game is already over");
            ctx.text("{\"error\": \"Game is already over\"}");
            return;
        }
        
        // Check if it's the player's turn
        if game_state.active_player != Some(player_color) {
            warn!("Not player's turn");
            ctx.text("{\"error\": \"Not your turn\"}");
            return;
        }
        
        // Parse the move
        let chess_move = match ChessMove::from_str(&move_str) {
            Ok(m) => m,
            Err(e) => {
                warn!("Invalid move format: {}", e);
                ctx.text("{\"error\": \"Invalid move format\"}");
                return;
            }
        };
        
        // Check if the move is legal
        if !MoveGen::new_legal(&game_state.game.current_position()).any(|m| m == chess_move) {
            warn!("Illegal move: {}", move_str);
            ctx.text("{\"error\": \"Illegal move\"}");
            return;
        }
        
        // Make the move
        let from_square = chess_move.get_source();
        let to_square = chess_move.get_dest();
        let piece_moved = game_state.game.current_position().piece_on(from_square).unwrap();
        let is_capture = game_state.game.current_position().piece_on(to_square).is_some();
        let is_promotion = chess_move.get_promotion().is_some();
        
        // Update the game state
        game_state.game.make_move(chess_move);
        
        // Update the active player
        game_state.active_player = match game_state.active_player {
            Some(Color::White) => Some(Color::Black),
            Some(Color::Black) => Some(Color::White),
            None => None,
        };
        
        // Update the time control
        let now = Instant::now();
        if let Some(last_move_time) = game_state.last_move_time {
            let elapsed = now.duration_since(last_move_time).as_millis() as u64;
            
            // Update the time for the player who just moved
            match player_color {
                Color::White => {
                    if game_state.white_time_ms > elapsed {
                        game_state.white_time_ms -= elapsed;
                        // Add increment if not the first move
                        game_state.white_time_ms += game_state.increment_ms;
                    } else {
                        // White ran out of time
                        game_state.white_time_ms = 0;
                        game_state.game_result = Some(GameResult::BlackCheckmates); // Using checkmates as timeout
                    }
                },
                Color::Black => {
                    if game_state.black_time_ms > elapsed {
                        game_state.black_time_ms -= elapsed;
                        // Add increment if not the first move
                        game_state.black_time_ms += game_state.increment_ms;
                    } else {
                        // Black ran out of time
                        game_state.black_time_ms = 0;
                        game_state.game_result = Some(GameResult::WhiteCheckmates); // Using checkmates as timeout
                    }
                },
            }
        }
        game_state.last_move_time = Some(now);
        
        // Check for game end conditions
        let current_position = game_state.game.current_position();
        
        // Check for checkmate or stalemate
        if MoveGen::new_legal(&current_position).count() == 0 {
            if current_position.checkers().popcnt() > 0 {
                // Checkmate
                game_state.game_result = match player_color {
                    Color::White => Some(GameResult::WhiteCheckmates),
                    Color::Black => Some(GameResult::BlackCheckmates),
                };
            } else {
                // Stalemate
                game_state.game_result = Some(GameResult::Stalemate);
            }
        }
        
        // Check for insufficient material
        if has_insufficient_material(&current_position) {
            game_state.game_result = Some(GameResult::DrawDeclared);
        }
        
        // Create the last move info
        let last_move = LastMove {
            from: from_square.to_string(),
            to: to_square.to_string(),
            piece: piece_moved.to_string(),
            color: color_to_string(player_color),
            is_capture,
            is_promotion,
        };
        
        // Get the game status
        let status = get_game_status(&game_state.game, game_state.game_result);
        
        // Send the move result to all players
        let response = ServerMessage {
            message_type: "move_made".to_string(),
            game_id: Some(self.game_id.clone()),
            color: Some(color_to_string(player_color)),
            fen: Some(current_position.to_string()),
            time_white: Some(game_state.white_time_ms),
            time_black: Some(game_state.black_time_ms),
            increment: Some(game_state.increment_ms),
            status: Some(status),
            last_move: Some(last_move),
            available_moves: None,
            error: None,
        };
        
        // Broadcast the move to all players
        self.broadcast_to_game(&self.game_id, &response);
    }
    
    pub fn handle_get_moves(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        info!("Getting available moves: {:?}", msg);
        
        // Check if the player is in a game
        if self.game_id.is_empty() {
            warn!("Player is not in a game");
            ctx.text("{\"error\": \"Not in a game\"}");
            return;
        }
        
        // Get the square from the message
        let square_str = match &msg.square {
            Some(s) => s.clone(),
            None => {
                warn!("No square provided");
                ctx.text("{\"error\": \"No square provided\"}");
                return;
            }
        };
        
        // Parse the square
        let square = match Square::from_str(&square_str) {
            Ok(s) => s,
            Err(_) => {
                warn!("Invalid square format: {}", square_str);
                ctx.text("{\"error\": \"Invalid square format\"}");
                return;
            }
        };
        
        // Get the game state
        let games = self.app_state.games.lock().unwrap();
        let game_state = match games.get(&self.game_id) {
            Some(state) => state,
            None => {
                warn!("Game not found: {}", self.game_id);
                ctx.text("{\"error\": \"Game not found\"}");
                return;
            }
        };
        
        // Check if there's a piece on the square
        let current_position = game_state.game.current_position();
        if current_position.piece_on(square).is_none() {
            warn!("No piece on square: {}", square_str);
            ctx.text("{\"error\": \"No piece on square\"}");
            return;
        }
        
        // Check if it's the player's piece (if they are a player)
        if let Some(player_color) = self.color {
            if let Some(piece_color) = current_position.color_on(square) {
                if piece_color != player_color && game_state.active_player == Some(player_color) {
                    warn!("Not player's piece");
                    ctx.text("{\"error\": \"Not your piece\"}");
                    return;
                }
            }
        }
        
        // Get all legal moves for the piece on the square
        let mut available_moves = Vec::new();
        for chess_move in MoveGen::new_legal(&current_position) {
            if chess_move.get_source() == square {
                available_moves.push(chess_move.to_string());
            }
        }
        
        // Send the available moves to the player
        let response = ServerMessage {
            message_type: "available_moves".to_string(),
            game_id: Some(self.game_id.clone()),
            color: self.color.map(color_to_string),
            fen: Some(current_position.to_string()),
            time_white: Some(game_state.white_time_ms),
            time_black: Some(game_state.black_time_ms),
            increment: Some(game_state.increment_ms),
            status: Some(get_game_status(&game_state.game, game_state.game_result)),
            last_move: None,
            available_moves: Some(available_moves),
            error: None,
        };
        
        if let Ok(response_str) = serde_json::to_string(&response) {
            ctx.text(response_str);
        } else {
            warn!("Failed to serialize response");
            ctx.text("{\"error\": \"Internal server error\"}");
        }
    }
    
    pub fn handle_time_sync(&mut self, msg: ClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
        // Check if the player is in a game
        if self.game_id.is_empty() {
            warn!("Player is not in a game");
            ctx.text("{\"error\": \"Not in a game\"}");
            return;
        }
        
        // Get the game state
        let games = self.app_state.games.lock().unwrap();
        let game_state = match games.get(&self.game_id) {
            Some(state) => state,
            None => {
                warn!("Game not found: {}", self.game_id);
                ctx.text("{\"error\": \"Game not found\"}");
                return;
            }
        };
        
        // Send the current time to the player
        let response = ServerMessage {
            message_type: "time_sync".to_string(),
            game_id: Some(self.game_id.clone()),
            color: self.color.map(color_to_string),
            fen: Some(game_state.game.current_position().to_string()),
            time_white: Some(game_state.white_time_ms),
            time_black: Some(game_state.black_time_ms),
            increment: Some(game_state.increment_ms),
            status: Some(get_game_status(&game_state.game, game_state.game_result)),
            last_move: None,
            available_moves: None,
            error: None,
        };
        
        if let Ok(response_str) = serde_json::to_string(&response) {
            ctx.text(response_str);
        } else {
            warn!("Failed to serialize response");
            ctx.text("{\"error\": \"Internal server error\"}");
        }
    }
}
