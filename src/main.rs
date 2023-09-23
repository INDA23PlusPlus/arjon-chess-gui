use ggez::conf::{FullscreenType, NumSamples, WindowMode, WindowSetup};
use ggez::graphics::Transform;
use ggez::{event, graphics, Context, GameResult};
use jonathan_hallstrom_chess::{Board, Color, Move};
use mint::{Point2, Vector2};
use std::cmp::min;
use std::collections::HashMap;

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

enum PromotablePieces {
    Queen,
    Knight,
    Bishop,
    Rook,
    NA,
}
struct Game {
    board: Board,
    squares: [[Square; 8]; 8],
    legal_moves: [[HashMap<(usize, usize), Vec<Move>>; 8]; 8],
    selected_from: Option<(usize, usize)>,
    selected_to: Option<(usize, usize)>,
    pieces_image: graphics::Image,
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
            pieces_image: graphics::Image::from_bytes(ctx, include_bytes!("Pieces.png")).unwrap(),
        }
    }
}

impl event::EventHandler for Game {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(ctx, graphics::Color::WHITE);

        // Draw board squares
        for row in 0..8usize {
            for col in 0..8usize {
                let color = if (row + col) % 2 == 0 {
                    graphics::Color::new(1.0, 0.9, 0.9, 1.0)
                } else {
                    graphics::Color::new(0.9, 0.7, 0.7, 1.0)
                };
                let rect = graphics::Mesh::new_rectangle(
                    ctx,
                    graphics::DrawMode::fill(),
                    graphics::Rect::new_i32((col * 100) as i32, (row * 100) as i32, 100, 100),
                    color,
                )?;
                canvas.draw(&rect, graphics::DrawParam::default());

                // Draw piece on current square
                if self.squares[row][col] != Square::Empty {
                    let piece = &self.squares[row][col];
                    let color = piece.color().unwrap();

                    let rect = graphics::Rect::new(
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
                    canvas.draw(
                        &self.pieces_image,
                        graphics::DrawParam {
                            src: rect,
                            color: graphics::Color::WHITE,
                            transform: Transform::Values {
                                dest: Point2 {
                                    x: (col * 100) as f32,
                                    y: (row * 100) as f32,
                                },
                                rotation: 0.0,
                                scale: Vector2 { x: 1.0, y: 1.0 },
                                offset: Point2 { x: 0.0, y: 0.0 },
                            },
                            z: 0,
                        },
                    );
                }
            }
        }

        // Draw selection for promotion
        if let Some((row, col)) = self.selected_to {
            // Grey out the arena
            let greyed = graphics::Color::new(0.0, 0.0, 0.0, 0.75);
            let rect = graphics::Mesh::new_rectangle(
                ctx,
                graphics::DrawMode::fill(),
                graphics::Rect::new_i32(0, 0, 800, 800),
                greyed,
            )?;
            canvas.draw(&rect, graphics::DrawParam::default());
            let dir = match row {
                7 => -1isize,
                0 => 1isize,
                _ => panic!("Promotion not on last row"),
            };

            let (from_row, from_col) = self.selected_from.unwrap();

            let color = self.squares[from_row][from_col].color().unwrap();

            for dy in 0..4 {
                let rect = graphics::Rect::new(
                    (1.0 + dy as f32) / 6.0,
                    match color {
                        Color::White => 0.0,
                        Color::Black => 1.0,
                    } / 2.0,
                    1.0 / 6.0,
                    1.0 / 2.0,
                );
                canvas.draw(
                    &self.pieces_image,
                    graphics::DrawParam {
                        src: rect,
                        color: graphics::Color::WHITE,
                        transform: Transform::Values {
                            dest: Point2 {
                                x: (col * 100) as f32,
                                y: ((row as isize + dy * dir) * 100) as f32,
                            },
                            rotation: 0.0,
                            scale: Vector2 { x: 1.0, y: 1.0 },
                            offset: Point2 { x: 0.0, y: 0.0 },
                        },
                        z: 0,
                    },
                );
            }
        }
        // Draw available moves
        else if let Some((row, col)) = self.selected_from {
            let color = graphics::Color::new(0.0, 0.5, 0.0, 0.75);
            let rect = graphics::Mesh::new_rectangle(
                ctx,
                graphics::DrawMode::fill(),
                graphics::Rect::new_i32((col * 100) as i32, (row * 100) as i32, 100, 100),
                color,
            )?;
            canvas.draw(&rect, graphics::DrawParam::default());

            for ((row, col), _) in &self.legal_moves[row][col] {
                let circle = graphics::Mesh::new_circle(
                    ctx,
                    graphics::DrawMode::fill(),
                    Point2 {
                        x: (col * 100 + 50) as f32,
                        y: (row * 100 + 50) as f32,
                    },
                    25.0,
                    0.1,
                    color,
                )?;
                canvas.draw(&circle, graphics::DrawParam::default());
            }
        }

        canvas.finish(ctx)
    }

    fn mouse_button_down_event(
        &mut self,
        ctx: &mut Context,
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
                    1 => jonathan_hallstrom_chess::PieceType::Bishop,
                    2 => jonathan_hallstrom_chess::PieceType::Knight,
                    3 => jonathan_hallstrom_chess::PieceType::Rook,
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
        title: "Chess".to_owned(),
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
