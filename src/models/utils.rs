use chess::{Color, Game, GameResult, Board};

/// Convert a chess Color to a string representation
pub fn color_to_string(color: Color) -> String {
    match color {
        Color::White => "white".to_string(),
        Color::Black => "black".to_string(),
    }
}

/// Get the current game status as a string
pub fn get_game_status(game: &Game, game_result: Option<GameResult>) -> String {
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

/// Check if the board has insufficient material for checkmate
pub fn has_insufficient_material(board: &Board) -> bool {
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
