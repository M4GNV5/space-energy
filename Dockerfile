# --- client: static bundle ---
FROM node:22-bookworm-slim AS client
WORKDIR /app/client
COPY client/package.json client/package-lock.json ./
RUN npm ci
COPY client/ ./
RUN npm run build

# --- server: release binary ---
FROM rust:1.90-slim-bookworm AS server
WORKDIR /app
COPY rust-toolchain.toml ./
COPY server/Cargo.toml server/Cargo.lock ./server/
# Build the dependencies against stub sources so they stay cached until Cargo.* changes.
RUN mkdir -p server/src \
    && echo "fn main() {}" > server/src/main.rs \
    && touch server/src/lib.rs \
    && cd server && cargo build --release --locked
COPY server/src ./server/src
RUN cd server && touch src/main.rs src/lib.rs && cargo build --release --locked

# --- runtime ---
FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --no-create-home game \
    && mkdir /data && chown game:game /data
COPY --from=server /app/server/target/release/server /usr/local/bin/server
COPY --from=client /app/client/dist /srv/client

ENV PORT=8080 \
    CLIENT_DIR=/srv/client \
    SAVE_FILE=/data/save.json
USER game
WORKDIR /data
VOLUME /data
EXPOSE 8080
# The server saves on Ctrl-C (SIGINT), not on SIGTERM.
STOPSIGNAL SIGINT
CMD ["server"]
