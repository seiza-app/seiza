# Seiza's sync server in one small image (PLAN §8, Sync). CI publishes it as
# ghcr.io/seiza-app/seiza-sync; to build it yourself, from this folder:
#   docker build -t seiza-sync .
# compose.example.yml shows how to run it; README.md has the rest.

FROM rust:1.99-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:trixie-slim
# The source label links the published image to the repository on GitHub.
LABEL org.opencontainers.image.source=https://github.com/seiza-app/seiza \
  org.opencontainers.image.description="Seiza's self-hostable sync server" \
  org.opencontainers.image.licenses=AGPL-3.0-only
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
