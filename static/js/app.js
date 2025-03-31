// Event listeners
document.addEventListener('DOMContentLoaded', () => {
    // Initialize elements
    const createGameBtn = document.getElementById('create-game');
    const joinGameBtn = document.getElementById('join-game');
    const gameIdInput = document.getElementById('game-id-input');
    const gameIdDisplay = document.getElementById('game-id-display');
    const playerColorDisplay = document.getElementById('player-color');
    const playerInfo = document.getElementById('player-info');
    const gameStatus = document.getElementById('game-status');
    const connectionStatus = document.getElementById('connection-status');
    const chessboard = document.getElementById('chessboard');

    // Game state
    let socket;
    let isConnected = false;
    let gameId = '';
    let playerColor = '';
    let selectedSquare = null;
    let isMyTurn = false;
    let board = {};
    let validMoves = [];

    // Initialize WebSocket connection
    const connectWebSocket = () => {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/ws`;
        console.log('Connecting to WebSocket at:', wsUrl);
        
        socket = new WebSocket(wsUrl);
        
        socket.onopen = () => {
            console.log('WebSocket connection established');
            isConnected = true;
            createGameBtn.disabled = false;
            joinGameBtn.disabled = false;
            gameIdInput.disabled = false;
            connectionStatus.textContent = 'Connected';
            connectionStatus.style.color = 'green';
            gameStatus.textContent = 'Connected to server. Create a new game or join an existing one.';
        };
        
        socket.onclose = (event) => {
            console.log('WebSocket connection closed:', event);
            isConnected = false;
            createGameBtn.disabled = true;
            joinGameBtn.disabled = true;
            gameIdInput.disabled = true;
            connectionStatus.textContent = 'Disconnected';
            connectionStatus.style.color = 'red';
            gameStatus.textContent = 'Connection lost. Please refresh the page.';
            
            // Attempt to reconnect after a delay
            setTimeout(connectWebSocket, 3000);
        };
        
        socket.onerror = (error) => {
            console.error('WebSocket error:', error);
            connectionStatus.textContent = 'Connection Error';
            connectionStatus.style.color = 'red';
        };
        
        socket.onmessage = (event) => {
            console.log('Message received:', event.data);
            handleMessage(JSON.parse(event.data));
        };
    };

    // Handle messages from the server
    const handleMessage = (message) => {
        console.log('Handling message:', message);
        
        switch (message.message_type) {
            case 'game_created':
                gameId = message.game_id;
                playerColor = message.color;
                gameIdDisplay.textContent = `Game ID: ${gameId}`;
                playerColorDisplay.textContent = `You are playing as: ${playerColor}`;
                playerInfo.style.display = 'flex';
                gameStatus.textContent = `Game created. Waiting for opponent to join. Share the Game ID: ${gameId}`;
                updateBoard(message.fen, true);
                break;
                
            case 'joined':
                gameId = message.game_id;
                playerColor = message.color;
                gameIdDisplay.textContent = `Game ID: ${gameId}`;
                playerColorDisplay.textContent = `You are playing as: ${playerColor}`;
                playerInfo.style.display = 'flex';
                gameStatus.textContent = `You joined the game as ${playerColor}`;
                updateBoard(message.fen, true);
                break;
                
            case 'player_joined':
                gameStatus.textContent = 'Opponent joined the game. Game is starting!';
                updateBoard(message.fen, true);
                break;
                
            case 'move_made':
                updateBoard(message.fen, true);
                if (message.last_move) {
                    highlightLastMove(message.last_move);
                }
                if (message.game_status) {
                    updateGameStatus(message.game_status);
                }
                break;
                
            case 'available_moves':
                if (message.available_moves && message.available_moves.length > 0) {
                    validMoves = message.available_moves;
                    highlightValidMoves(validMoves);
                }
                break;
                
            case 'error':
                console.error('Error from server:', message.error);
                gameStatus.textContent = `Error: ${message.error}`;
                break;
                
            default:
                console.log('Unknown message type:', message.message_type);
        }
    };

    // Create a new game
    createGameBtn.addEventListener('click', () => {
        if (!isConnected) {
            console.error('Cannot create game: WebSocket not connected');
            return;
        }
        
        const message = {
            action: 'create'
        };
        
        socket.send(JSON.stringify(message));
        gameStatus.textContent = 'Creating a new game...';
    });

    // Join an existing game
    joinGameBtn.addEventListener('click', () => {
        if (!isConnected) {
            console.error('Cannot join game: WebSocket not connected');
            return;
        }
        
        const gameIdToJoin = gameIdInput.value.trim();
        if (!gameIdToJoin) {
            gameStatus.textContent = 'Please enter a valid Game ID';
            return;
        }
        
        const message = {
            action: 'join',
            game_id: gameIdToJoin
        };
        
        socket.send(JSON.stringify(message));
        gameStatus.textContent = 'Attempting to join game...';
    });

    // Initialize the chessboard
    const initializeBoard = () => {
        // Clear the chessboard
        chessboard.innerHTML = '';
        
        // Create chess utility
        const chess = new Chess();
        
        // Create the squares
        for (let rank = 0; rank < 8; rank++) {
            for (let file = 0; file < 8; file++) {
                const squareName = chess.FILES[file] + chess.RANKS[rank];
                const square = document.createElement('div');
                square.className = `square ${chess.getSquareColor(squareName)}`;
                square.dataset.square = squareName;
                square.addEventListener('click', () => handleSquareClick(squareName));
                chessboard.appendChild(square);
            }
        }
    };

    // Update the board based on FEN string
    const updateBoard = (fen, checkTurn = false) => {
        if (!fen) return;
        
        // Create chess utility
        const chess = new Chess();
        
        // Parse the FEN string
        const position = chess.parseFen(fen);
        board = position.board;
        
        // Update the turn status
        if (checkTurn) {
            isMyTurn = (position.activeColor === 'w' && playerColor === 'white') || 
                      (position.activeColor === 'b' && playerColor === 'black');
            
            console.log('Turn status:', { 
                activeColor: position.activeColor, 
                playerColor, 
                isMyTurn 
            });
        }
        
        // Clear all squares
        const squares = document.querySelectorAll('.square');
        squares.forEach(square => {
            square.innerHTML = '';
        });
        
        // Place pieces on the board
        for (const [square, piece] of Object.entries(board)) {
            const squareElement = document.querySelector(`.square[data-square="${square}"]`);
            if (squareElement && chessPieces[piece]) {
                const pieceElement = document.createElement('div');
                pieceElement.className = 'piece';
                pieceElement.innerHTML = chessPieces[piece];
                squareElement.appendChild(pieceElement);
            }
        }
        
        // Clear highlights
        clearHighlights();
    };

    // Handle square click
    const handleSquareClick = (squareName) => {
        console.log('Square clicked:', squareName);
        
        if (!gameId || !isMyTurn) {
            console.log('Not your turn or not in a game');
            return;
        }
        
        // Create chess utility
        const chess = new Chess();
        
        // If no square is selected, check if the clicked square has a piece of the player's color
        if (!selectedSquare) {
            const piece = board[squareName];
            if (!piece) {
                console.log('No piece on this square');
                return;
            }
            
            const pieceColor = chess.getPieceColor(piece);
            if (pieceColor !== playerColor) {
                console.log('Not your piece');
                return;
            }
            
            // Select the square and get valid moves
            selectedSquare = squareName;
            highlightSquare(squareName);
            
            // Request valid moves from the server
            const message = {
                action: 'get_moves',
                move_from: squareName
            };
            
            socket.send(JSON.stringify(message));
        } else {
            // If a square is already selected, try to make a move
            if (validMoves.includes(squareName)) {
                // Make the move
                const message = {
                    action: 'move',
                    move_from: selectedSquare,
                    move_to: squareName
                };
                
                socket.send(JSON.stringify(message));
                
                // Reset selection
                selectedSquare = null;
                validMoves = [];
                clearHighlights();
            } else if (squareName === selectedSquare) {
                // Deselect the square if clicked again
                selectedSquare = null;
                validMoves = [];
                clearHighlights();
            } else {
                // Check if the new square has a piece of the player's color
                const piece = board[squareName];
                if (piece && chess.getPieceColor(piece) === playerColor) {
                    // Select the new square
                    selectedSquare = squareName;
                    clearHighlights();
                    highlightSquare(squareName);
                    
                    // Request valid moves for the new square
                    const message = {
                        action: 'get_moves',
                        move_from: squareName
                    };
                    
                    socket.send(JSON.stringify(message));
                }
            }
        }
    };

    // Get valid moves for a piece
    const getValidMoves = (squareName) => {
        if (!isConnected || !gameId) return;
        
        const message = {
            action: 'get_moves',
            game_id: gameId,
            move_from: squareName
        };
        
        socket.send(JSON.stringify(message));
    };

    // Highlight a square
    const highlightSquare = (squareName) => {
        const square = document.querySelector(`.square[data-square="${squareName}"]`);
        if (square) {
            square.classList.add('highlight');
        }
    };

    // Highlight valid moves
    const highlightValidMoves = (moves) => {
        moves.forEach(move => {
            const square = document.querySelector(`.square[data-square="${move}"]`);
            if (square) {
                square.classList.add('valid-move');
            }
        });
    };

    // Highlight the last move
    const highlightLastMove = (move) => {
        const fromSquare = document.querySelector(`.square[data-square="${move.from}"]`);
        const toSquare = document.querySelector(`.square[data-square="${move.to}"]`);
        
        if (fromSquare) fromSquare.classList.add('last-move');
        if (toSquare) toSquare.classList.add('last-move');
    };

    // Clear all highlights
    const clearHighlights = () => {
        const squares = document.querySelectorAll('.square');
        squares.forEach(square => {
            square.classList.remove('highlight', 'valid-move', 'last-move');
        });
    };

    // Update game status based on server message
    const updateGameStatus = (status) => {
        switch (status) {
            case 'white_turn':
                gameStatus.textContent = playerColor === 'white' ? 
                    'Your turn (White)' : 'Waiting for opponent\'s move (White)';
                break;
                
            case 'black_turn':
                gameStatus.textContent = playerColor === 'black' ? 
                    'Your turn (Black)' : 'Waiting for opponent\'s move (Black)';
                break;
                
            case 'check':
                const inCheck = (isMyTurn) ? 'You are in check!' : 'Your opponent is in check!';
                gameStatus.textContent = inCheck;
                break;
                
            case 'white_wins':
                gameStatus.textContent = 'Game over: White wins!';
                break;
                
            case 'black_wins':
                gameStatus.textContent = 'Game over: Black wins!';
                break;
                
            case 'draw':
                gameStatus.textContent = 'Game over: Draw!';
                break;
                
            default:
                gameStatus.textContent = `Game status: ${status}`;
        }
    };

    // Initialize the game
    initializeBoard();
    connectWebSocket();
});
