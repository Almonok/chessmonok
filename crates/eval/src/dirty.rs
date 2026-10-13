use std::ops::Deref;

use shakmaty::{Chess, Color, Move, Piece, Position, Role, Square};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyPiece {
    pub piece: Piece,
    pub from: Option<Square>,
    pub to: Option<Square>,
}

/// Every piece one move disturbed: at most three, for a capturing promotion.
#[derive(Clone, Copy, Debug)]
pub struct DirtyPieces {
    items: [DirtyPiece; 3],
    len: usize,
}

impl DirtyPieces {
    const FILLER: DirtyPiece = DirtyPiece {
        piece: Piece {
            color: Color::White,
            role: Role::Pawn,
        },
        from: None,
        to: None,
    };

    fn new() -> Self {
        Self {
            items: [Self::FILLER; 3],
            len: 0,
        }
    }

    fn push(&mut self, piece: Piece, from: Option<Square>, to: Option<Square>) {
        self.items[self.len] = DirtyPiece { piece, from, to };
        self.len += 1;
    }
}

impl Deref for DirtyPieces {
    type Target = [DirtyPiece];

    fn deref(&self) -> &[DirtyPiece] {
        &self.items[..self.len]
    }
}

pub fn dirty_pieces(before: &Chess, mv: Move) -> DirtyPieces {
    let us = before.turn();
    let ours = |role| Piece { color: us, role };
    let theirs = |role| Piece {
        color: us.other(),
        role,
    };
    let mut dirty = DirtyPieces::new();
    match mv {
        Move::Normal {
            role,
            from,
            capture,
            to,
            promotion,
        } => {
            if let Some(captured) = capture {
                dirty.push(theirs(captured), Some(to), None);
            }
            match promotion {
                None => dirty.push(ours(role), Some(from), Some(to)),
                Some(promoted) => {
                    dirty.push(ours(role), Some(from), None);
                    dirty.push(ours(promoted), None, Some(to));
                }
            }
        }
        Move::EnPassant { from, to } => {
            dirty.push(ours(Role::Pawn), Some(from), Some(to));
            dirty.push(
                theirs(Role::Pawn),
                Some(Square::from_coords(to.file(), from.rank())),
                None,
            );
        }
        Move::Castle { king, rook } => {
            let side = mv.castling_side().expect("a castle has a side");
            dirty.push(ours(Role::King), Some(king), Some(side.king_to(us)));
            dirty.push(ours(Role::Rook), Some(rook), Some(side.rook_to(us)));
        }
        Move::Put { .. } => unreachable!("drops do not exist in standard chess"),
    }
    dirty
}
