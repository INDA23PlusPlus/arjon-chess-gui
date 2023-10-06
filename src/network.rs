use crate::network::Handshake::{ClientToServer, ServerToClient};
use crate::{parse_move, BoardRepr, Move, Square};
use chess_network_protocol;
use chess_network_protocol::{ClientToServerHandshake, ServerToClientHandshake};
use jonathan_hallstrom_chess::PieceType;
use serde_json;
use std::net::{TcpListener, TcpStream};

pub(crate) struct Network {
    pub(crate) stream: TcpStream,
    pub(crate) is_server: bool,
    pub(crate) player_color: jonathan_hallstrom_chess::Color,
}

pub(crate) enum Handshake {
    ServerToClient(ServerToClientHandshake),
    ClientToServer(ClientToServerHandshake),
}

pub(crate) fn connect(as_server: bool, ip: &str) -> TcpStream {
    let stream;
    if as_server {
        println!("Listening to clients on IP: {}.", ip);
        let listener = TcpListener::bind(ip).unwrap();
        stream = listener.accept().unwrap().0;
    } else {
        println!("Connecting to IP: {}", ip);
        stream = TcpStream::connect(ip).unwrap();
    }

    println!("Connection established");
    stream
}

pub(crate) fn handshake(stream: TcpStream, handshake: Handshake) -> Network {
    let mut is_server;
    let mut player_color;
    match handshake {
        Handshake::ServerToClient(server_to_client_handshake) => {
            is_server = true;

            let received: ClientToServerHandshake = serde_json::from_reader(&stream).unwrap();
            println!("Handshake from client: {:?}", received);

            // This is the color the client wants us to play as
            player_color = match received.server_color {
                chess_network_protocol::Color::White => jonathan_hallstrom_chess::Color::White,
                chess_network_protocol::Color::Black => jonathan_hallstrom_chess::Color::Black,
            };

            serde_json::to_writer(&stream, &server_to_client_handshake).unwrap();
        }
        Handshake::ClientToServer(client_to_server_handshake) => {
            is_server = false;

            // client_to_server_handshake contains the color the server will play as,
            // so we will play as the opposite color
            player_color = match client_to_server_handshake.server_color {
                chess_network_protocol::Color::White => jonathan_hallstrom_chess::Color::Black,
                chess_network_protocol::Color::Black => jonathan_hallstrom_chess::Color::White,
            };

            serde_json::to_writer(&stream, &client_to_server_handshake).unwrap();

            let received: ServerToClientHandshake = serde_json::from_reader(&stream).unwrap();
            println!("Handshake from server: {:?}", received);
        }
    }
    stream.set_nonblocking(true).unwrap();
    Network {
        stream,
        is_server,
        player_color,
    }
}

pub(crate) fn internal_to_network_piece(internal: &Square) -> chess_network_protocol::Piece {
    match internal {
        Square::Empty => chess_network_protocol::Piece::None,
        Square::Pawn(color) => match color {
            jonathan_hallstrom_chess::Color::White => chess_network_protocol::Piece::WhitePawn,
            jonathan_hallstrom_chess::Color::Black => chess_network_protocol::Piece::BlackPawn,
        },
        Square::Rook(color) => match color {
            jonathan_hallstrom_chess::Color::White => chess_network_protocol::Piece::WhiteRook,
            jonathan_hallstrom_chess::Color::Black => chess_network_protocol::Piece::BlackRook,
        },
        Square::Bishop(color) => match color {
            jonathan_hallstrom_chess::Color::White => chess_network_protocol::Piece::WhiteBishop,
            jonathan_hallstrom_chess::Color::Black => chess_network_protocol::Piece::BlackBishop,
        },
        Square::Knight(color) => match color {
            jonathan_hallstrom_chess::Color::White => chess_network_protocol::Piece::WhiteKnight,
            jonathan_hallstrom_chess::Color::Black => chess_network_protocol::Piece::BlackKnight,
        },
        Square::King(color) => match color {
            jonathan_hallstrom_chess::Color::White => chess_network_protocol::Piece::WhiteKing,
            jonathan_hallstrom_chess::Color::Black => chess_network_protocol::Piece::BlackKing,
        },
        Square::Queen(color) => match color {
            jonathan_hallstrom_chess::Color::White => chess_network_protocol::Piece::WhiteQueen,
            jonathan_hallstrom_chess::Color::Black => chess_network_protocol::Piece::BlackQueen,
        },
    }
}

