# Rimbun

Rimbun is an experimental collaborative writing platform for section-based documents with competing published versions.

It is motivated by a gap in existing tools:

- platforms like Wikipedia are very good at presenting the current consensus version, but they largely hide competing stable alternatives in the edit history
- collaborative editors like Google Docs are very good at drafting and suggesting changes, but they do not treat published alternative versions as first-class readable objects
- Rimbun aims to make these alternatives visible, comparable, and editable without forcing readers to reconstruct them from revision logs

The goal is not only to preserve history, but to surface meaningful variation directly in the reading experience.

The current repository already contains a working MVP slice:

- a Rust backend with authentication, document and section storage, drafts, publishing, moderation, and simple projection logic
- a Vue frontend with reader, compare, section edit, and outline edit views
- multi-user accounts with privileged outline management
- a local Postgres-backed development setup

The long-term product idea is still the same: show a main version of a text, surface meaningful alternatives, and make it easy to compare and edit them. But this repository is no longer just a concept note; it is a runnable application prototype.

## Current MVP

What exists today:

- accounts with username, display name, email, and password
- privileged users can create documents and edit the section outline
- normal users can edit section drafts and publish section versions
- reader view for the whole document
- compare view for section-level main versions and principal alternatives
- section edit view
- outline edit view
- simple multi-account switching in one browser
- automated Rust integration tests and frontend build verification

What is still intentionally incomplete:

- no full production deployment stack in this repository
- no live collaboration
- real embeddings are now supported through a local embedding service, but the compare UI still does not expose richer cluster semantics yet
- compare view is still section-based, not yet a fine-grained diff browser
- markdown is currently shown as plain formatted text in reader/compare views, not yet as a full renderer

## Repository Layout

- `crates/rimbun-api`: Axum HTTP API
- `crates/rimbun-core`: shared domain types and core logic
- `crates/rimbun-embedding-service`: local embedding HTTP service backed by `popsam-core`
- `crates/rimbun-jobs`: background-job-oriented crate for later async processing
- `crates/rimbun-embedding-client`: client crate for the local embedding service
- `web/`: Vue 3 + TypeScript frontend
- `migrations/`: SQL migrations for Postgres

## Development

### Prerequisites

- Rust stable toolchain
- Node.js and npm
- Docker with the Compose plugin

### 1. Start Postgres

```bash
docker compose up -d
```

This starts the local Postgres defined in [docker-compose.yml](./docker-compose.yml).

### 2. Create local environment configuration

```bash
cp .env.example .env
```

The defaults in [.env.example](./.env.example) are enough for local development:

```env
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/rimbun
RIMBUN_PORT=3000
SESSION_SECRET=change-me
EMBEDDING_SERVICE_URL=http://127.0.0.1:8001
```

Both local Rust services load `.env` automatically on startup.

### 3. Run the local embedding service

```bash
cargo run -p rimbun-embedding-service
```

Notes:

- the service listens on `127.0.0.1:8001` by default
- on first startup it may download the default multilingual sentence-transformer model
- the API uses this service to compute and persist real embeddings for published submissions

### 4. Run the backend

```bash
cargo run -p rimbun-api
```

Notes:

- the API listens on `127.0.0.1:3000` by default
- database migrations run automatically on startup

### 5. Run the frontend

```bash
cd web
npm install
npm run dev
```

The Vite dev server proxies `/api` requests to the Rust backend. Open the URL shown by Vite, typically:

```text
http://127.0.0.1:5173
```

### 6. Development service manager

You can also run the local stack through the repository script:

```bash
./rimbunctl dev start
```

Available commands:

```bash
./rimbunctl dev start [service]
./rimbunctl dev stop [service]
./rimbunctl dev restart [service]
./rimbunctl dev status
./rimbunctl dev log [service] [--follow]
./rimbunctl dev list-profiles
./rimbunctl dev backup [name]
./rimbunctl dev verify-backup <backup-file>
./rimbunctl dev restore <backup-file>
./rimbunctl dev set-role <username> <role>
./rimbunctl dev set-password <username> <new-password>
```

Supported services:

- `db`
- `embedding`
- `backend`
- `frontend`
- `all`

Runtime state is written under `.rimbun/dev/`:

- backups: `.rimbun/dev/backups/*.sql`
- backup metadata and checksums: `.rimbun/dev/backups/*.sql.json`
- logs: `.rimbun/dev/logs/*.log`
- pids: `.rimbun/dev/pids/*.pid`

