# Seiza's sync server in one small image (PLAN §8, Sync). Build it from this folder:
#   docker build -t seiza-sync .
# compose.example.yml shows how to run it; README.md has the rest.

FROM rust:1.99-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:trixie-slim
# Runs as its own user, which only owns the data folder.
RUN useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin seiza \
  && mkdir /data && chown seiza:seiza /data
COPY --from=build /src/target/release/seiza-sync /usr/local/bin/seiza-sync
USER seiza
ENV SEIZA_SYNC_DB=/data/seiza-sync.db \
  SEIZA_SYNC_ADDR=0.0.0.0:8080
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s CMD ["seiza-sync", "health"]
CMD ["seiza-sync", "serve"]
