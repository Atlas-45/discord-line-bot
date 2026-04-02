FROM rust:1-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/discord-line /usr/local/bin/discord-line

RUN mkdir -p /data

ENV BIND_ADDR=0.0.0.0:8080
ENV DATABASE_URL=sqlite:///data/data.sqlite
ENV RUST_LOG=info

EXPOSE 8080

CMD ["discord-line"]