Examples:

```bash
./rimbunctl dev start
./rimbunctl dev restart backend
./rimbunctl dev status
./rimbunctl dev log frontend --follow
./rimbunctl dev log all
./rimbunctl dev list-profiles
./rimbunctl dev backup before-upgrade
./rimbunctl dev verify-backup 20260614-120000-before-upgrade.sql
./rimbunctl dev restore 20260614-120000-before-upgrade.sql
./rimbunctl dev set-role curoli admin
./rimbunctl dev set-password curoli 'new secure password'
```

`rimbunctl` is implemented as a Rust CLI with a thin launcher script at the repository root.
For `start` and `restart`, it waits until every requested service actually responds before
reporting success. Longer starts print periodic progress updates; the readiness timeout is five
minutes, which also covers backend recompilation in development.
`status` reports each configured process and readiness probe, endpoints, PIDs, log paths,
migration state, the latest backup, and an overall `healthy`, `degraded`, or `stopped` state.
It exits successfully only when the profile is healthy, so it can also be used in scripts.
`backup` records a SHA-256 checksum and verifies restorability by loading the dump into a temporary
database. The temporary database is removed afterward. `restore` validates this metadata before
touching the target database. Legacy backups without metadata remain restorable with a warning;
use `verify-backup` to add metadata and verify them first. Restoring a backup created for another
profile requires the explicit `--allow-profile-mismatch` option.

It now supports reusable `fragments` and concrete `profiles`.

- `fragments` define reusable pieces of configuration
- `profiles` define runnable combinations via `extends`
- `vars` provide reusable parameters such as ports, database names, and hostnames
- `env` defines environment variables passed to all commands in the profile
- `state_namespace` decides where `.rimbun/<namespace>/...` runtime state is stored

If no custom file exists, `rimbunctl` provides a built-in `dev` profile assembled from internal fragments. You can add a repository-local `rimbunctl.toml` to define further fragments and profiles.

Example configuration:

```toml
[fragments.local-docker.services.db]
workdir = "."
bootstrap = "docker compose up -d postgres"
run = "docker compose logs -f postgres"
stop = "docker compose stop postgres >/dev/null"

[fragments.local-docker.database]
backup = "docker compose exec -T postgres pg_dump -U postgres -d {db_name} > {file}"
restore = "docker compose exec -T postgres psql -U postgres -d {db_name} < {file}"
verify = "set -e; cleanup() { docker compose exec -T postgres dropdb -U postgres --if-exists {verification_db} >/dev/null; }; trap cleanup EXIT; docker compose exec -T postgres createdb -U postgres {verification_db}; docker compose exec -T postgres psql -U postgres -d {verification_db} -v ON_ERROR_STOP=1 < {file} >/dev/null; test \"$(docker compose exec -T postgres psql -U postgres -d {verification_db} -tAc \"SELECT count(*) FROM pg_class WHERE relkind = 'r' AND relname IN ('users', 'documents', '_sqlx_migrations')\")\" = \"3\""

[fragments.project_demo]
vars.db_name = "rimbun_demo"
vars.backend_port = "3000"
vars.frontend_port = "5173"
vars.embedding_port = "8001"
env.DATABASE_URL = "postgres://postgres:postgres@127.0.0.1:5432/{db_name}"
env.RIMBUN_PORT = "{backend_port}"
env.RIMBUN_EMBEDDING_PORT = "{embedding_port}"
env.EMBEDDING_SERVICE_URL = "http://127.0.0.1:{embedding_port}"

[fragments.project_demo.services.embedding]
workdir = "."
run = "cargo run -p rimbun-embedding-service --bin rimbun-embedding-service"

[fragments.project_demo.services.backend]
workdir = "."
run = "cargo run -p rimbun-api --bin rimbun-api"
depends_on = ["db", "embedding"]

[fragments.project_demo.services.frontend]
workdir = "web"
bootstrap = "test -d node_modules || npm install"
run = "npm run dev -- --host 127.0.0.1 --port {frontend_port} < /dev/null"

[profiles.demo_local]
extends = ["local-docker", "project_demo"]
state_namespace = "demo_local"

[profiles.demo_local_2]
extends = ["local-docker", "project_demo"]
state_namespace = "demo_local_2"
vars.db_name = "rimbun_demo_2"
vars.backend_port = "3001"
vars.frontend_port = "5174"
vars.embedding_port = "8002"
```

