# Matrix Bot

A Matrix bot written in Rust. It shows the weekly lunch menu for Mensa Furtwangen (HFU) and monitors websites for changes, sending a notification whenever a tracked value updates.

## Features

- **Mensa menu** — fetches the current week's menu from [swfr.de](https://www.swfr.de/essen/mensen-cafes-speiseplaene/mensa-furtwangen); show today, a specific day, or the full week
- **Website alerts** — monitor any element on any page; get notified when its text, HTML, or any attribute changes
- **Access control** — admin users (set via env) plus a mutable allow-list
- **Works in DMs and group rooms** — no prefix needed in DMs; prefix with the bot name in group rooms
- **Auto-join** — accepts room invites automatically
- **Persistent session** — login is saved to disk; no new device created on redeploy
- **E2EE** — end-to-end encryption via matrix-sdk

---

## Configuration

Copy `.env.example` to `.env` and fill in your values:

```env
MATRIX_HOMESERVER=https://matrix.example.com
MATRIX_USERNAME=@bot:example.com
MATRIX_PASSWORD=your-password

# Optional — defaults to the local part of MATRIX_USERNAME
BOT_NAME=bot

# Comma-separated list of Matrix IDs that always have full access
ADMIN_USERS=@you:example.com,@other:example.com

# Template for change notifications — all placeholders are optional
ALERT_TEMPLATE=Alert {name} changed\n\nOld: {old}\nNew: {new}

# Log level
RUST_LOG=info,matrix_sdk_crypto=error
```

**Template placeholders:** `{name}`, `{url}`, `{old}`, `{new}`, `{css}`, `{property}`

---

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

2. Create and fill in your `.env`:
   ```bash
   cp .env.example .env
   $EDITOR .env
   ```

3. Start the bot:
   ```bash
   docker compose up -d
   ```

The bot logs in, saves its session to `./session/`, and starts listening. The `session/` directory is bind-mounted so it survives container rebuilds.

### Automatic updates

A cron job polls for new commits every minute and rebuilds automatically with zero downtime.

1. Make sure the current user can run `docker` without `sudo`:
   ```bash
   sudo usermod -aG docker $USER
   newgrp docker          # activates the group without logging out
   ```

2. Add to crontab (`crontab -e`):
   ```
   * * * * * sh /opt/matrix-bot/deployment/cronjob.sh
   ```

When a new commit lands on `main`, the job pulls the changes and runs `docker compose up -d --no-deps --build`. Concurrent runs are prevented by a lockfile. Logs are written to `deployment/logs/cronjob.log`.

### Manual redeploy

```bash
git pull
docker compose up -d --build
```

---

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
