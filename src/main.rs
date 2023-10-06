mod network;

use crate::network::Handshake::ClientToServer;
use crate::network::{
    internal_to_network_board, internal_to_network_move, internal_to_network_moves,
    internal_to_server_handshake, Network,
};
use chess_network_protocol;
use chess_network_protocol::ServerToClient;
use ggez::conf::{FullscreenType, NumSamples, WindowMode, WindowSetup};
use ggez::event::EventHandler;
use ggez::graphics::{Canvas, DrawMode, Image, Mesh, Rect, Transform};
use ggez::winit::dpi::LogicalSize;
use ggez::winit::event::VirtualKeyCode::B;
use ggez::{event, graphics, Context, GameResult};
use jonathan_hallstrom_chess::{Board, Color, Move};
use mint::{Point2, Vector2};
use std::cmp::min;
use std::collections::HashMap;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::os::macos::raw::stat;

#[macro_use]
extern crate lazy_static;

static PIECES_IMAGE_BYTES: &'static [u8] = include_bytes!("Pieces.png");

lazy_static! {
    static ref CHESSBOARD_MESH: graphics::MeshBuilder = {
        let mut mesh = graphics::MeshBuilder::new();
        for row in 0..8usize {
            for col in 0..8usize {
                mesh.rectangle(
                    DrawMode::fill(),
                    graphics::Rect::new(col as f32 / 8.0, row as f32 / 8.0, 1.0 / 8.0, 1.0 / 8.0),
                    match (row + col) % 2 == 0 {
                        true => WHITE_SQUARE_COLOR,
                        false => BLACK_SQUARE_COLOR,
                    },
                )
                .unwrap();
            }
        }
        mesh
    };
}

const COL_COUNT_F32: f32 = 8.0;
const ROW_COUNT_F32: f32 = 8.0;
const HIGHLIGHT_COLOR: graphics::Color = graphics::Color::new(0.0, 0.5, 0.0, 0.75);
const DARK_FILM_COLOR: graphics::Color = graphics::Color::new(0.0, 0.0, 0.0, 0.75);
const BLACK_SQUARE_COLOR: graphics::Color = graphics::Color::new(0.9, 0.7, 0.7, 1.0);
const WHITE_SQUARE_COLOR: graphics::Color = graphics::Color::new(1.0, 0.9, 0.9, 1.0);

#[derive(Eq, PartialEq, Copy, Clone, Hash)]
enum Square {
    Empty,
    Pawn(Color),
    Rook(Color),
    Bishop(Color),
    Knight(Color),
    King(Color),
    Queen(Color),
}

impl Square {
    #[inline]
    fn color(&self) -> Option<Color> {
        match self {
            Square::Empty => None,
            Square::Pawn(color)
            | Square::Rook(color)
            | Square::Bishop(color)
            | Square::Knight(color)
            | Square::King(color)
            | Square::Queen(color) => Some(*color),
        }
    }
}

fn parse_fen(fen: &str) -> [[Square; 8]; 8] {
    let mut board = [[Square::Empty; 8]; 8];
    let mut iter = fen.chars();

    let mut row = 0usize;
    while row < 8 {
        let mut col = 0usize;
        while col < 8 {
            let c = iter.next().unwrap();

            assert_ne!(c, '/');
            assert!(c.is_alphanumeric());

            if c.is_numeric() {
                col += c.to_digit(10).unwrap() as usize;
            } else {
                let color = match c.is_uppercase() {
                    true => Color::White,
                    false => Color::Black,
                };
                board[row][col] = match c.to_lowercase().last().unwrap() {
                    'p' => Square::Pawn(color),
                    'b' => Square::Bishop(color),
                    'r' => Square::Rook(color),
                    'n' => Square::Knight(color),
                    'q' => Square::Queen(color),
                    _ => Square::King(color),
                };
                col += 1;
            }
        }
        {
            let c = iter.next().unwrap();
            assert!((c == '/' && row < 7) || (c == ' ' && row == 7));
        }
        row += 1;
    }
    board
}

#[inline]
fn to_cordinate(c: char) -> usize {
    if c >= 'a' && c <= 'h' {
        return c as usize - 'a' as usize;
    }
    7 - (c as usize - '1' as usize)
}

#[inline]
fn parse_move(mv: &str) -> ((usize, usize), (usize, usize)) {
    let x: Vec<char> = mv.chars().collect();
    (
        (to_cordinate(x[1]), to_cordinate(x[0])),
        (to_cordinate(x[3]), to_cordinate(x[2])),
    )
}

