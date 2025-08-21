use std::io;
use std::net::{TcpListener, TcpStream};
use std::io::{BufRead, BufReader, Write};
use std::thread;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

const RESET: &str = "\x1b[0m";
const ORANGE: &str = "\x1b[93m";
const RED: &str = "\x1b[0;31m";

const BOARD_WIDTH: usize = 7;
const BOARD_HEIGHT: usize = 6;

type Board = [[u8; BOARD_WIDTH]; BOARD_HEIGHT];

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
enum Player {
    One = 1,
    Two = 2,
    None = 0,
}

impl Player {
    fn from_int(int: u8) -> Player {
        match int {
            1 => Player::One,
            2 => Player::Two,
            _ => Player::None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum MoveError {
    GameFinished,
    InvalidColumn,
    ColumnFull,
}

impl std::fmt::Display for MoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveError::ColumnFull => write!(f, "column is full"),
            MoveError::InvalidColumn => write!(f, "column must be between 1 and 7"),
            MoveError::GameFinished => write!(f, "game is already finished"),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct Game {
    current_move: u8,
    current_player: Player,
    board: Board,
    is_finished: bool,
    winner: Player,
}

impl Game {
    fn default() -> Game {
        Game {
            current_move: 0,
            current_player: Player::One,
            board: [
                [0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0],
            ],
            is_finished: false,
            winner: Player::None,
        }
    }

    fn clear_screen(&self) {
        print!("{}[2J", 27 as char);
    }

    fn display_board(&self) {
        self.clear_screen();

        println!("{}--------------------{}", ORANGE, RESET);
        println!("{}CONNECT 4 (Move {}){}", ORANGE, self.current_move, RESET);
        println!("{}--------------------{}", ORANGE, RESET);

        for row in self.board {
            let row_str: String = row
                .iter()
                .map(|&cell| match cell {
                    1 => "🔴",
                    2 => "🟡",
                    _ => "⚫",
                })
                .collect::<Vec<&str>>()
                .join(" ");

            println!("{}", row_str);
        }

        println!("{}--------------------{}", ORANGE, RESET);

        if self.is_finished {
            match self.winner {
                Player::One => println!("{}🔴 Player 1 has won!{}", ORANGE, RESET),
                Player::Two => println!("{}🟡 Player 2 has won!{}", ORANGE, RESET),
                Player::None => println!("{}It's a draw!{}", ORANGE, RESET),
            }

            println!("{}--------------------{}", ORANGE, RESET);
        }
    }

    fn display_error(&self, error: String) {
        self.display_board();
        println!("{}Error: {}{}", RED, error, RESET);
    }

    fn calculate_winner(&mut self) -> Player {
        if self.current_move < BOARD_WIDTH as u8 {
            return Player::None;
        }

        for row in 0..BOARD_HEIGHT {
            for col in 0..BOARD_WIDTH {
                let cell = self.board[row][col];

                if cell != 0 {
                    let directions = [
                        (0, 1),  // horizontal
                        (1, 0),  // vertical
                        (1, 1),  // diagonal (top-left to bottom-right)
                        (-1, 1), // diagonal (bottom-left to top-right)
                    ];

                    for (row_step, col_step) in directions {
                        let mut consecutive_count = 1;
                        let mut r = row as isize + row_step;
                        let mut c = col as isize + col_step;

                        while r >= 0
                            && r < BOARD_HEIGHT as isize
                            && c >= 0
                            && c < BOARD_WIDTH as isize
                        {
                            if self.board[r as usize][c as usize] == cell {
                                consecutive_count += 1;

                                if consecutive_count == 4 {
                                    self.is_finished = true;
                                    return Player::from_int(cell);
                                }
                            } else {
                                break;
                            }
                            r += row_step;
                            c += col_step;
                        }
                    }
                }
            }
        }

        if self.current_move >= BOARD_HEIGHT as u8 * BOARD_WIDTH as u8 {
            self.is_finished = true;
        }

        Player::None
    }

    fn play_move(&mut self, column: usize) -> Result<(), MoveError> {
        if self.is_finished {
            return Err(MoveError::GameFinished);
        }

        if column >= BOARD_WIDTH {
            return Err(MoveError::InvalidColumn);
        }

        if let Some(row) = (0..BOARD_HEIGHT)
            .rev()
            .find(|&row| self.board[row][column] == 0)
        {
            self.board[row][column] = self.current_player as u8;
            self.current_move += 1;
        } else {
            return Err(MoveError::ColumnFull);
        }

        let calculated_winner = self.calculate_winner();

        if calculated_winner != Player::None {
            self.winner = calculated_winner;
        } else {
            self.current_player = match self.current_player {
                Player::One => Player::Two,
                _ => Player::One,
            };
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
enum GameMessage {
    Move(usize),
    GameState(Game),
    PlayerAssignment(Player),
    Error(String),
    Quit,
}

#[derive(Clone)]
enum GameMode {
    Local,
    NetworkHost,
    NetworkClient(String),
}

fn main() {
    println!("🔴🟡 CONNECT 4 - Multiplayer Edition 🟡🔴");
    println!("Choose game mode:");
    println!("1. Local Multiplayer (same device)");
    println!("2. Host Network Game");
    println!("3. Join Network Game");
    println!("Enter your choice (1-3):");

    let mut choice = String::new();
    io::stdin().read_line(&mut choice).expect("Failed to read line");

    let game_mode = match choice.trim() {
        "1" => GameMode::Local,
        "2" => {
            println!("Enter port to host on (default: 8080):");
            let mut port = String::new();
            io::stdin().read_line(&mut port).expect("Failed to read line");
            let port = port.trim().parse().unwrap_or(8080);
            println!("Hosting game on port {}...", port);
            GameMode::NetworkHost
        },
        "3" => {
            println!("Enter server address (e.g., 127.0.0.1:8080):");
            let mut addr = String::new();
            io::stdin().read_line(&mut addr).expect("Failed to read line");
            GameMode::NetworkClient(addr.trim().to_string())
        },
        _ => {
            println!("Invalid choice, defaulting to local multiplayer.");
            GameMode::Local
        }
    };

    match game_mode {
        GameMode::Local => run_local_game(),
        GameMode::NetworkHost => run_host_game(),
        GameMode::NetworkClient(addr) => run_client_game(addr),
    }
}

fn run_local_game() {
    let mut game = Game::default();
    game.display_board();

    loop {
        while !game.is_finished {
            println!("\n");

            match game.current_player {
                Player::One => println!("PLAYER 1 (🔴)"),
                Player::Two => println!("PLAYER 2 (🟡)"),
                _ => (),
            };

            println!("Enter a column between 1 and 7:");

            let mut user_move = String::new();
            io::stdin()
                .read_line(&mut user_move)
                .expect("Failed to read line");

            let user_move: usize = match user_move.trim().parse() {
                Ok(num) => {
                    if num < 1 || num > 7 {
                        game.display_error(MoveError::InvalidColumn.to_string());
                        continue;
                    } else {
                        num
                    }
                }
                Err(err) => {
                    game.display_error(err.to_string());
                    continue;
                }
            };

            match game.play_move(user_move - 1) {
                Ok(_) => {
                    game.display_board();
                }
                Err(err) => {
                    game.display_error(err.to_string());
                }
            }
        }

        println!("Press 'R' to restart or 'Q' to quit the game.");

        let mut user_input = String::new();
        io::stdin()
            .read_line(&mut user_input)
            .expect("Failed to read line");

        match user_input.trim() {
            "R" | "r" => {
                game = Game::default();
                game.display_board();
            }
            "Q" | "q" => {
                println!("Quitting...");
                break;
            }
            _ => game.display_error("invalid input".to_string()),
        }
    }
}

fn run_host_game() {
    let listener = TcpListener::bind("127.0.0.1:8080").expect("Failed to bind to port");
    println!("Waiting for a player to connect...");
    
    let (stream, addr) = listener.accept().expect("Failed to accept connection");
    println!("Player connected from {}", addr);
    
    let game = Arc::new(Mutex::new(Game::default()));
    let stream = Arc::new(Mutex::new(stream));
    
    // Send player assignment
    {
        let mut stream = stream.lock().unwrap();
        let msg = GameMessage::PlayerAssignment(Player::Two);
        let json = serde_json::to_string(&msg).unwrap();
        writeln!(stream, "{}", json).ok();
        stream.flush().ok();
    }
    
    println!("You are Player 1 (🔴), opponent is Player 2 (🟡)");
    
    let game_clone = Arc::clone(&game);
    let stream_clone = Arc::clone(&stream);
    
    // Spawn thread to handle incoming messages
    thread::spawn(move || {
        let reader = BufReader::new(stream_clone.lock().unwrap().try_clone().unwrap());
        for line in reader.lines() {
            if let Ok(line) = line {
                if let Ok(msg) = serde_json::from_str::<GameMessage>(&line) {
                    match msg {
                        GameMessage::Move(col) => {
                            let mut game = game_clone.lock().unwrap();
                            if game.current_player == Player::Two {
                                match game.play_move(col) {
                                    Ok(_) => {
                                        game.display_board();
                                    }
                                    Err(err) => {
                                        game.display_error(err.to_string());
                                    }
                                }
                            }
                        }
                        GameMessage::Quit => {
                            println!("Opponent quit the game.");
                            return;
                        }
                        _ => {}
                    }
                }
            }
        }
    });
    
    // Main game loop for host
    {
        let game = game.lock().unwrap();
        game.display_board();
    }
    
    loop {
        let game_finished = {
            let game = game.lock().unwrap();
            game.is_finished
        };
        
        if game_finished {
            break;
        }
        
        let current_player = {
            let game = game.lock().unwrap();
            game.current_player
        };
        
        if current_player == Player::One {
            println!("\nYour turn (🔴):");
            println!("Enter a column between 1 and 7:");
            
            let mut user_move = String::new();
            io::stdin().read_line(&mut user_move).expect("Failed to read line");
            
            let user_move: usize = match user_move.trim().parse() {
                Ok(num) => {
                    if num < 1 || num > 7 {
                        println!("Column must be between 1 and 7");
                        continue;
                    } else {
                        num
                    }
                }
                Err(_) => {
                    println!("Invalid input");
                    continue;
                }
            };
            
            {
                let mut game = game.lock().unwrap();
                match game.play_move(user_move - 1) {
                    Ok(_) => {
                        game.display_board();
                        // Send game state to client
                        let mut stream = stream.lock().unwrap();
                        let msg = GameMessage::GameState((*game).clone());
                        let json = serde_json::to_string(&msg).unwrap();
                        writeln!(stream, "{}", json).ok();
                        stream.flush().ok();
                    }
                    Err(err) => {
                        println!("Error: {}", err);
                    }
                }
            }
        } else {
            println!("Waiting for opponent's move...");
            thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    
    println!("Game ended. Press Enter to exit.");
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
}

fn run_client_game(addr: String) {
    let stream = match TcpStream::connect(&addr) {
        Ok(stream) => stream,
        Err(_) => {
            println!("Failed to connect to server at {}", addr);
            return;
        }
    };
    
    println!("Connected to server!");
    
    let stream = Arc::new(Mutex::new(stream));
    let game = Arc::new(Mutex::new(Game::default()));
    let my_player = Arc::new(Mutex::new(Player::None));
    
    let stream_clone = Arc::clone(&stream);
    let game_clone = Arc::clone(&game);
    let player_clone = Arc::clone(&my_player);
    
    // Spawn thread to handle incoming messages
    thread::spawn(move || {
        let reader = BufReader::new(stream_clone.lock().unwrap().try_clone().unwrap());
        for line in reader.lines() {
            if let Ok(line) = line {
                if let Ok(msg) = serde_json::from_str::<GameMessage>(&line) {
                    match msg {
                        GameMessage::PlayerAssignment(player) => {
                            *player_clone.lock().unwrap() = player;
                            let player_info = match player {
                                Player::One => "1 🔴",
                                Player::Two => "2 🟡",
                                _ => "Unknown"
                            };
                            println!("You are Player {}", player_info);
                        }
                        GameMessage::GameState(new_game) => {
                            *game_clone.lock().unwrap() = new_game;
                            game_clone.lock().unwrap().display_board();
                        }
                        GameMessage::Error(err) => {
                            println!("Server error: {}", err);
                        }
                        _ => {}
                    }
                }
            }
        }
    });
    
    // Wait for player assignment
    while *my_player.lock().unwrap() == Player::None {
        thread::sleep(std::time::Duration::from_millis(100));
    }
    
    // Main game loop for client
    loop {
        let (current_player, game_finished) = {
            let game = game.lock().unwrap();
            (game.current_player, game.is_finished)
        };
        
        if game_finished {
            break;
        }
        
        let my_turn = {
            let my_player = my_player.lock().unwrap();
            current_player == *my_player
        };
        
        if my_turn {
            let player_symbol = match *my_player.lock().unwrap() {
                Player::One => "🔴",
                Player::Two => "🟡",
                _ => "?"
            };
            
            println!("\nYour turn ({}):", player_symbol);
            println!("Enter a column between 1 and 7:");
            
            let mut user_move = String::new();
            io::stdin().read_line(&mut user_move).expect("Failed to read line");
            
            let user_move: usize = match user_move.trim().parse() {
                Ok(num) => {
                    if num < 1 || num > 7 {
                        println!("Column must be between 1 and 7");
                        continue;
                    } else {
                        num
                    }
                }
                Err(_) => {
                    println!("Invalid input");
                    continue;
                }
            };
            
            // Send move to server
            {
                let mut stream = stream.lock().unwrap();
                let msg = GameMessage::Move(user_move - 1);
                let json = serde_json::to_string(&msg).unwrap();
                writeln!(stream, "{}", json).ok();
                stream.flush().ok();
            }
        } else {
            println!("Waiting for opponent's move...");
            thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    
    println!("Game ended. Press Enter to exit.");
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
}
