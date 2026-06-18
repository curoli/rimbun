# Single-Host Production Setup

This document describes a pragmatic first production setup for three separate Rimbun projects on one Linux server:

- Quran translations
- Rimbun feature requests
- Benaristan

The model is:

- one host
- one cloned repository
- one Postgres instance
- one `systemd` service pair per project:
  - `rimbun-api@<profile>`
  - `rimbun-embedding@<profile>`
- one nginx server block per project
- static frontend files served directly by nginx

This setup assumes a dedicated Linux service user:

- user: `rimbun`
- group: `rimbun`

## Project Profiles

The repository already contains these production profiles in [rimbunctl.toml](../rimbunctl.toml):

- `quran-prod`
- `feature-requests-prod`
- `benaristan-prod`

Each production profile has its own:

- database name
- backend port
- embedding service port
- runtime state namespace under `.rimbun/<profile>/`

The frontend is not run as a long-lived process in production. Instead:

- `web/dist` is built during deployment
- the built assets are copied into a project-specific site directory
- nginx serves them statically

## Server Packages

Install at least:

```bash
sudo apt update
sudo apt install -y \
  build-essential \
  ca-certificates \
  curl \
  git \
  nginx \
  postgresql \
  python3 \
  pkg-config \
  libssl-dev
```

Also install:

- Rust toolchain
- Node.js and npm

## Suggested Directory Layout

```text
/srv/rimbun/
  repo/
  sites/
    quran-prod/current/
    feature-requests-prod/current/
    benaristan-prod/current/
```

Suggested repository location:

```bash
sudo useradd --system --create-home --home-dir /srv/rimbun --shell /bin/bash rimbun
sudo mkdir -p /srv/rimbun
sudo chown rimbun:rimbun /srv/rimbun
git clone <your-repo-url> /srv/rimbun/repo
sudo chown -R rimbun:rimbun /srv/rimbun/repo
```

## Environment Files

Create one environment file per project under `/etc/rimbun/`.

Examples:

- `/etc/rimbun/quran-prod.env`
- `/etc/rimbun/feature-requests-prod.env`
- `/etc/rimbun/benaristan-prod.env`

Template:

```env
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/PROFILE_DB_NAME
RIMBUN_PORT=PROFILE_BACKEND_PORT
RIMBUN_EMBEDDING_PORT=PROFILE_EMBEDDING_PORT
EMBEDDING_SERVICE_URL=http://127.0.0.1:PROFILE_EMBEDDING_PORT
SESSION_SECRET=replace-this-with-a-long-random-secret
RUST_LOG=info
```

Use the concrete values from the production profiles:

- `quran-prod`
  - DB: `quran_translations_prod`
  - backend: `3002`
  - embedding: `8003`
  - domain: `translations.ropeofgod.org`
- `feature-requests-prod`
  - DB: `rimbun_feature_requests_prod`
  - backend: `3012`
  - embedding: `8013`
  - domain: `docs.rimbun.org`
- `benaristan-prod`
  - DB: `benaristan_prod`
  - backend: `3022`
  - embedding: `8023`
  - domain: `docs.benaristan.org`

## Database Preparation

If Postgres runs locally on the same host, create the databases:

```bash
sudo -u postgres createdb quran_translations_prod
sudo -u postgres createdb rimbun_feature_requests_prod
sudo -u postgres createdb benaristan_prod
```

The production profiles do not auto-create databases. Production DB creation is intentionally an explicit manual step.

## Build and Frontend Publish

From the repository root:

```bash
cargo build --release -p rimbun-api -p rimbun-embedding-service
cd web
npm install
npm run build
```

Then publish the built frontend into the site directories:

```bash
mkdir -p /srv/rimbun/sites/quran-prod/current
mkdir -p /srv/rimbun/sites/feature-requests-prod/current
mkdir -p /srv/rimbun/sites/benaristan-prod/current

rsync -a --delete /srv/rimbun/repo/web/dist/ /srv/rimbun/sites/quran-prod/current/
rsync -a --delete /srv/rimbun/repo/web/dist/ /srv/rimbun/sites/feature-requests-prod/current/
rsync -a --delete /srv/rimbun/repo/web/dist/ /srv/rimbun/sites/benaristan-prod/current/
```

## systemd Units

Install the unit templates from [ops/systemd](../ops/systemd):

```bash
sudo cp ops/systemd/rimbun-api@.service /etc/systemd/system/
sudo cp ops/systemd/rimbun-embedding@.service /etc/systemd/system/
sudo systemctl daemon-reload
```

These unit templates assume:

- repository root: `/srv/rimbun/repo`
- service user: `rimbun`
- environment directory: `/etc/rimbun/`

Enable and start:

```bash
sudo systemctl enable --now rimbun-embedding@quran-prod
sudo systemctl enable --now rimbun-api@quran-prod

sudo systemctl enable --now rimbun-embedding@feature-requests-prod
sudo systemctl enable --now rimbun-api@feature-requests-prod

sudo systemctl enable --now rimbun-embedding@benaristan-prod
sudo systemctl enable --now rimbun-api@benaristan-prod
```

## nginx Configuration

Install the server blocks from [ops/nginx](../ops/nginx):

```bash
sudo cp ops/nginx/*.conf /etc/nginx/sites-available/
sudo ln -s /etc/nginx/sites-available/quran.conf /etc/nginx/sites-enabled/quran.conf
sudo ln -s /etc/nginx/sites-available/feature-requests.conf /etc/nginx/sites-enabled/feature-requests.conf
sudo ln -s /etc/nginx/sites-available/benaristan.conf /etc/nginx/sites-enabled/benaristan.conf
sudo nginx -t
sudo systemctl reload nginx
```

Then add TLS, for example with Let's Encrypt.

## Migrations and Restart During Deploy

On each deploy:

1. Back up the target database
2. Pull the new code
3. Rebuild backend and embedding binaries
4. Rebuild the frontend
5. Re-sync the site directory
6. Restart the affected project services

Example for Quran:

```bash
cd /srv/rimbun/repo
./rimbunctl quran-prod backup before-upgrade
git pull
cargo build --release -p rimbun-api -p rimbun-embedding-service
cd web
npm install
npm run build
rsync -a --delete dist/ /srv/rimbun/sites/quran-prod/current/
cd ..
sudo systemctl restart rimbun-embedding@quran-prod
sudo systemctl restart rimbun-api@quran-prod
```

## Backups

You can continue using `rimbunctl` backups on the server:

```bash
./rimbunctl quran-prod backup
./rimbunctl feature-requests-prod backup nightly
./rimbunctl benaristan-prod restore 20260618-010000.sql
```

For real production use, add a scheduled backup job via cron or `systemd` timers.