fn parse_moves(moves: Vec<Move>) -> [[HashMap<(usize, usize), Vec<Move>>; 8]; 8] {
    let mut parsed: [[HashMap<(usize, usize), Vec<Move>>; 8]; 8] = Default::default();

    for mv in moves {
        let (from, to) = parse_move(&mv.to_algebraic_notation());
        parsed[from.0][from.1]
            .entry(to)
            .or_insert_with(Vec::new)
            .push(mv);
    }
    parsed
}

struct Render {
    pieces_image: Image,
    chessboard_mesh: Mesh,
    promotion_mesh: Mesh,
    selected_piece_mesh: Mesh,
    available_move_mesh: Mesh,
}

impl Render {
    fn new(ctx: &Context) -> Self {
        Self {
            pieces_image: Image::from_bytes(ctx, PIECES_IMAGE_BYTES).unwrap(),
            chessboard_mesh: Mesh::from_data(ctx, CHESSBOARD_MESH.build().clone()),
            promotion_mesh: Mesh::new_rectangle(
                ctx,
                DrawMode::fill(),
                Rect::one(),
                DARK_FILM_COLOR,
            )
            .unwrap(),
            selected_piece_mesh: Mesh::new_rectangle(
                ctx,
                DrawMode::fill(),
                {
                    let mut rect = Rect::one();
                    rect.scale(1.0 / COL_COUNT_F32, 1.0 / ROW_COUNT_F32);
                    rect
                },
                HIGHLIGHT_COLOR,
            )
            .unwrap(),
            available_move_mesh: Mesh::new_circle(
                ctx,
                DrawMode::fill(),
                Point2 {
                    x: 0.5 / COL_COUNT_F32,
                    y: 0.5 / ROW_COUNT_F32,
                },
                0.25 / COL_COUNT_F32,
                0.25 / (COL_COUNT_F32 * 1024.0),
                HIGHLIGHT_COLOR,
            )
            .unwrap(),
        }
    }
}

pub(crate) struct BoardRepr {
    // Rendering aid
    squares: [[Square; 8]; 8],
    legal_moves: [[HashMap<(usize, usize), Vec<Move>>; 8]; 8],
    selected_from: Option<(usize, usize)>,
    selected_to: Option<(usize, usize)>,
}

impl BoardRepr {
    fn new(board: &Board) -> Self {
        Self {
            squares: parse_fen(&board.to_fen()),
            legal_moves: parse_moves(board.get_legal_moves()),
            selected_from: None,
            selected_to: None,
        }
    }
}

struct Game {
    // Game logic
    board: Board,

    // Board representation
    board_repr: BoardRepr,

    // Rendering stuff
    render: Render,

    // Networking
    network: Network,
}

impl Game {
    #[inline]
    fn refresh_board(&mut self) {
        self.board_repr.legal_moves = parse_moves(self.board.get_legal_moves());
        self.board_repr.squares = parse_fen(&self.board.to_fen());
        self.board_repr.selected_from = None;
        self.board_repr.selected_to = None;
    }
    fn new(
        ctx: &Context,
        stream: TcpStream,
        is_server: bool,
        server_color: Option<chess_network_protocol::Color>,
    ) -> Self {
        let board = Board::default();
        let board_repr = BoardRepr::new(&board);
        let network = network::handshake(
            stream,
            match is_server {
                true => network::Handshake::ServerToClient(internal_to_server_handshake(
                    &board_repr,
                    &board,
                )),
                false => network::Handshake::ClientToServer(
                    chess_network_protocol::ClientToServerHandshake {
                        server_color: server_color.expect("Client has to choose server color."),
                    },
                ),
            },
        );
        Self {
            board,
            board_repr,
            render: Render::new(ctx),
            network,
        }
    }
    #[inline]
    fn draw_squares(&self, canvas: &mut Canvas) {
        let (width, height) = {
            let cords = canvas.screen_coordinates().unwrap();
            (cords.w, cords.h)
        };

        canvas.draw(
            &self.render.chessboard_mesh,
            graphics::DrawParam::default().scale(Vector2 {
                x: width,
                y: height,
            }),
        );
    }