The special placeholder `{file}` is replaced by the target backup path. A database verification
command also receives a unique `{verification_db}` name and must remove that temporary database
even when verification fails. For restores, use either a file name relative to
`.rimbun/<state_namespace>/backups/` or an absolute path.

This repository also contains a concrete [rimbunctl.toml](./rimbunctl.toml) with nine predefined project profiles:

- `quran-dev`
- `quran-test`
- `quran-prod`
- `feature-requests-dev`
- `feature-requests-test`
- `feature-requests-prod`
- `benaristan-dev`
- `benaristan-test`
- `benaristan-prod`

These profiles are intended for three separate Rimbun projects:

- Quran translations
- Rimbun feature requests
- Benaristan

Each project has isolated:

- Postgres database name
- backend port
- frontend port
- embedding service port
- `.rimbun/<state_namespace>/...` runtime state

Examples:

```bash
./rimbunctl quran-dev start
./rimbunctl feature-requests-test start
./rimbunctl benaristan-prod start
```

The `dev` and `test` profiles use the normal local development stack. The `prod` profiles are local production-like profiles that run release Rust binaries and serve the built frontend statically, but they are still meant for one-machine operation rather than a full deployment platform.

### 7. First local workflow

After registering the first account, it will be a normal user by default. To test outline editing, promote it manually in Postgres:

```bash
docker compose exec postgres psql -U postgres -d rimbun -c "select username, role from users;"
docker compose exec postgres psql -U postgres -d rimbun -c "update users set role = 'privileged' where username = 'YOUR_USERNAME';"
```

Then:

1. log in as that privileged user
2. create a document
3. create sections in the outline view
4. open a section edit view and publish content
5. use the reader and compare views

## Production

There is not yet a finished production deployment recipe in this repository, but the application can already be run in a simple production-style setup.

For a concrete one-host setup for the three predefined project instances, see:

- [docs/production-single-host.md](./docs/production-single-host.md)
- [ops/systemd/](./ops/systemd)
- [ops/nginx/](./ops/nginx)

### What you need

- a Postgres database
- the local embedding service
- environment variables set explicitly
- the Rust API running as a long-lived process
- the Vue frontend built into static files
- ideally a reverse proxy such as nginx or Caddy in front

### 1. Prepare environment variables

Set real production values, especially:

- `DATABASE_URL`
- `SESSION_SECRET`
- `RIMBUN_PORT`
- `RIMBUN_EMBEDDING_PORT`
- `EMBEDDING_SERVICE_URL`

Example:

```bash
export DATABASE_URL='postgres://USER:PASSWORD@DBHOST:5432/rimbun'
export SESSION_SECRET='replace-with-a-long-random-secret'
export RIMBUN_PORT=3000
export RIMBUN_EMBEDDING_PORT=8001
export EMBEDDING_SERVICE_URL='http://127.0.0.1:8001'
```

### 2. Build and run the embedding service

```bash
cargo build --release -p rimbun-embedding-service
./target/release/rimbun-embedding-service
```

### 3. Build and run the backend

```bash
cargo build --release -p rimbun-api
./target/release/rimbun-api
```

Notes:

- migrations still run automatically on startup
- the API currently serves only the backend, not the frontend assets

### 4. Build the frontend

```bash
cd web
npm install
npm run build
```

This creates static assets in `web/dist/`.

### 5. Serve the frontend

Serve `web/dist/` from a static file server or reverse proxy, and route `/api` to the Rust backend.

A production setup therefore usually looks like this:

- `https://your-domain/` -> static files from `web/dist`
- `https://your-domain/api/...` -> `rimbun-api`
- `http://127.0.0.1:8001/embed` -> local-only embedding service

### Current production caveats

- no Docker production stack is included yet
- no systemd unit files or reverse-proxy config are included yet
- the embedding service must be running if you want real semantic embeddings instead of the runtime fallback
- no horizontal scaling or session-store tuning has been done yet

## Testing

Backend:

```bash
cargo test -p rimbun-api
```

Frontend build check:

```bash
cd web
npm run build
```

Frontend E2E tests:

```bash
cd web
npm run test:e2e
```

## Roadmap Direction

The next major product steps are likely to be:

- richer compare and diff presentation
- proper markdown rendering
- richer cluster-aware compare presentation on top of the existing `popsam`-based ranking
- improved moderation and ranking behavior
- a fuller production deployment story

## Name

Rimbun is an Indonesian word meaning thick when applied to things that grow, such as hair or plants. The name is meant to evoke richness and branching variation.
