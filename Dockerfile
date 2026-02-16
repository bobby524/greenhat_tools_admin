# ============================================================
# Stage 1 — build (uses Rust slim image)
# ============================================================
FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# --- dependency cache layer ---
# We write a minimal workspace Cargo.toml that only references the gateway
# crate.  The repo-level Cargo.toml also contains mcp-spike + vendored deps
# that aren't needed (and aren't copied) inside the image.
RUN printf '[workspace]\nmembers = ["gateway"]\nresolver = "2"\n' > Cargo.toml

COPY gateway/Cargo.toml gateway/Cargo.toml

# Create a dummy main so `cargo build` resolves deps
RUN mkdir -p gateway/src && echo 'fn main(){}' > gateway/src/main.rs
RUN cargo build --release --package gateway \
    && rm -rf gateway/src

# --- real source ---
COPY gateway/src gateway/src
# Touch so cargo sees the timestamp change
RUN touch gateway/src/main.rs \
    && cargo build --release --package gateway

# ============================================================
# Stage 2 — minimal runtime image
# ============================================================
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --gid 1000 app && useradd --uid 1000 --gid app --create-home app
USER app

COPY --from=builder /app/target/release/gateway /usr/local/bin/gateway

ENV PORT=8080
EXPOSE 8080

ENTRYPOINT ["gateway"]
