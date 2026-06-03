# rebuilding the docker compose, with zero downtime
docker compose up -d --no-deps --build

# remove the old images
docker image prune -f
