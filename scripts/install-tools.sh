#!/usr/bin/env bash
# Fetches fastchess and an opening book into gitignored tools/ and books/.
set -euo pipefail
cd "$(dirname "$0")/.."
# v1.8.1+ is built for macOS 26; v1.8.0 runs on macOS 15.
FASTCHESS_TAG=${FASTCHESS_TAG:-v1.8.0-alpha}
rm -rf tools && mkdir -p tools books
curl -fsSL "https://github.com/Disservin/fastchess/releases/download/${FASTCHESS_TAG}/fastchess-mac-arm64.tar" | tar -x -C tools
find tools -name fastchess -type f -exec chmod +x {} \; -exec ln -sf "$PWD/{}" tools/fastchess \;
curl -fsSL -o books/8moves_v3.pgn.zip https://github.com/official-stockfish/books/raw/master/8moves_v3.pgn.zip
unzip -o -q books/8moves_v3.pgn.zip -d books
tools/fastchess --version