pub(crate) fn internal_to_network_board(
    internal: &[[Square; 8]; 8],
) -> [[chess_network_protocol::Piece; 8]; 8] {
    let mut board = [[chess_network_protocol::Piece::None; 8]; 8];

    for row in 0..8usize {
        for col in 0..8usize {
            let piece = &internal[row][col];
            // Network representation has the rows flipped
            board[7 - row][col] = internal_to_network_piece(piece);
        }
    }

    board
}

pub(crate) fn internal_to_network_move(internal: &Move) -> chess_network_protocol::Move {
    let ((start_x, mut start_y), (end_x, mut end_y)) =
        parse_move(&internal.to_algebraic_notation());

    // Flip the row
    start_y = 7 - start_y;
    end_y = 7 - end_y;

    let mut promotion = chess_network_protocol::Piece::None;

    if internal.to_algebraic_notation().len() == 5 {
        promotion = match internal.to_algebraic_notation().chars().last().unwrap() {
            'R' => chess_network_protocol::Piece::WhiteRook,
            'r' => chess_network_protocol::Piece::BlackRook,
            'B' => chess_network_protocol::Piece::WhiteBishop,
            'b' => chess_network_protocol::Piece::BlackBishop,
            'N' => chess_network_protocol::Piece::WhiteKnight,
            'n' => chess_network_protocol::Piece::BlackKnight,
            'Q' => chess_network_protocol::Piece::WhiteQueen,
            'q' => chess_network_protocol::Piece::BlackQueen,
            _ => panic!("Invalid promotion piece"),
        }
    }

    chess_network_protocol::Move {
        start_x,
        start_y,
        end_x,
        end_y,
        promotion,
    }
}

pub(crate) fn internal_to_network_moves(internal: &Vec<Move>) -> Vec<chess_network_protocol::Move> {
    let mut moves = Vec::new();
    for mv in internal {
        moves.push(internal_to_network_move(mv));
    }
    moves
}

pub(crate) fn internal_to_server_handshake(
    board_repr: &BoardRepr,
    board: &jonathan_hallstrom_chess::Board,
) -> ServerToClientHandshake {
    ServerToClientHandshake {
        board: internal_to_network_board(&board_repr.squares),
        features: vec![
            chess_network_protocol::Features::EnPassant,
            chess_network_protocol::Features::Promotion,
        ],
        joever: chess_network_protocol::Joever::White,
        moves: internal_to_network_moves(&board.get_legal_moves()),
    }
}

impl Network {
    pub(crate) fn send_board_state(
        &self,
        repr: &BoardRepr,
        board: &jonathan_hallstrom_chess::Board,
        server_move: &jonathan_hallstrom_chess::Move,
    ) {
        let state = chess_network_protocol::ServerToClient::State {
            board: internal_to_network_board(&repr.squares),
            moves: internal_to_network_moves(&board.get_legal_moves()),
            joever: chess_network_protocol::Joever::White,
            move_made: internal_to_network_move(server_move),
        };
        serde_json::to_writer(&self.stream, &state).unwrap();
    }

    pub(crate) fn send_move(&self, client_move: &Move) {
        let mv = chess_network_protocol::ClientToServer::Move {
            0: internal_to_network_move(client_move),
        };
        serde_json::to_writer(&self.stream, &mv).unwrap();
    }

    pub(crate) fn get_board_state(&self) -> Option<chess_network_protocol::ServerToClient> {
        if let Ok(state) = serde_json::from_reader(&self.stream) {
            return Some(state);
        }
        None
    }
}
