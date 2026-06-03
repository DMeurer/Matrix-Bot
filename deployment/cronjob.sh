cd /opt/matrix-bot || echo "$(date --utc): Failed to cd into /opt/matrix-bot, aborting..." >> "deployment/logs/cronjob.log" 2>&1

echo "$(date --utc): Starting cron job..." >> "deployment/logs/cronjob.log" 2>&1

mkdir -p deployment/logs

LOCK_FILE="/tmp/matrix-bot.lockfile"

flock -n "$LOCK_FILE" sh deployment/check-updates.sh >> "deployment/logs/cronjob.log" 2>&1
