#!/usr/bin/env bash
# Runs chessmonok on Lichess. Needs LICHESS_BOT_TOKEN (env or bot/.env).
set -euo pipefail
cd "$(dirname "$0")"
LICHESS_BOT_REF=${LICHESS_BOT_REF:-f2b38b4f3ec4fc990001c05a4000fcf8adbb339d}
[ -f .env ] && set -a && . ./.env && set +a
: "${LICHESS_BOT_TOKEN:?set LICHESS_BOT_TOKEN in bot/.env}"
if [ ! -d lichess-bot ]; then
  git clone https://github.com/lichess-bot-devs/lichess-bot.git lichess-bot
  git -C lichess-bot checkout "$LICHESS_BOT_REF"
fi
(cd .. && cargo build --release -p engine)
mkdir -p lichess-bot/engines && cp ../target/release/chessmonok lichess-bot/engines/
cd lichess-bot
[ -d .venv ] || uv venv -q --python 3.12 .venv
uv pip install -q -p .venv -r requirements.txt
[ -f ../../books/8moves_v3.bin ] || .venv/bin/python ../make_book.py ../../books/8moves_v3.pgn ../../books/8moves_v3.bin
exec .venv/bin/python lichess-bot.py --config ../config.yml
