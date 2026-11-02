# chessmonok on Lichess

`run.sh` plays chessmonok on Lichess through
[lichess-bot](https://github.com/lichess-bot-devs/lichess-bot) (AGPL-3.0, used
unmodified). It clones lichess-bot at a pinned commit into `bot/lichess-bot/`,
installs it with uv, builds the release engine and starts the bot with
`config.yml`: standard chess, blitz only, one game at a time, casual and rated.
When idle for a minute it challenges other bots to rated 3+0, 3+2, 5+0 or 5+2
games, and it keeps every game's PGN in `bot/lichess-bot/game_records/`.

The engine evaluates with the NNUE net `nets/current.nnue`, a symlink to the
net to play (`run.sh` refuses to start without it):

```sh
ln -sf run1-e11.nnue nets/current.nnue
```

The bot plays from its first move on its own: the local opening book is off,
so its results reflect the engine. To play the first 8 moves from a book
instead, set `polyglot: enabled: true` in `config.yml`; `run.sh` then builds
the book once from `books/8moves_v3.pgn` (fetched by
`scripts/install-tools.sh`) with `make_book.py`. Online books stay off.

## One-time setup

1. Create a **new** Lichess account. It must never have played a game: only
   such accounts can become BOT accounts.
2. Logged in as that account, create a personal API token at
   <https://lichess.org/account/oauth/token> with the scope **Play games with
   the bot API** (`bot:play`).
3. Upgrade the account to BOT. **This is irreversible**: a BOT account can never
   play as a human again.

   ```sh
   curl -d '' https://lichess.org/api/bot/account/upgrade \
     -H "Authorization: Bearer <token>"
   ```

4. Store the token where git never sees it (`bot/.env` is gitignored):

   ```sh
   echo "LICHESS_BOT_TOKEN=<token>" > bot/.env
   ```

## Playing

```sh
bot/run.sh
```

Then, from your own account, challenge the bot to a casual blitz game. Stop it
with Ctrl-C: it finishes the game in progress first, so a rated game is never
abandoned.
