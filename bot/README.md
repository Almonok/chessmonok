# chessmonok on Lichess

`run.sh` plays chessmonok on Lichess through
[lichess-bot](https://github.com/lichess-bot-devs/lichess-bot) (AGPL-3.0, used
unmodified). It clones lichess-bot at a pinned commit into `bot/lichess-bot/`,
installs it with uv, builds the release engine and starts the bot with
`config.yml`: standard chess, bullet/blitz/rapid, one game at a time, casual
games only.

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
with Ctrl-C.
