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
    const startTimeSelect = document.getElementById('start-time');
    const incrementSelect = document.getElementById('increment');
    const whiteTimeDisplay = document.getElementById('white-time');
    const blackTimeDisplay = document.getElementById('black-time');

    // Game state
    let socket;
    let isConnected = false;
    let gameId = '';
    let playerColor = '';
    let isMyTurn = false;
    let selectedSquare = null;
    let board = {};
    let validMoves = [];
    let timerInterval = null;
    let whiteTimeMs = 900000; // 15 minutes in milliseconds
    let blackTimeMs = 900000; // 15 minutes in milliseconds
    let incrementMs = 10000; // 10 seconds in milliseconds
    let lastMoveTime = null;
    let activeColor = 'white';

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
            startTimeSelect.disabled = false;
            incrementSelect.disabled = false;
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
            startTimeSelect.disabled = true;
            incrementSelect.disabled = true;
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
                gameStatus.textContent = formatGameStatus(message.game_status || 'waiting_for_opponent');
                
                // Parse FEN and update board
                if (message.fen) {
                    const chess = new Chess();
                    const position = chess.parseFen(message.fen);
                    updateBoard(position, true);
                }
                
                handleGameCreated(message);
                break;
                
            case 'joined':
                gameId = message.game_id;
                playerColor = message.color;
                gameIdDisplay.textContent = `Game ID: ${gameId}`;
                playerColorDisplay.textContent = `You are playing as: ${playerColor}`;
                playerInfo.style.display = 'flex';
                gameStatus.textContent = formatGameStatus(message.game_status || 'in_progress');
                
                // Parse FEN and update board
                if (message.fen) {
                    const chess = new Chess();
                    const position = chess.parseFen(message.fen);
                    updateBoard(position, true);
                }
                
                break;
                
            case 'game_update':
                // Parse FEN and update board
                if (message.fen) {
                    const chess = new Chess();
                    const position = chess.parseFen(message.fen);
                    updateBoard(position, true);
                }
                
                // Update game status if provided
                if (message.game_status) {
                    gameStatus.textContent = formatGameStatus(message.game_status);
                }
                
                // Update last move if provided
                if (message.last_move) {
                    const { from, to } = message.last_move;
                    highlightLastMove(from, to);
                }
                
                // Clear selection and valid moves
                selectedSquare = null;
                validMoves = [];
                clearHighlights();
                break;
                
            case 'move_made':
                // Parse FEN and update board
                if (message.fen) {
                    const chess = new Chess();
                    const position = chess.parseFen(message.fen);
                    updateBoard(position, true);
                    
                    // Update active color based on the FEN (which indicates whose turn it is)
                    // In FEN, if the active color is 'w', it's White's turn, otherwise Black's turn
                    activeColor = position.activeColor === 'w' ? 'white' : 'black';
                    console.log('FEN:', message.fen);
                    console.log('Parsed position:', position);
                    console.log('Active color set to:', activeColor);
                    
                    // Reset the timer for the new active player
                    lastMoveTime = Date.now();
                }
                
                // Update game status if provided
                if (message.game_status) {
                    gameStatus.textContent = formatGameStatus(message.game_status);
                }
                
                // Update last move if provided
                if (message.last_move) {
                    const { from, to } = message.last_move;
                    highlightLastMove(from, to);
                }
                
                // Update timer values
                if (message.white_time_ms !== undefined) whiteTimeMs = message.white_time_ms;
                if (message.black_time_ms !== undefined) blackTimeMs = message.black_time_ms;
                
                // Update timers
                updateTimerDisplays();
                
                // Restart the timers to ensure they're running with the correct active player
                stopTimers();
                startTimers();
                
                // Clear selection and valid moves
                selectedSquare = null;
                validMoves = [];
                clearHighlights();
                break;
                
            case 'player_joined':
                // Update game status
                if (message.game_status) {
                    gameStatus.textContent = formatGameStatus(message.game_status);
                }
                
                // Update timer values
                if (message.white_time_ms !== undefined) whiteTimeMs = message.white_time_ms;
                if (message.black_time_ms !== undefined) blackTimeMs = message.black_time_ms;
                
                // Update timers display
                updateTimerDisplays();
                
                // Start timers if we're the white player (since opponent has joined)
                if (playerColor === 'white') {
                    console.log('Black player joined, starting timers');
                    startTimers();
                }
                break;
                
            case 'available_moves':
                validMoves = message.available_moves || [];
                highlightValidMoves(validMoves);
                break;
                
            case 'error':
                console.log('Error from server:', message.error);
                if (message.error) {
                    // Show error message to the user
                    const errorToast = document.createElement('div');
                    errorToast.className = 'error-toast';
                    errorToast.textContent = message.error;
                    document.body.appendChild(errorToast);
                    
                    // Remove the error toast after 3 seconds
                    setTimeout(() => {
                        errorToast.remove();
                    }, 3000);
                }
                break;
                
            default:
                console.log('Unknown message type:', message.message_type);
        }
    };

    // Format game status for display
    const formatGameStatus = (status) => {
        switch (status) {
            case 'white_turn':
                return 'White\'s turn';
            case 'black_turn':
                return 'Black\'s turn';
            case 'white_wins':
                return 'White wins!';
            case 'black_wins':
                return 'Black wins!';
            case 'draw':
                return 'Game ended in a draw';
            case 'check':
                return 'Check!';
            case 'waiting_for_opponent':
                return 'Waiting for opponent to join...';
            case 'in_progress':
                return 'Game in progress';
            default:
                return status;
        }
    };

    // Create a new game
    createGameBtn.addEventListener('click', () => {
        if (!isConnected) {
            console.error('Cannot create game: WebSocket not connected');
            return;
        }
        
        const startTimeMinutes = parseInt(startTimeSelect.value, 10);
        const incrementSeconds = parseInt(incrementSelect.value, 10);
        
        const message = {
            action: 'create',
            start_time_minutes: startTimeMinutes,
            increment_seconds: incrementSeconds
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
                
                // Add click event
                square.addEventListener('click', () => handleSquareClick(squareName));
                
                // Add drag-and-drop events
                square.addEventListener('dragover', (e) => {
                    // Allow dropping
                    e.preventDefault();
                    
                    // Highlight if it's a valid move
                    if (validMoves.includes(squareName)) {
                        square.classList.add('drag-over');
                    }
                });
                
                square.addEventListener('dragleave', () => {
                    square.classList.remove('drag-over');
                });
                
                square.addEventListener('drop', (e) => {
                    e.preventDefault();
                    square.classList.remove('drag-over');
                    
                    // Get the square that was dragged from
                    const fromSquare = e.dataTransfer.getData('text/plain');
                    
                    // Check if this is a valid move
                    if (validMoves.includes(squareName)) {
                        // Make the move
                        const message = {
                            action: 'move',
                            move_from: fromSquare,
                            move_to: squareName
                        };
                        
                        socket.send(JSON.stringify(message));
                        
                        // Reset selection
                        selectedSquare = null;
                        validMoves = [];
                        clearHighlights();
                    }
                });
                
                chessboard.appendChild(square);
            }
        }
    };

    // Update the board based on FEN string
    const updateBoard = (position, checkTurn = true) => {
        if (!position) return;
        
        // Update the board object
        board = position.board;
        
        if (checkTurn) {
            // Update turn status
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
        
        // Check if we need to flip the board
        const shouldFlipBoard = playerColor === 'black';
        
        // Apply board orientation
        if (shouldFlipBoard) {
            chessboard.classList.add('flipped');
        } else {
            chessboard.classList.remove('flipped');
        }
        
        // Place pieces on the board
        for (const [square, piece] of Object.entries(board)) {
            const squareElement = document.querySelector(`.square[data-square="${square}"]`);
            if (squareElement && chessPieces[piece]) {
                const pieceElement = document.createElement('div');
                pieceElement.className = 'piece';
                if (shouldFlipBoard) {
                    pieceElement.classList.add('flipped');
                }
                pieceElement.innerHTML = chessPieces[piece];
                pieceElement.dataset.square = square;
                pieceElement.dataset.piece = piece;
                
                // Add drag-and-drop functionality
                pieceElement.draggable = true;
                
                // Drag start event
                pieceElement.addEventListener('dragstart', (e) => {
                    // Only allow dragging if it's the player's turn and piece
                    if (!isMyTurn) {
                        e.preventDefault();
                        return false;
                    }
                    
                    const chess = new Chess();
                    const pieceColor = chess.getPieceColor(piece);
                    
                    // Compare with playerColor directly since both are now 'white' or 'black'
                    if (pieceColor !== playerColor) {
                        e.preventDefault();
                        return false;
                    }
                    
                    // Set drag data and styling
                    e.dataTransfer.setData('text/plain', square);
                    setTimeout(() => {
                        pieceElement.classList.add('dragging');
                    }, 0);
                    
                    // Select the square and get valid moves
                    selectedSquare = square;
                    highlightSquare(square);
                    
                    // Request valid moves from the server
                    const message = {
                        action: 'get_moves',
                        move_from: square
                    };
                    
                    socket.send(JSON.stringify(message));
                });
                
                // Drag end event
                pieceElement.addEventListener('dragend', () => {
                    pieceElement.classList.remove('dragging');
                });
                
                squareElement.appendChild(pieceElement);
            }
        }
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
            
            // Get piece color (now returns 'white' or 'black' to match playerColor)
            const pieceColor = chess.getPieceColor(piece);
            
            console.log('Piece color check:', { piece, pieceColor, playerColor });
            
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
                } else if (validMoves.length > 0) {
                    // If the player clicked on an invalid move target, clear the selection
                    selectedSquare = null;
                    validMoves = [];
                    clearHighlights();
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
    const highlightLastMove = (from, to) => {
        const fromSquare = document.querySelector(`.square[data-square="${from}"]`);
        const toSquare = document.querySelector(`.square[data-square="${to}"]`);
        
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

    // Format time in mm:ss format
    const formatTime = (timeMs) => {
        const totalSeconds = Math.max(0, Math.floor(timeMs / 1000));
        const minutes = Math.floor(totalSeconds / 60);
        const seconds = totalSeconds % 60;
        return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
    };

    // Update timer displays
    const updateTimerDisplays = () => {
        whiteTimeDisplay.textContent = formatTime(whiteTimeMs);
        blackTimeDisplay.textContent = formatTime(blackTimeMs);
        
        // Update active timer styling
        document.querySelector('.white-timer').classList.toggle('active-timer', activeColor === 'white');
        document.querySelector('.black-timer').classList.toggle('active-timer', activeColor === 'black');
        
        // Add low time warning (less than 30 seconds)
        document.querySelector('.white-timer').classList.toggle('low-time', whiteTimeMs < 30000);
        document.querySelector('.black-timer').classList.toggle('low-time', blackTimeMs < 30000);
    };

    // Start the timers
    const startTimers = () => {
        if (timerInterval) {
            clearInterval(timerInterval);
        }
        
        lastMoveTime = Date.now();
        console.log('Starting timers with active color:', activeColor);
        
        timerInterval = setInterval(() => {
            const now = Date.now();
            const elapsed = now - lastMoveTime;
            
            // Only update the active player's timer
            if (activeColor === 'white') {
                whiteTimeMs = Math.max(0, whiteTimeMs - 100);
                console.log('White time updated:', whiteTimeMs);
            } else {
                blackTimeMs = Math.max(0, blackTimeMs - 100);
                console.log('Black time updated:', blackTimeMs);
            }
            
            updateTimerDisplays();
            
            // Check for time out
            if (whiteTimeMs <= 0) {
                gameStatus.textContent = 'White lost on time! Black wins!';
                stopTimers();
            } else if (blackTimeMs <= 0) {
                gameStatus.textContent = 'Black lost on time! White wins!';
                stopTimers();
            }
        }, 100);
    };

    // Stop the timers
    const stopTimers = () => {
        if (timerInterval) {
            clearInterval(timerInterval);
            timerInterval = null;
        }
    };

    // Handle game created message
    const handleGameCreated = (message) => {
        // Update timer values
        if (message.white_time_ms !== undefined) whiteTimeMs = message.white_time_ms;
        if (message.black_time_ms !== undefined) blackTimeMs = message.black_time_ms;
        if (message.increment_ms !== undefined) incrementMs = message.increment_ms;
        
        // Update UI
        gameStatus.textContent = formatGameStatus(message.game_status || 'waiting_for_opponent');
        
        // Update timers display but don't start them yet
        updateTimerDisplays();
        
        // Don't start timers until opponent joins
        if (message.game_status === 'in_progress') {
            startTimers();
        } else {
            stopTimers(); // Make sure timers are stopped
        }
    };

    // Handle game joined message
    const handleGameJoined = (message) => {
        // Update timer values
        if (message.white_time_ms !== undefined) whiteTimeMs = message.white_time_ms;
        if (message.black_time_ms !== undefined) blackTimeMs = message.black_time_ms;
        if (message.increment_ms !== undefined) incrementMs = message.increment_ms;
        
        // Update UI
        gameStatus.textContent = formatGameStatus(message.game_status || 'in_progress');
        
        // Update timers
        updateTimerDisplays();
        
        // Only start timers if the game is in progress (both players present)
        if (message.game_status === 'in_progress') {
            startTimers();
        }
    };

    // Handle move made message
    const handleMoveMade = (message) => {
        // Update timer values
        if (message.white_time_ms !== undefined) whiteTimeMs = message.white_time_ms;
        if (message.black_time_ms !== undefined) blackTimeMs = message.black_time_ms;
        
        // Update timers
        updateTimerDisplays();
        
        // Highlight the last move
        if (message.last_move) {
            const { from, to } = message.last_move;
            highlightLastMove(from, to);
        }
        
        // Update game status if provided
        if (message.game_status) {
            if (message.game_status === 'checkmate') {
                const winner = activeColor === 'white' ? 'Black' : 'White';
                gameStatus.textContent = `Checkmate! ${winner} wins!`;
                stopTimers();
            } else if (message.game_status === 'stalemate') {
                gameStatus.textContent = 'Stalemate! Game is a draw.';
                stopTimers();
            } else if (message.game_status === 'draw') {
                gameStatus.textContent = 'Draw! Game is over.';
                stopTimers();
            } else if (message.game_status === 'check') {
                gameStatus.textContent = `${activeColor === 'white' ? 'White' : 'Black'} is in check!`;
            } else {
                gameStatus.textContent = 'Game in progress';
            }
        }
    };

    // Initialize the game
    initializeBoard();
    connectWebSocket();
});