    fn draw_piece(&self, canvas: &mut Canvas, piece: &Square, row: usize, col: usize) {
        if *piece == Square::Empty {
            return;
        }
        let color = piece.color().unwrap();

        let rect = Rect::new(
            match piece {
                Square::Pawn(_) => 5.0,
                Square::Rook(_) => 4.0,
                Square::Knight(_) => 3.0,
                Square::Bishop(_) => 2.0,
                Square::Queen(_) => 1.0,
                _ => 0.0,
            } / 6.0,
            match color {
                Color::White => 0.0,
                Color::Black => 1.0,
            } / 2.0,
            1.0 / 6.0,
            1.0 / 2.0,
        );

        let (width, height) = {
            let cords = canvas.screen_coordinates().unwrap();
            (cords.w, cords.h)
        };

        canvas.draw(
            &self.render.pieces_image,
            graphics::DrawParam {
                src: rect,
                color: graphics::Color::WHITE,
                transform: Transform::Values {
                    dest: Point2 {
                        x: col as f32 * width / 8.0,
                        y: row as f32 * height / 8.0,
                    },
                    rotation: 0.0,
                    scale: Vector2 {
                        x: (width * 6.0) / (self.render.pieces_image.width() as f32 * 8.0),
                        y: (height * 2.0) / (self.render.pieces_image.height() as f32 * 8.0),
                    },
                    offset: Point2 { x: 0.0, y: 0.0 },
                },
                z: 0,
            },
        );
    }

    #[inline]
    fn draw_pieces(&self, canvas: &mut Canvas) {
        for row in 0..8usize {
            for col in 0..8usize {
                // Draw piece on current square
                self.draw_piece(canvas, &self.board_repr.squares[row][col], row, col);
            }
        }
    }
    #[inline]
    fn draw_promotion_selection(&self, canvas: &mut Canvas, row: usize, col: usize) {
        let (width, height) = {
            let cords = canvas.screen_coordinates().unwrap();
            (cords.w, cords.h)
        };

        // Grey out the chessboard
        canvas.draw(
            &self.render.promotion_mesh,
            graphics::DrawParam::default().scale(Vector2 {
                x: width,
                y: height,
            }),
        );
        let dir = match row {
            7 => -1isize,
            0 => 1isize,
            _ => panic!("Promotion not on last row"),
        };

        let (from_row, from_col) = self.board_repr.selected_from.unwrap();

        let color = self.board_repr.squares[from_row][from_col].color().unwrap();
        let pieces: [Square; 4] = [
            Square::Queen(color),
            Square::Knight(color),
            Square::Rook(color),
            Square::Bishop(color),
        ];

        for dy in 0..4usize {
            let row = (row as isize + dir * dy as isize) as usize;
            self.draw_piece(canvas, &pieces[dy], row, col);
        }
    }

    #[inline]
    fn draw_move_selection(&self, canvas: &mut Canvas, row: usize, col: usize) {
        let (width, height) = {
            let cords = canvas.screen_coordinates().unwrap();
            (cords.w, cords.h)
        };

        canvas.draw(
            &self.render.selected_piece_mesh,
            graphics::DrawParam::default().dest_rect(Rect {
                x: (col as f32) * width / 8.0,
                y: (row as f32) * height / 8.0,
                w: width,
                h: height,
            }),
        );

        for ((row, col), _) in &self.board_repr.legal_moves[row][col] {
            canvas.draw(
                &self.render.available_move_mesh,
                graphics::DrawParam::default().dest_rect(Rect {
                    x: (*col as f32) * width / 8.0,
                    y: (*row as f32) * height / 8.0,
                    w: width,
                    h: height,
                }),
            );
        }
    }
    fn server_play_move(&mut self, opponent_move: &chess_network_protocol::Move) {
        let legal_moves = self.board.get_legal_moves();
        for mv in legal_moves {
            if network::internal_to_network_move(&mv) == *opponent_move {
                self.board.play_move(mv).unwrap();
                self.refresh_board();
                return;
            }
        }
        panic!("No internal move matching opponent move found!!!");
    }

    fn play_move(&mut self, player_move: &Move) {
        self.board.play_move(*player_move).unwrap();
        self.refresh_board();
        if self.network.is_server {
            let message = chess_network_protocol::ServerToClient::State {
                board: internal_to_network_board(&self.board_repr.squares),
                moves: internal_to_network_moves(&self.board.get_legal_moves()),
                joever: chess_network_protocol::Joever::Ongoing,
                move_made: internal_to_network_move(player_move),
            };
            serde_json::to_writer(&self.network.stream, &message).unwrap();
        } else {
            let message =
                chess_network_protocol::ClientToServer::Move(internal_to_network_move(player_move));
            // We will suggest our move to the server and the server will respond with a new board state
            serde_json::to_writer(&self.network.stream, &message).unwrap();
        }
    }
}

impl event::EventHandler for Game {
    #[inline]
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
        if let Some(state) = self.network.get_board_state() {
            match state {
                ServerToClient::State { move_made, .. } => {
                    self.server_play_move(&move_made);
                    self.refresh_board();
                }
                ServerToClient::Error { .. } => {}
                ServerToClient::Resigned { .. } => {}
                ServerToClient::Draw { .. } => {}
            }
        }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        // Start with a white canvas the size of the program window
        let mut canvas = Canvas::from_frame(ctx, graphics::Color::WHITE);

