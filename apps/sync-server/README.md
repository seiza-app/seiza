# Seiza sync server

A small server you run yourself so Seiza can sync between your computers (PLAN.md §8, Sync). It's
one Rust program with one SQLite file, in one Docker image.

> **Not in use yet.** Settings › Sync can start a group and pair devices with it in development
> builds, but the app doesn't send or fetch anything until the next step of the build order.

## What it can and can't see

Everything a group stores is encrypted by the app before it's sent, with a key the server never
gets. The server keeps:

- each **sync group** (about one per person) and its **devices**, with when each was added and
  last seen;
- each device's token as a **hash** only, so a copy of the database can't sign in;
- each group's **batches** of encrypted records, in order, and their sizes and times;
- each device's **label** (its name), also encrypted by the app.

So it can see who's in which group, how much they store and when they sync, and nothing else.
Pairing a new device happens through it too, but the code's words are never sent to it (only the
number at the front, which picks the pairing).

## Running it with Docker

CI publishes the image to GitHub's registry as `ghcr.io/seiza-app/seiza-sync`, for both
`linux/amd64` and `linux/arm64`. Copy [compose.example.yml](compose.example.yml) into a folder as
`compose.yml`, change what you need, and start it:

```sh
docker compose up -d
```

It listens on port 8080 and keeps its database in the `seiza-sync-data` volume. Its health check
is `seiza-sync health`.

### Your first group

**Start your own group straight away.** By default the server accepts exactly one group, the first
device's, and then closes to new ones, so it's yours from the moment your first computer joins.

### Updating

Starting the container doesn't fetch a newer image by itself. To update, from the folder with
`compose.yml`:

```sh
docker compose pull && docker compose up -d
```

The database stays in its volume, and the server updates it to the new version's layout when it
starts, so back it up first (see below) if you want a way back. The tags:

- `latest`: the newest build of `main`, once its checks pass.
- `sha-<commit>`: that one build, e.g. to go back to an earlier one.
- a version like `0.1.0`: from a `sync-server-v0.1.0` tag, once there are versions.

To update without thinking about it, a tool like Watchtower can run the pull for you on a
schedule. Building the image yourself works too: use `build: .` in `compose.yml` instead of
`image:`, and `docker compose up -d --build` after `git pull`.

## In a Proxmox LXC behind a Cloudflare Tunnel

1. Make an LXC (Debian is fine) with Docker in it, and run the server as above. (Without Docker:
   build it with `cargo build --release` and run `target/release/seiza-sync` as a systemd service,
   with the settings below in its environment.)
2. In Cloudflare's Zero Trust dashboard, add a public hostname to your tunnel, e.g.
   `sync.example.com`, pointing at `http://<the LXC's address>:8080`. Cloudflare serves it over
   HTTPS; the server itself only speaks plain HTTP, so don't expose port 8080 to the internet any
   other way.
3. Set `SEIZA_SYNC_CLIENT_IP_HEADER=CF-Connecting-IP`, so the server's rate limits count each
   visitor rather than the tunnel.
4. Optionally, add a Cloudflare rate-limiting rule for the hostname as well.

Cloudflare Access doesn't fit here: it's made for people signing in through a browser, and the app
talks to the server directly. The server's own protections are below.

## Settings

All from the environment:

| Variable | Default | What it does |
|---|---|---|
| `SEIZA_SYNC_ADDR` | `0.0.0.0:8080` | Where it listens. |
| `SEIZA_SYNC_DB` | `seiza-sync.db` (`/data/seiza-sync.db` in Docker) | The database file. |
| `SEIZA_SYNC_NEW_GROUPS` | `first-group-only` | Who can start a sync group: `first-group-only` (only while the server has none), `invites` (each needs a code from `seiza-sync invite`), or `open` (anyone). |
| `SEIZA_SYNC_GROUP_MAX_MB` | `50` | The most each group can store. |
| `SEIZA_SYNC_CLIENT_IP_HEADER` | none | A header from a proxy in front with the visitor's address, e.g. `CF-Connecting-IP`. Only set it when every request comes through that proxy, since anyone could send the header otherwise. |

## Commands

In Docker, run them with `docker compose exec seiza-sync seiza-sync <command>`.

- `seiza-sync stats`: counts groups, devices, groups active in the last 7 and 30 days, and what's
  stored. These come from what the server keeps anyway; the app sends nothing extra.
- `seiza-sync invite`: a one-time code for one new group, valid for 7 days (for
  `SEIZA_SYNC_NEW_GROUPS=invites`).
- `seiza-sync backup <file>`: copies the database to a new file, safely while it runs.
- `seiza-sync health`: checks that the server here answers.

## Backups

Every device keeps its whole log, so losing the server loses nothing: point your devices at a new
one (Settings › Sync › Move to another server, once it exists) and one of them uploads everything
again. A backup just saves you re-pairing:

```sh
docker compose exec seiza-sync seiza-sync backup /data/backup.db
docker compose cp seiza-sync:/data/backup.db ./seiza-sync-backup.db
```

To restore, stop the server and put the backup in place of `/data/seiza-sync.db`.

## Protections

- Every request to `/v1` needs the header `Seiza-Protocol: 1`. It isn't a secret (the app is open
  source, so nothing built into it could be): it lets the server drop scanners and stray traffic
  before doing any work.
- Groups are protected by their devices' tokens, and new devices only get in through a pairing:
  single use, five minutes, one new device per pairing, and either side can end it, which the app
  does as soon as a code turns out wrong. A device that was added but never collected its token
  (the pairing ended, expired or was replaced first) is taken out of the group again.
- Rate limits per address, each minute: 300 requests, 10 attempts to join a pairing, and 3 new
  groups.
- Size caps: 1 MB per batch, 4 KB per pairing message, and the group cap above.

## The API

All under `/v1`, in JSON with camelCase names. Encrypted data is base64, except uploaded batches,
which are the raw request body. Devices send `Authorization: Bearer <token>`.

| Request | Who | What it does |
|---|---|---|
| `POST /groups` `{label, invite?}` | anyone, if allowed | Starts a group with its first device. Returns `{groupId, deviceId, token}`. |
| `GET /devices` | a device | The group's devices, `{devices: [{id, label, createdAt, lastSeenAt}], you}`. |
| `DELETE /devices/{id}` | a device | Removes a device (Remove, or Leave sync for itself). The group goes once its last device does. |
| `POST /batches` (raw body) | a device | Adds a batch. Returns `{seq}`. |
| `GET /batches?after=N` | a device | Batches after `N`, in order: `{batches: [{seq, deviceId, data, createdAt}], more}`. |
| `POST /pairings` | a device | Opens a pairing: `{slot, expiresAt}`. A group has one open at a time. |
| `POST /pairings/{slot}/join` | the new device | Joins it: `{joinerKey, expiresAt}`. The joiner key is its token for the rest of the pairing. |
| `POST /pairings/{slot}/messages` (raw body) | either side | Sends the other side a message. |
| `GET /pairings/{slot}/messages?after=N` | either side | The other side's messages after the first `N`: `{messages, joined}`. |
| `POST /pairings/{slot}/admit` `{label}` | the device that opened it | Adds the new device once its person says Add: `{deviceId}`. |
| `GET /pairings/{slot}/device` | the new device | Its `{groupId, deviceId, token}` once admitted (204 until then). This ends the pairing. |
| `DELETE /pairings/{slot}` | either side | Ends the pairing. |

`GET /health` answers `ok`, with no header needed.

## Developing

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo deny check    # after cargo install cargo-deny --locked
```

Dependencies must have been released at least two weeks ago, the whole lockfile included.
