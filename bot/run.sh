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
NET=../nets/current.nnue
if [ ! -f "$NET" ]; then
  echo "nets/current.nnue is missing: link it to the net to play, e.g." >&2
  echo "  ln -s run1-e11.nnue nets/current.nnue" >&2
  exit 1
fi
(cd .. && cargo build --release -p engine)
mkdir -p lichess-bot/engines
cp ../target/release/chessmonok lichess-bot/engines/
cp -L "$NET" lichess-bot/engines/current.nnue
cd lichess-bot
[ -d .venv ] || uv venv -q --python 3.12 .venv
uv pip install -q -p .venv -r requirements.txt
# The opening book is only built when config.yml enables it.
if grep -A1 '^  polyglot:' ../config.yml | grep -q 'enabled: true'; then
  [ -f ../../books/8moves_v3.bin ] || .venv/bin/python ../make_book.py ../../books/8moves_v3.pgn ../../books/8moves_v3.bin
fi
exec .venv/bin/python lichess-bot.py --config ../config.yml