        // Draw chessboard pattern
        self.draw_squares(&mut canvas);

        // Draw pieces
        self.draw_pieces(&mut canvas);

        // Draw selection for promotion if promoting move is selected
        if let Some((row, col)) = self.board_repr.selected_to {
            self.draw_promotion_selection(&mut canvas, row, col);
        }
        // Else draw available moves if piece is selected
        else if let Some((row, col)) = self.board_repr.selected_from {
            self.draw_move_selection(&mut canvas, row, col);
        }

        // Submit drawing
        canvas.finish(ctx)
    }

    fn mouse_button_down_event(
        &mut self,
        ctx: &mut Context,
        _button: event::MouseButton,
        x: f32,
        y: f32,
    ) -> GameResult {
        let (width, height) = ctx.gfx.drawable_size();
        // Coerce in the range 0..=7 in case mouse pointer registers outside normal range
        let row = min((y * ROW_COUNT_F32 / height).abs() as usize, 7usize);
        let col = min((x * COL_COUNT_F32 / width).abs() as usize, 7usize);

        let (prev_row, prev_col) = self.board_repr.selected_from.unwrap_or((0usize, 0usize));

        let cords = Some((row, col));

        if self.board_repr.selected_to.is_some() {
            if col == self.board_repr.selected_to.as_ref().unwrap().1
                && ((self.board_repr.selected_to.as_ref().unwrap().0 == 0 && row < 4)
                    || (self.board_repr.selected_to.as_ref().unwrap().0 == 7 && row >= 4))
            {
                let drow = (self.board_repr.selected_to.as_ref().unwrap().0 as isize - row as isize)
                    .abs() as usize;
                let promotion_piece = match drow {
                    0 => jonathan_hallstrom_chess::PieceType::Queen,
                    1 => jonathan_hallstrom_chess::PieceType::Knight,
                    2 => jonathan_hallstrom_chess::PieceType::Rook,
                    3 => jonathan_hallstrom_chess::PieceType::Bishop,
                    _ => panic!("Couldn't select a promotion piece."),
                };
                let mut move_to_be_made = None;
                for mv in &self.board_repr.legal_moves[prev_row][prev_col]
                    [self.board_repr.selected_to.as_ref().unwrap()]
                {
                    if mv.get_promoted_type().unwrap() == promotion_piece {
                        move_to_be_made = Some(mv);
                        break;
                    }
                }
                assert!(move_to_be_made.is_some());
                self.play_move(move_to_be_made.unwrap());
                self.refresh_board();
            } else {
                self.board_repr.selected_to = None;
                self.board_repr.selected_from = None;
            }
        } else if self.board_repr.selected_from.is_some()
            && self.board_repr.legal_moves[prev_row][prev_col].contains_key(cords.as_ref().unwrap())
        {
            let moves = &self.board_repr.legal_moves[prev_row][prev_col][cords.as_ref().unwrap()];
            if moves.len() > 1 {
                self.board_repr.selected_to = cords.clone();
            } else {
                self.board.play_move(moves[0]).unwrap();
                self.refresh_board();
            }
        } else if self.board_repr.squares[row][col]
            .color()
            .map_or(false, |color| color == self.board.get_curr_player())
            && self.board_repr.selected_from != cords
        {
            self.board_repr.selected_from = cords;
        } else {
            self.board_repr.selected_from = None;
        }

        Ok(())
    }
}

fn main() -> GameResult {
    let ws = WindowSetup {
        title: "Arvid Jonassons Chess GUI".to_owned(),
        samples: NumSamples::One,
        vsync: true,
        icon: "".to_string(),
        srgb: true,
    };

    let wm = WindowMode {
        width: 800.0,
        height: 800.0,
        maximized: false,
        fullscreen_type: FullscreenType::Windowed,
        borderless: false,
        min_width: 1.0,
        max_width: 0.0,
        min_height: 1.0,
        max_height: 0.0,
        resizable: true,
        visible: true,
        transparent: false,
        resize_on_scale_factor_change: false,
        logical_size: Some(LogicalSize::new(800.0, 800.0)),
    };

    let cb = ggez::ContextBuilder::new("Chess GUI", "Arvid Jonasson")
        .window_setup(ws)
        .window_mode(wm);

    let (ctx, event_loop) = cb.build()?;
    let is_server = true;
    let server_color = Some(chess_network_protocol::Color::Black);
    let ip = "127.0.0.1:5000";

    let stream = network::connect(is_server, ip);
    let game = Game::new(&ctx, stream, is_server, server_color);
    event::run(ctx, event_loop, game)
}
