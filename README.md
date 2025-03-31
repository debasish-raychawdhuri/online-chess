# Multiplayer Chess Web Application

A real-time multiplayer chess game built with Rust (backend) and HTML/JavaScript (frontend), using WebSockets for communication.

## Features

- Create and join chess games with unique game IDs
- Real-time gameplay with WebSocket communication
- Visual highlighting of selected pieces, valid moves, and last move
- Turn-based gameplay with proper validation
- Game state updates and notifications
- Responsive design for different screen sizes

## Technology Stack

- **Backend**: Rust with Actix Web framework
  - actix-web: Web server framework
  - actix-web-actors: WebSocket support
  - chess: Chess game logic and validation
  - serde: Serialization/deserialization for JSON
  - uuid: Generating unique game IDs

- **Frontend**: HTML, CSS, JavaScript
  - Pure JavaScript (no frameworks)
  - WebSocket API for real-time communication
  - Responsive design with CSS

## Getting Started

### Prerequisites

- Rust and Cargo (https://www.rust-lang.org/tools/install)

### Running the Application

1. Clone the repository
2. Navigate to the project directory
3. Build and run the application:

```bash
cargo run
```

4. Open your browser and navigate to `http://localhost:8080`

## How to Play

1. **Create a Game**:
   - Click the "Create New Game" button
   - Share the generated Game ID with your opponent

2. **Join a Game**:
   - Enter the Game ID in the input field
   - Click the "Join Game" button

3. **Playing**:
   - Click on your piece to select it
   - Valid moves will be highlighted
   - Click on a highlighted square to move your piece
   - The game will automatically validate moves and update the board

## Project Structure

- `src/main.rs`: Main server code and WebSocket handlers
- `src/models.rs`: Data models for the application
- `static/index.html`: Main HTML page
- `static/css/style.css`: Styling for the application
- `static/js/chess.js`: Chess utility functions
- `static/js/app.js`: Frontend application logic

## License

This project is open source and available under the MIT License.
