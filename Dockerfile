FROM rust:1.87-slim AS builder
WORKDIR /app
COPY . .
RUN apt-get update && apt-get install -y pkg-config && rm -rf /var/lib/apt/lists/*
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/Matrix-Bot .
CMD ["./Matrix-Bot"]
