use ggez::conf::{FullscreenType, NumSamples, WindowMode, WindowSetup};
use ggez::graphics::{Canvas, DrawMode, Image, Mesh, Rect, Transform};
use ggez::{event, graphics, Context, GameResult};
use jonathan_hallstrom_chess::{Board, Color, Move};
use mint::{Point2, Vector2};
use std::cmp::min;
use std::collections::HashMap;

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
                    graphics::Rect::new(
                        col as f32 / 8.0f32,
                        row as f32 / 8.0f32,
                        1.0f32 / 8.0f32,
                        1.0f32 / 8.0f32,
                    ),
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

struct Game {
    // Game logic
    board: Board,

    // Rendering aid
    squares: [[Square; 8]; 8],
    legal_moves: [[HashMap<(usize, usize), Vec<Move>>; 8]; 8],
    selected_from: Option<(usize, usize)>,
    selected_to: Option<(usize, usize)>,

    // Rendering
    pieces_image: Image,
    chessboard_mesh: Mesh,
    promotion_mesh: Mesh,
    selected_piece_mesh: Mesh,
    available_move_mesh: Mesh,
}

impl Game {
    fn refresh_board(&mut self) {
        self.legal_moves = parse_moves(self.board.get_legal_moves());
        self.squares = parse_fen(&self.board.to_fen());
        self.selected_from = None;
        self.selected_to = None;
    }
    fn new(ctx: &Context) -> Self {
        let board = Board::default();
        let legal_moves = parse_moves(board.get_legal_moves());
        let squares = parse_fen(&board.to_fen());
        Self {
            board,
            squares,
            legal_moves,
            selected_from: None,
            selected_to: None,
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
                    x: 0.5f32 / COL_COUNT_F32,
                    y: 0.5f32 / ROW_COUNT_F32,
                },
                0.25f32 / COL_COUNT_F32,
                0.25f32 / (COL_COUNT_F32 * 1024.0f32),
                HIGHLIGHT_COLOR,
            )
            .unwrap(),
        }
    }
    fn draw_squares(&self, canvas: &mut Canvas) {
        let (width, height) = {
            let cords = canvas.screen_coordinates().unwrap();
            (cords.w, cords.h)
        };

        canvas.draw(
            &self.chessboard_mesh,
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
            &self.pieces_image,
            graphics::DrawParam {
                src: rect,
                color: graphics::Color::WHITE,
                transform: Transform::Values {
                    dest: Point2 {
                        x: col as f32 * width / 8.0f32,
                        y: row as f32 * height / 8.0f32,
                    },
                    rotation: 0.0,
                    scale: Vector2 {
                        x: (width * 6.0f32) / (self.pieces_image.width() as f32 * 8.0f32),
                        y: (height * 2.0f32) / (self.pieces_image.height() as f32 * 8.0f32),
                    },
                    offset: Point2 { x: 0.0, y: 0.0 },
                },
                z: 0,
            },
        );
    }

    fn draw_pieces(&self, canvas: &mut Canvas) {
        for row in 0..8usize {
            for col in 0..8usize {
                // Draw piece on current square
                self.draw_piece(canvas, &self.squares[row][col], row, col);
            }
        }
    }
    fn draw_promotion_selection(&self, canvas: &mut Canvas, row: usize, col: usize) {
        let (width, height) = {
            let cords = canvas.screen_coordinates().unwrap();
            (cords.w, cords.h)
        };

        // Grey out the chessboard
        canvas.draw(
            &self.promotion_mesh,
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

        let (from_row, from_col) = self.selected_from.unwrap();

        let color = self.squares[from_row][from_col].color().unwrap();
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

    fn draw_move_selection(&self, canvas: &mut Canvas, row: usize, col: usize) {
        let (width, height) = {
            let cords = canvas.screen_coordinates().unwrap();
            (cords.w, cords.h)
        };

        canvas.draw(
            &self.selected_piece_mesh,
            graphics::DrawParam::default().dest_rect(Rect {
                x: (col as f32) * width / 8.0f32,
                y: (row as f32) * height / 8.0f32,
                w: width,
                h: height,
            }),
        );

        for ((row, col), _) in &self.legal_moves[row][col] {
            canvas.draw(
                &self.available_move_mesh,
                graphics::DrawParam::default().dest_rect(Rect {
                    x: (*col as f32) * width / 8.0f32,
                    y: (*row as f32) * height / 8.0f32,
                    w: width,
                    h: height,
                }),
            );
        }
    }
}

impl event::EventHandler for Game {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
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
        if let Some((row, col)) = self.selected_to {
            self.draw_promotion_selection(&mut canvas, row, col);
        }
        // Else draw available moves if piece is selected
        else if let Some((row, col)) = self.selected_from {
            self.draw_move_selection(&mut canvas, row, col);
        }

        // Submit drawing
        canvas.finish(ctx)
    }

    fn mouse_button_down_event(
        &mut self,
        _ctx: &mut Context,
        _button: event::MouseButton,
        x: f32,
        y: f32,
    ) -> GameResult {
        // Coerce in the range 0..=7 in case mouse pointer registers outside normal range
        let row = min((y / 100.0).abs() as usize, 7usize);
        let col = min((x / 100.0).abs() as usize, 7usize);

        let (prev_row, prev_col) = self.selected_from.unwrap_or((0usize, 0usize));

        let cords = Some((row, col));

        if self.selected_to.is_some() {
            if col == self.selected_to.as_ref().unwrap().1
                && ((self.selected_to.as_ref().unwrap().0 == 0 && row < 4)
                    || (self.selected_to.as_ref().unwrap().0 == 7 && row >= 4))
            {
                let drow =
                    (self.selected_to.as_ref().unwrap().0 as isize - row as isize).abs() as usize;
                let promotion_piece = match drow {
                    0 => jonathan_hallstrom_chess::PieceType::Queen,
                    1 => jonathan_hallstrom_chess::PieceType::Knight,
                    2 => jonathan_hallstrom_chess::PieceType::Rook,
                    3 => jonathan_hallstrom_chess::PieceType::Bishop,
                    _ => panic!("Couldn't select a promotion piece."),
                };
                let mut move_made = false;
                for mv in &self.legal_moves[prev_row][prev_col][self.selected_to.as_ref().unwrap()]
                {
                    if mv.get_promoted_type().unwrap() == promotion_piece {
                        move_made = true;
                        self.board.play_move(*mv).unwrap();
                        break;
                    }
                }
                assert!(move_made);
                self.refresh_board();
            } else {
                self.selected_to = None;
                self.selected_from = None;
            }
        } else if self.selected_from.is_some()
            && self.legal_moves[prev_row][prev_col].contains_key(cords.as_ref().unwrap())
        {
            let moves = &self.legal_moves[prev_row][prev_col][cords.as_ref().unwrap()];
            if moves.len() > 1 {
                self.selected_to = cords.clone();
            } else {
                self.board.play_move(moves[0]).unwrap();
                self.refresh_board();
            }
        } else if self.squares[row][col]
            .color()
            .map_or(false, |color| color == self.board.get_curr_player())
            && self.selected_from != cords
        {
            self.selected_from = cords;
        } else {
            self.selected_from = None;
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
        resizable: false,
        visible: true,
        transparent: false,
        resize_on_scale_factor_change: false,
        logical_size: None,
    };

    let cb = ggez::ContextBuilder::new("Chess GUI", "Arvid Jonasson")
        .window_setup(ws)
        .window_mode(wm);

    let (ctx, event_loop) = cb.build()?;
    let game = Game::new(&ctx);
    event::run(ctx, event_loop, game)
}
