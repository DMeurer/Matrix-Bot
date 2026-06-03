echo "$(date --utc): Checking for updates..."
git fetch

UPSTREAM=${1:-'@{u}'}
LOCAL=$(git rev-parse @)
REMOTE=$(git rev-parse "$UPSTREAM")
BASE=$(git merge-base @ "$UPSTREAM")

if [ $LOCAL = $REMOTE ]; then
  echo "$(date --utc): No changes detected. Exiting..."
elif [ $LOCAL = $BASE ]; then
  echo "$(date --utc): Changes detected. Pulling changes and rebuilding..."
  git pull
  sh ./deployment/deploy.sh
elif [ $REMOTE = $BASE ]; then
  echo "$(date --utc): Local changes detected. Stashing and rebuilding..."
  git stash
  sh ./deployment/deploy.sh
else
  echo "$(date --utc): Diverged branches detected. Exiting..."
fi
