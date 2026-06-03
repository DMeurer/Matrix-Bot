# Matrix Bot

A Matrix bot written in Rust that scrapes and displays the weekly lunch menu for the Mensa Furtwangen (Hochschule Furtwangen University cafeteria).

## Features

- Fetches the current week's menu from [swfr.de](https://www.swfr.de/essen/mensen-cafes-speiseplaene/mensa-furtwangen)
- Shows today's meals, a specific day, or the full week
- Works in private chats (DMs) and group rooms
- Automatically accepts room invites
- Persists login session across restarts (no re-login on redeploy)
- E2EE support via matrix-sdk

## Commands

| Context | Command | Result |
|---|---|---|
| DM | `mensa` | Today's menu |
| DM | `mensa 0` | Full week |
| DM | `mensa 1`–`mensa 6` | Specific day (Mon–Sat) |
| Group room | `<botname> mensa` | Today's menu |
| Group room | `<botname> mensa 0` | Full week |

## Deployment

### Prerequisites

- Docker + Docker Compose
- Git

### Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/DMeurer/Matrix-Bot.git /opt/matrix-bot
   cd /opt/matrix-bot
   ```

2. Create your `.env` file from the example:
   ```bash
   cp .env.example .env
   ```

3. Fill in your credentials in `.env`:
   ```env
   MATRIX_HOMESERVER=https://matrix.example.com
   MATRIX_USERNAME=@bot:example.com
   MATRIX_PASSWORD=your-password
   BOT_NAME=bot                          # optional, defaults to local part of username
   RUST_LOG=info,matrix_sdk_crypto=error
   ```

4. Start the bot:
   ```bash
   docker compose up -d
   ```

The bot will log in, save its session to `./session/`, and start listening for messages. The session persists across restarts — the bot will not create a new device on every redeploy.

### Automatic updates

A cron job can poll for new commits every minute and rebuild automatically.

1. Activate the cron job:
   ```bash
   crontab -e
   ```

2. Add this line:
   ```
   * * * * * sh /opt/matrix-bot/deployment/cronjob.sh
   ```

When a new commit is pushed to `main`, the cron job will pull the changes and run `docker compose up -d --build` with zero downtime. Logs are written to `deployment/logs/cronjob.log`.

### Manual redeploy

```bash
cd /opt/matrix-bot
git pull
docker compose up -d --build
```

## Development

### Run locally

```bash
cp .env.example .env   # fill in credentials
cargo run
```

### Run tests

```bash
cargo test
```
