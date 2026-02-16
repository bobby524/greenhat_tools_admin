# syntax=docker/dockerfile:1

# ============================================================
# Stage 1 — build
# ============================================================
FROM rust:1.90-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Build dependency cache layer first
COPY gateway/Cargo.toml ./Cargo.toml
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs
RUN cargo build --release && rm -rf src

# Real source
COPY gateway/src ./src
RUN cargo build --release

# ============================================================
# Stage 2 — runtime
# ============================================================
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --gid 1000 app && useradd --uid 1000 --gid app --create-home app

COPY --from=builder /app/target/release/gateway /usr/local/bin/gateway

USER app
ENV PORT=8080
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/gateway"]
