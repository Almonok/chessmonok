"""Builds a polyglot opening book from a PGN of openings, for lichess-bot.

Every position reached in the PGN gets the moves played from it, weighted by how
often they were played. Usage: make_book.py <openings.pgn> <book.bin>
"""

import struct
import sys
from collections import Counter

import chess
import chess.pgn
import chess.polyglot

PROMOTIONS = {None: 0, chess.KNIGHT: 1, chess.BISHOP: 2, chess.ROOK: 3, chess.QUEEN: 4}


def encode(board: chess.Board, move: chess.Move) -> int:
    """Polyglot move: to-file, to-rank, from-file, from-rank, promotion; castling is king-takes-rook."""
    to = move.to_square
    if board.is_castling(move):
        rook_file = 7 if chess.square_file(to) > chess.square_file(move.from_square) else 0
        to = chess.square(rook_file, chess.square_rank(to))
    return (to | move.from_square << 6) | PROMOTIONS[move.promotion] << 12


def main(pgn_path: str, book_path: str) -> None:
    counts: Counter[tuple[int, int]] = Counter()
    games = 0
    with open(pgn_path) as pgn:
        while (game := chess.pgn.read_game(pgn)) is not None:
            board = game.board()
            for move in game.mainline_moves():
                counts[chess.polyglot.zobrist_hash(board), encode(board, move)] += 1
                board.push(move)
            games += 1
    entries = sorted(counts.items(), key=lambda e: (e[0][0], -e[1]))
    with open(book_path, "wb") as book:
        for (key, move), weight in entries:
            book.write(struct.pack(">QHHI", key, move, min(weight, 0xFFFF), 0))
    print(f"{book_path}: {len(entries)} entries from {games} games")


if __name__ == "__main__":
    main(*sys.argv[1:])
