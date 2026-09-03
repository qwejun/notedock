# NoteDock server image.
#
# Two stages so the runtime layer carries no Rust toolchain. The dependency
# pre-build is what keeps rebuilds fast: application code changes far more often
# than the lockfile, and this way only the last layer is invalidated.
FROM rust:1.98-slim-bookworm AS build

WORKDIR /src

# SQLite is vendored by sqlx, but the build still needs a C compiler for it.
RUN apt-get update \
 && apt-get install -y --no-install-recommends build-essential \
 && rm -rf /var/lib/apt/lists/*

# Manifests first, with stub sources, so `cargo build` caches the dependency
# graph in its own layer.
COPY Cargo.toml Cargo.lock ./
COPY crates/notedock-api/Cargo.toml crates/notedock-api/
COPY crates/notedock-server/Cargo.toml crates/notedock-server/
RUN mkdir -p crates/notedock-api/src crates/notedock-server/src \
 && echo 'pub fn main() {}' > crates/notedock-server/src/main.rs \
 && touch crates/notedock-api/src/lib.rs \
 && cargo build --release -p notedock-server \
 && rm -rf crates/notedock-api/src crates/notedock-server/src

COPY crates/notedock-api crates/notedock-api
COPY crates/notedock-server crates/notedock-server
# Cargo skips a rebuild when only mtimes changed, so nudge the crates it must
# recompile with the real sources.
RUN touch crates/notedock-api/src/lib.rs crates/notedock-server/src/main.rs \
 && cargo build --release -p notedock-server

FROM debian:bookworm-slim AS runtime

# Needed only if the deployment later terminates TLS in this container; harmless
# now and avoids a surprising rebuild then.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /data notedock

COPY --from=build /src/target/release/notedock-server /usr/local/bin/notedock-server

# The browser client is built by CI (and by the documented local deployment
# step) before this image is created, so the published image is self-contained.
COPY apps/web/dist /srv/web

# Database and, later, blobs. Mounted as a volume; nothing persists in the image.
RUN mkdir -p /data && chown -R notedock:notedock /data /srv/web
VOLUME ["/data"]
USER notedock

ENV NOTEDOCK_BIND=[::]:8080 \
    NOTEDOCK_DB=/data/notedock.db \
    NOTEDOCK_LOG=notedock_server=info

EXPOSE 8080

# No shell form: the process must be PID 1 so Docker's stop signal reaches it and
# the graceful shutdown actually runs.
ENTRYPOINT ["/usr/local/bin/notedock-server"]
