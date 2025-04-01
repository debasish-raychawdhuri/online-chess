use chess::{Board, Color, Game, GameResult, Piece, Square};

/// Convert a chess color to a string
pub fn color_to_string(color: Color) -> String {
    match color {
        Color::White => "white".to_string(),
        Color::Black => "black".to_string(),
    }
}

/// Get the game status as a string
pub fn get_game_status(game: &Game, game_result: Option<GameResult>) -> String {
    if let Some(result) = game_result {
        match result {
            GameResult::WhiteCheckmates => "white_wins".to_string(),
            GameResult::BlackCheckmates => "black_wins".to_string(),
            GameResult::WhiteResigns => "black_wins".to_string(),
            GameResult::BlackResigns => "white_wins".to_string(),
            GameResult::DrawAccepted => "draw".to_string(),
            GameResult::DrawDeclared => "draw".to_string(),
            GameResult::Stalemate => "stalemate".to_string(),
        }
    } else {
        let position = game.current_position();
        match position.status() {
            chess::BoardStatus::Ongoing => "in_progress".to_string(),
            chess::BoardStatus::Check => "check".to_string(),
            chess::BoardStatus::Checkmate => "checkmate".to_string(),
            chess::BoardStatus::Stalemate => "stalemate".to_string(),
        }
    }
}

/// Check if the board has insufficient material for checkmate
pub fn has_insufficient_material(board: &Board) -> bool {
    // Count pieces for each side
    let mut white_knights = 0;
    let mut white_bishops = 0;
    let mut white_rooks = 0;
    let mut white_queens = 0;
    let mut white_pawns = 0;
    
    let mut black_knights = 0;
    let mut black_bishops = 0;
    let mut black_rooks = 0;
    let mut black_queens = 0;
    let mut black_pawns = 0;
    
    // White bishop square colors
    let mut white_bishop_on_white = false;
    let mut white_bishop_on_black = false;
    
    // Black bishop square colors
    let mut black_bishop_on_white = false;
    let mut black_bishop_on_black = false;
    
    // Count all pieces on the board
    for square in Square::ALL {
        if let Some(piece) = board.piece_on(square) {
            let color = board.color_on(square).unwrap();
            
            match (color, piece) {
                (Color::White, Piece::Knight) => white_knights += 1,
                (Color::White, Piece::Bishop) => {
                    white_bishops += 1;
                    // Check if bishop is on white or black square
                    if (square.get_rank().to_index() + square.get_file().to_index()) % 2 == 0 {
                        white_bishop_on_white = true;
                    } else {
                        white_bishop_on_black = true;
                    }
                },
                (Color::White, Piece::Rook) => white_rooks += 1,
                (Color::White, Piece::Queen) => white_queens += 1,
                (Color::White, Piece::Pawn) => white_pawns += 1,
                
                (Color::Black, Piece::Knight) => black_knights += 1,
                (Color::Black, Piece::Bishop) => {
                    black_bishops += 1;
                    // Check if bishop is on white or black square
                    if (square.get_rank().to_index() + square.get_file().to_index()) % 2 == 0 {
                        black_bishop_on_white = true;
                    } else {
                        black_bishop_on_black = true;
                    }
                },
                (Color::Black, Piece::Rook) => black_rooks += 1,
                (Color::Black, Piece::Queen) => black_queens += 1,
                (Color::Black, Piece::Pawn) => black_pawns += 1,
                _ => {}, // Kings are always present
            }
        }
    }
    
    // Check for insufficient material scenarios
    
    // King vs King
    if white_knights == 0 && white_bishops == 0 && white_rooks == 0 && white_queens == 0 && white_pawns == 0 &&
       black_knights == 0 && black_bishops == 0 && black_rooks == 0 && black_queens == 0 && black_pawns == 0 {
        return true;
    }
    
    // King and Bishop vs King
    if (white_knights == 0 && white_bishops == 1 && white_rooks == 0 && white_queens == 0 && white_pawns == 0 &&
        black_knights == 0 && black_bishops == 0 && black_rooks == 0 && black_queens == 0 && black_pawns == 0) ||
       (white_knights == 0 && white_bishops == 0 && white_rooks == 0 && white_queens == 0 && white_pawns == 0 &&
        black_knights == 0 && black_bishops == 1 && black_rooks == 0 && black_queens == 0 && black_pawns == 0) {
        return true;
    }
    
    // King and Knight vs King
    if (white_knights == 1 && white_bishops == 0 && white_rooks == 0 && white_queens == 0 && white_pawns == 0 &&
        black_knights == 0 && black_bishops == 0 && black_rooks == 0 && black_queens == 0 && black_pawns == 0) ||
       (white_knights == 0 && white_bishops == 0 && white_rooks == 0 && white_queens == 0 && white_pawns == 0 &&
        black_knights == 1 && black_bishops == 0 && black_rooks == 0 && black_queens == 0 && black_pawns == 0) {
        return true;
    }
    
    // King and Bishop vs King and Bishop (bishops on same color)
    if white_knights == 0 && white_bishops == 1 && white_rooks == 0 && white_queens == 0 && white_pawns == 0 &&
       black_knights == 0 && black_bishops == 1 && black_rooks == 0 && black_queens == 0 && black_pawns == 0 {
        // If both bishops are on the same color squares, it's a draw
        if (white_bishop_on_white && black_bishop_on_white) || (white_bishop_on_black && black_bishop_on_black) {
            return true;
        }
    }
    
    // All other positions have sufficient material for checkmate
    false
}
