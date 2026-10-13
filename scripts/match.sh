#!/usr/bin/env bash
# Engine-vs-engine SPRT match. usage: scripts/match.sh NEW OLD [rounds]
# Extra engine options: NEW_OPTS="option.X=1" OLD_OPTS="option.RandomMover=true"
set -euo pipefail
cd "$(dirname "$0")/.."
NEW=${1:?path to the new engine}
OLD=${2:?path to the old engine}
ROUNDS=${3:-200}
mkdir -p matches
# shellcheck disable=SC2086
tools/fastchess \
  -engine cmd="$NEW" name=new ${NEW_OPTS:-} \
  -engine cmd="$OLD" name=old ${OLD_OPTS:-} \
  -each proto=uci tc=8+0.08 \
  -openings file=books/8moves_v3.pgn format=pgn order=random \
  -rounds "$ROUNDS" -games 2 -repeat -concurrency "${CONCURRENCY:-4}" \
  -sprt elo0=0 elo1=10 alpha=0.05 beta=0.05 \
  -pgnout file="matches/$(date +%Y%m%d-%H%M%S).pgn"
