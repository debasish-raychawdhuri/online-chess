/**
 * Chess.js - A simple chess utility library
 * This library provides helper functions for working with chess positions and FEN strings
 */

class Chess {
    constructor() {
        this.PIECES = {
            'p': '♟', 'r': '♜', 'n': '♞', 'b': '♝', 'q': '♛', 'k': '♚',  // black pieces
            'P': '♙', 'R': '♖', 'N': '♘', 'B': '♗', 'Q': '♕', 'K': '♔'   // white pieces
        };
        
        this.FILES = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
        this.RANKS = ['8', '7', '6', '5', '4', '3', '2', '1'];
    }
    
    /**
     * Parse a FEN string into a board representation
     * @param {string} fen - The FEN string to parse
     * @returns {Object} - An object with the board position
     */
    parseFen(fen) {
        const parts = fen.split(' ');
        const position = parts[0];
        const rows = position.split('/');
        
        const board = {};
        
        for (let rankIndex = 0; rankIndex < 8; rankIndex++) {
            const row = rows[rankIndex];
            let fileIndex = 0;
            
            for (let i = 0; i < row.length; i++) {
                const char = row[i];
                
                if (/\d/.test(char)) {
                    // Skip empty squares
                    fileIndex += parseInt(char, 10);
                } else {
                    // Place piece on the board
                    const square = this.FILES[fileIndex] + this.RANKS[rankIndex];
                    board[square] = char;
                    fileIndex++;
                }
            }
        }
        
        return {
            board,
            activeColor: parts[1],
            castling: parts[2],
            enPassant: parts[3],
            halfMoveClock: parseInt(parts[4], 10),
            fullMoveNumber: parseInt(parts[5], 10)
        };
    }
    
    /**
     * Get the piece symbol for a piece
     * @param {string} piece - The piece code (e.g., 'K', 'p')
     * @returns {string} - The Unicode symbol for the piece
     */
    getPieceSymbol(piece) {
        return this.PIECES[piece] || '';
    }
    
    /**
     * Get the color of a piece
     * @param {string} piece - The piece code (e.g., 'K', 'p')
     * @returns {string} - 'white' or 'black'
     */
    getPieceColor(piece) {
        if (!piece) return null;
        return piece.toUpperCase() === piece ? 'white' : 'black';
    }
    
    /**
     * Check if a square has algebraic notation format (e.g., 'a1')
     * @param {string} square - The square to check
     * @returns {boolean} - Whether the square has valid notation
     */
    isValidSquare(square) {
        if (typeof square !== 'string' || square.length !== 2) return false;
        const file = square[0].toLowerCase();
        const rank = square[1];
        return this.FILES.includes(file) && this.RANKS.includes(rank);
    }
    
    /**
     * Get all squares on the board
     * @returns {Array} - Array of all square names (e.g., ['a1', 'a2', ...])
     */
    getAllSquares() {
        const squares = [];
        for (const rank of this.RANKS) {
            for (const file of this.FILES) {
                squares.push(file + rank);
            }
        }
        return squares;
    }
    
    /**
     * Get the square color (black or white)
     * @param {string} square - The square name (e.g., 'a1')
     * @returns {string} - 'black' or 'white'
     */
    getSquareColor(square) {
        if (!this.isValidSquare(square)) return null;
        
        const file = this.FILES.indexOf(square[0].toLowerCase());
        const rank = this.RANKS.indexOf(square[1]);
        
        // If the sum of file and rank is odd, the square is black, otherwise white
        return (file + rank) % 2 === 1 ? 'black' : 'white';
    }
}
