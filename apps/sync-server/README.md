# Seiza sync server

A small server you run yourself so Seiza can sync between your computers (PLAN.md §8, Sync). It's
one Rust program with one SQLite file, in one Docker image of a few megabytes. It runs on one
thread in about 5 MB of memory, and answers in under a millisecond, ten devices at once included.

It syncs each computer's finished sessions (with their goals), visits to redirected sites,
constellation names, profiles and satellites, and live sessions: one started on one computer starts on the
others that follow the group's sessions (Settings › Sync).

## Setting it up, start to finish

1. **Run the server** with Docker ([below](#running-it-with-docker)), on any Linux machine that
   stays on: a Proxmox LXC, a NAS, a Raspberry Pi or a small VPS.
2. **Give it an HTTPS address.** The app talks to plain `http://` only on the same computer or the
   local network, so to sync from anywhere, put it behind HTTPS: a Cloudflare Tunnel
   ([below](#in-a-proxmox-lxc-behind-a-cloudflare-tunnel)), or a reverse proxy like Caddy that
   gets a certificate for you.
3. **Choose who can start a group** (`SEIZA_SYNC_NEW_GROUPS`, [below](#settings)). The default
   suits a server for just you. For friends, see [Sharing it](#sharing-it-with-friends).
4. **Connect your first computer.** In Seiza, open Settings › Sync, type the server's address under
   **Use your own server**, check **This computer's name**, and choose **Start a new sync group**.
5. **Add your other computers.** On the first one, choose **Add a device**: it shows a code like
   `4821-orbit-lamp-quote-river`, good once, for five minutes. On the new one, type the same
   address, choose **Join with a pairing code** and enter it. The first computer asks "Add …?";
   choose **Add**. If the new computer already has sessions of its own, it offers **Join and
   match the sky**, which keeps its stars and redraws its rare events to match the group's.

Settings › Sync then lists the group's computers, when each was last seen and when this one last
synced, with **Remove** for a lost one, **Move…** to another server, and **Leave sync…**.

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

While a device syncs, it keeps one connection open (`GET /v1/live`), and the server tells it when
another device in its group sends something, so a session started on one computer starts on the
others within seconds. The same connection passes short encrypted messages between a group's
devices (asking whether anyone's been at their computer, before an idle session ends), which the
server never keeps. It's plain HTTP (server-sent events) with a comment every 40 seconds, so it
passes through a Cloudflare Tunnel and most reverse proxies as it is. Behind nginx, it sends
`X-Accel-Buffering: no` so nginx doesn't hold events back. A server from before this still syncs,
just at the moments the app already syncs (when its window opens, when a session starts or ends).

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

**Start your own group straight away.** By default the server takes exactly one group without
asking, the first device's, and then closes to new ones for good, so it's yours from the moment
your first computer joins. It stays closed even if that group is gone (its last computer chose
**Leave sync…**, or **Move…** to another server). To start a group there again, make an invite
code (below) and type it in the **Invite code** field Settings › Sync shows.

### Sharing it with friends

Each person gets a group of their own, which the others can't read or join. To let friends start
theirs, set `SEIZA_SYNC_NEW_GROUPS: invites` in `compose.yml`, run `docker compose up -d` again,
and make a code for each friend:

```sh
docker compose exec seiza-sync seiza-sync invite
```

Each code starts one group and lasts 7 days. Your friend types it in the **Invite code** field
Settings › Sync shows when they start a group. Your own group stays as it is. To keep an eye on
things, `seiza-sync stats` counts groups, devices and storage, and `SEIZA_SYNC_GROUP_MAX_MB` caps
what each group can store (50 MB by default, years of use).

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

1. Make an LXC (Debian is fine; 1 core, 256 MB of memory and a couple of GB of disk is plenty).
   Docker in an LXC needs **Options › Features › Nesting** on, and `keyctl` too for an
   unprivileged one. Install Docker in it and run the server as above. Or skip Docker
   ([below](#without-docker)).
2. In Cloudflare's Zero Trust dashboard, add a public hostname to your tunnel, e.g.
   `sync.example.com`, pointing at `http://<the LXC's address>:8080`. Cloudflare serves it over
   HTTPS; the server itself only speaks plain HTTP, so don't expose port 8080 to the internet any
   other way.
3. Set `SEIZA_SYNC_CLIENT_IP_HEADER=CF-Connecting-IP`, so the server's rate limits count each
   visitor rather than the tunnel.
4. Optionally, add a Cloudflare rate-limiting rule for the hostname as well.

Cloudflare Access doesn't fit here: it's made for people signing in through a browser, and the app
talks to the server directly. The server's own protections are below.

### Without Docker

The server is one program with no other parts. Build it with Rust (`cargo build --release` here
makes `target/release/seiza-sync`), copy it to `/usr/local/bin/`, give it a user of its own
(`useradd --system --no-create-home --shell /usr/sbin/nologin seiza`), and run it as a systemd
service:

```ini
# /etc/systemd/system/seiza-sync.service
[Unit]
Description=Seiza sync server
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/seiza-sync serve
Environment=SEIZA_SYNC_DB=/var/lib/seiza-sync/seiza-sync.db
Environment=SEIZA_SYNC_ADDR=0.0.0.0:8080
User=seiza
StateDirectory=seiza-sync
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Then `systemctl enable --now seiza-sync`. Run its commands as the server's user, with the same
database, so any files they make belong to it:

```sh
sudo -u seiza env SEIZA_SYNC_DB=/var/lib/seiza-sync/seiza-sync.db seiza-sync stats
```

To update, build and copy the new version in, then `systemctl restart seiza-sync`.

## Settings

All from the environment:

| Variable | Default | What it does |
|---|---|---|
| `SEIZA_SYNC_ADDR` | `0.0.0.0:8080` | Where it listens. |
| `SEIZA_SYNC_DB` | `seiza-sync.db` (`/data/seiza-sync.db` in Docker) | The database file. |
| `SEIZA_SYNC_NEW_GROUPS` | `first-group-only` | Who can start a sync group: `first-group-only` (the first group, then each needs a code from `seiza-sync invite`), `invites` (each needs a code from `seiza-sync invite`), or `open` (anyone). |
| `SEIZA_SYNC_GROUP_MAX_MB` | `50` | The most each group can store. |
| `SEIZA_SYNC_CLIENT_IP_HEADER` | none | A header from a proxy in front with the visitor's address, e.g. `CF-Connecting-IP`. Only set it when every request comes through that proxy, since anyone could send the header otherwise. |

## Commands

In Docker, run them with `docker compose exec seiza-sync seiza-sync <command>`.

- `seiza-sync stats`: counts groups, devices, groups active in the last 7 and 30 days, and what's
  stored. These come from what the server keeps anyway; the app sends nothing extra.
- `seiza-sync invite`: a one-time code for one new group, valid for 7 days (for
  `SEIZA_SYNC_NEW_GROUPS=invites`, or a new group on a `first-group-only` server after its first).
- `seiza-sync backup <file>`: copies the database to a new file, safely while it runs.
- `seiza-sync health`: checks that the server here answers.

## Backups

Every device keeps its whole log, so losing the server loses nothing: point your devices at a new
one (Settings › Sync › **Move…**) and one of them uploads everything again. You can also back up
the server's database:

```sh
docker compose exec seiza-sync seiza-sync backup /data/backup.db
docker compose cp seiza-sync:/data/backup.db ./seiza-sync-backup.db
```

To restore, stop the server and put the backup in place of `/data/seiza-sync.db`. Then pair your
computers again: **Leave sync…** on each, start a new group on one, and add the others to it with
**Add a device**. Each sends everything it has. The app doesn't notice a restore yet, so without this
your computers would skip what was sent between the backup and the restore, and some of what's
sent after it.

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
- Size caps: 1 MB per batch, 4 KB per pairing or live message, and the group cap above.
- Live connections: one per device (a new one ends its older one), 16 per group and 5,000 in all.
  Each checks every five minutes that its device is still in the group, and ends at once if it's
  removed.

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
| `GET /live` | a device | Its live connection, as server-sent events: `ready`, `batch` `{seq}` (or `{}` if it may have missed some) when another device uploads one, and `message` `{deviceId, data}`. |
| `POST /live` (raw body) | a device | Passes a message to the group's other connected devices. Not kept. |
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
