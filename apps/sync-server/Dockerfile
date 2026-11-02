# Seiza's sync server in one small image (PLAN §8, Sync). CI publishes it as
# ghcr.io/seiza-app/seiza-sync; to build it yourself, from this folder:
#   docker build -t seiza-sync .
# compose.example.yml shows how to run it; README.md has the rest.

# Alpine's Rust links against musl, so the program comes out as one static file that needs
# nothing else from the system: the image is that file and an empty data folder.
FROM rust:1.99-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked && mkdir /data

FROM scratch
# The source label links the published image to the repository on GitHub.
LABEL org.opencontainers.image.source=https://github.com/seiza-app/seiza \
  org.opencontainers.image.description="Seiza's self-hostable sync server" \
  org.opencontainers.image.licenses=AGPL-3.0-only
COPY --from=build /src/target/release/seiza-sync /usr/local/bin/seiza-sync
# Runs as its own user, which only owns the data folder. An empty image has no user list, so
# it's a number: the same one as images before this, so their data folders still fit.
COPY --from=build --chown=10001:10001 /data /data
USER 10001:10001
ENV PATH=/usr/local/bin \
  SEIZA_SYNC_DB=/data/seiza-sync.db \
  SEIZA_SYNC_ADDR=0.0.0.0:8080
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s CMD ["seiza-sync", "health"]
CMD ["seiza-sync", "serve"]
