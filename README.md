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
./rimbunctl dev start [service] [--source]
./rimbunctl dev stop [service]
./rimbunctl dev restart [service] [--source]
./rimbunctl dev status
./rimbunctl dev check
./rimbunctl dev deploy [--dry-run] [--allow-dirty]
./rimbunctl dev releases
./rimbunctl dev rollback <release> [--dry-run]
./rimbunctl dev log [service] [--follow]
./rimbunctl dev list-profiles
./rimbunctl dev backup [name]
./rimbunctl dev verify-backup <backup-file>
./rimbunctl dev restore <backup-file>
./rimbunctl dev list-users
./rimbunctl dev export-contributions <username> [output-file] [--document <slug>] [--section <number-or-uuid>] [--recursive] [--include-empty]
./rimbunctl dev import-contributions <username> <input-file> [--dry-run] [--publish]
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
- immutable release artifacts and records: `.rimbun/dev/releases/`
- atomically selected release: `.rimbun/dev/releases/current`
- logs: `.rimbun/dev/logs/*.log`
- pids: `.rimbun/dev/pids/*.pid`

Examples:

```bash
./rimbunctl dev start
./rimbunctl dev restart backend
./rimbunctl dev status
./rimbunctl dev check
./rimbunctl dev deploy --dry-run
./rimbunctl dev deploy
./rimbunctl dev releases
./rimbunctl dev rollback 20260821-120000-001 --dry-run
./rimbunctl dev rollback 20260821-120000-001
./rimbunctl dev log frontend --follow
./rimbunctl dev log all
./rimbunctl dev list-profiles
./rimbunctl dev backup before-upgrade
./rimbunctl dev verify-backup 20260614-120000-before-upgrade.sql
./rimbunctl dev restore 20260614-120000-before-upgrade.sql
./rimbunctl dev set-role curoli admin
./rimbunctl dev set-password curoli 'new secure password'
```

### AI-assisted contributions

Export a user's current contribution workspace as human-readable TOML, edit selected
`draft_markdown` and optional `draft_main_comment_markdown` values, and import it again:

```bash
./rimbunctl dev export-contributions jati jati.toml \
  --document quran --section 1.2 --recursive --include-empty
./rimbunctl dev import-contributions jati jati.toml --dry-run
./rimbunctl dev import-contributions jati jati.toml
./rimbunctl dev import-contributions jati jati.toml --publish
```

Each entry includes the stable `section_id`, its reader-style `section_number` such as `1.2.3`,
the current `section_title`, and a structured `section_breadcrumb` containing all non-empty
ancestor titles. `--include-empty` also exports sections without an existing contribution by this
user and uses the current main version as the initial draft and conflict-detection reference.
`--document` limits the export to a document slug; `--section` accepts a section number or UUID,
and `--recursive` includes its descendants. Imports only change sections whose `[[entries]]`
blocks remain in the input file. Use `--dry-run` to validate the complete file without writing.
The real import validates all entries before writing them in one transaction. Without `--publish`,
imported contributions remain drafts for review.

`rimbunctl` is implemented as a Rust CLI with a thin launcher script at the repository root.
For `start` and `restart`, it waits until every requested service actually responds before
reporting success. Longer starts print periodic progress updates; the readiness timeout is five
minutes, which also covers backend recompilation in development.
`status` reports each configured process and readiness probe, endpoints, PIDs, log paths,
migration state, projection queue health, the latest backup, and an overall
`healthy`, `degraded`, or `stopped` state.
It exits successfully only when the profile is healthy, so it can also be used in scripts.

For monitoring tools, request a structured report:

```bash
./rimbunctl quran-prod status --json > status.json
```

Both output formats use one collected report and the same classification. JSON
stdout contains exactly one document with `schema_version: 1`, `observed_at` (UTC,
after collection), `profile`, `state_dir`, `overall`, `exit_code`, `services`,
`migrations`, `projection_jobs`, `latest_backup`, and `reasons`. Service entries
include process/readiness state, endpoint, PID and log path. Migration records
include version/success flags and pending versions. The projection report includes
queue counters, oldest age, failed attempts, and effective thresholds. Backup
fields include path, size, modification time and verification state.

Each check has `state`, `healthy`, and fixed `reason_codes`. Top-level `reasons`
identify the `component`, optional `service`, and `code`, e.g.
`projection_age_exceeded`, `migrations_pending`, `database_connect_timeout`, or
`backup_corrupt`. Unknown measurements are `null`, not zero or an empty queue.
Missing backups remain permitted under the existing policy (`state: "missing"`,
`healthy: true`); unreadable backup directories are unknown/unhealthy. Only an
exact `verified` metadata status is accepted as verified.

Exit code 0 means `overall: "healthy"`; `degraded` and `stopped` return 1, and the
JSON `exit_code` matches. Valid `status --json` requests that cannot load their
configuration/profile return one sanitized error document with `overall: "error"`,
`exit_code: 1`, and `error.code: "status_setup_failed"`. Run without `--json` for
configuration diagnostics. Invalid CLI syntax still uses Clap's stderr usage
errors and exit code 2, without a JSON report.

JSON does not export environment variables, connection URLs, raw driver errors,
human diagnostic strings, or free-form queue errors (which could contain sensitive
data). Failing-job section IDs and attempt counts remain available. The text view
keeps its existing queue diagnostic line. Consumers should check `schema_version`
and tolerate additional fields; breaking format changes require a version bump.
Probes are sampled sequentially; separate invocations can naturally differ.

### Webhook Monitoring

`monitor --once` collects the same health report and sends notifications when the
overall state or set of problem codes changes. Configure the **name** of an
environment variable, not the secret webhook URL, in `rimbunctl.toml`:

```toml
[profiles.quran-prod.monitor]
webhook_url_env = "RIMBUN_MONITOR_WEBHOOK_URL"
```

The `monitor` table also works in reusable fragments and follows normal profile
inheritance. Supply the URL through the invoking environment or a protected,
untracked environment file. Profile environment values take precedence over
inherited values, including empty values (which are rejected, not ignored).

```bash
export RIMBUN_MONITOR_WEBHOOK_URL='https://your-receiver.example/rimbun'
./rimbunctl quran-prod monitor --once
```

The first healthy observation establishes a silent baseline. An unhealthy first
observation, a changed problem, or a stopped profile generates a `problem` event;
returning to healthy generates `recovery`. Unchanged problems are not repeated.
Timestamps, PIDs, queue counters and reason ordering do not trigger new events.
Notifications contain `schema_version: 1`, `event_id`, `kind`, and `previous` and
`current` observations with profile, UTC observation time, overall health, and
fixed component/service/problem codes. They exclude URLs, environment values,
filesystem paths, and free-form diagnostics. The receiver must accept this JSON
format; chat services may need an adapter.

Each profile has a locked, atomically saved state file under
`.rimbun/<state_namespace>/monitor/<profile-name-sha256>/state.json` (new directories
are mode 0700 and files 0600). The command prints the exact path. Events are saved
**before** delivery; at most one queued event is attempted per invocation, oldest
first. Failed deliveries remain queued even if the profile has already recovered;
later invocations retry them before delivering recovery. Health and delivery state
are reported separately. Exit code 0 requires healthy services and an empty queue;
unhealthy services, outstanding delivery, or monitor errors return 1.

Only HTTP 2xx acknowledges an event. Requests use a two-second connection timeout
and five-second request timeout, without redirects or environment proxies. Prefer
HTTPS. Delivery is **at least once**: a crash after HTTP success but before saving
the acknowledgement can resend the same event. Receivers should deduplicate using
`event_id` or the matching `Idempotency-Key` header. The webhook URL is never saved
in monitor state or included in delivery errors.

Concurrent runs for the same profile are rejected. Invalid, unsupported or
wrong-profile state is not silently reset. State is limited to 4 MiB; reaching the
limit stops observation/delivery without discarding the existing outbox and
requires operator intervention. Preserve pending events rather than deleting state
to clear an error. Monitoring must be invoked regularly to retry deliveries; it
does not install a scheduler or start services.

For example, after building with `cargo build --release -p rimbunctl`, a user
systemd service can run it from a checkout (adjust paths and profile):

```ini
# ~/.config/systemd/user/rimbun-monitor-quran.service
[Unit]
Description=Check Rimbun Quran profile and deliver alerts

[Service]
Type=oneshot
WorkingDirectory=%h/git/rimbun
EnvironmentFile=%h/.config/rimbun/monitor.env
ExecStart=%h/git/rimbun/target/release/rimbunctl quran-prod monitor --once
TimeoutStartSec=120
```

```ini
# ~/.config/systemd/user/rimbun-monitor-quran.timer
[Unit]
Description=Check Rimbun every minute

[Timer]
OnBootSec=2min
OnUnitActiveSec=1min

[Install]
WantedBy=timers.target
```

Keep `monitor.env` mode 0600, containing `RIMBUN_MONITOR_WEBHOOK_URL=...` and any
required database environment. Enable deliberately with
`systemctl --user daemon-reload` and
`systemctl --user enable --now rimbun-monitor-quran.timer`; inspect output with
`journalctl --user -u rimbun-monitor-quran.service`. A degraded check intentionally
marks the oneshot service failed, but the timer continues running it. User timers
need a running user manager; unattended servers may need an administrator-managed
system unit or lingering. The service deadline also bounds the overall command,
including local filesystem and process checks beyond individual probe deadlines.

### Database and Deployment Checks

Migration and projection-queue checks connect directly to `DATABASE_URL`, for both
local and external PostgreSQL databases; no Docker, `psql`, or GNU `timeout` is
needed for these checks. The profile's `env.DATABASE_URL` takes precedence over
the invoking environment, just as for managed services. All production database
checks run in an isolated CLI helper process with the complete merged environment,
including inherited and profile-specific PostgreSQL variables (`PGPASSWORD`,
`PGSSLMODE`, `PGSSLROOTCERT`, `PGOPTIONS`) and password-file home paths.
SQLx therefore uses the same
credentials, TLS settings and password-file rules as the backend, without changing
the parent process environment. Explicit connection-URL options retain SQLx's
precedence over environment defaults. A missing or invalid URL
is reported as unknown rather than falling back to another database. TLS settings
such as `sslmode=verify-full` and `sslrootcert` can be supplied in the connection URL.
Connections have a two-second deadline; each read-only transaction has a
three-second client deadline and PostgreSQL statement timeout. Connection failures
are also bounded by a six-second parent deadline, including synchronous password-file
and configuration reads before connection setup. Failures
are reported without including the URL, credentials, or database-server messages.
The database role needs `SELECT` access to `_sqlx_migrations` and `projection_jobs`.
`check` performs non-destructive smoke tests for service processes, readiness, migrations, the
latest backup, the frontend, site settings, the document list, and the first visible document.
Set `RIMBUN_CHECK_USERNAME` and `RIMBUN_CHECK_PASSWORD` in the invoking environment to
also test login and `/api/me`; the temporary session is logged out afterward. Failed checks return
exit code 1, while unavailable optional checks are reported as `SKIP`.
`deploy --dry-run` validates and prints the deployment plan without changing services or data.
`deploy` refuses a dirty Git worktree by default, prevents concurrent deployments for the same
profile, creates a verified backup, builds all configured artifacts, stops application services,
runs migrations, starts services with readiness checks, and finally runs `check`. Every completed
or failed phase is recorded under `.rimbun/<state_namespace>/releases/`; inspect the summary with
`releases`. Use `--allow-dirty` only when intentionally testing uncommitted code. The initial
deployment configuration is enabled for local development and test profiles; production profiles
remain disabled until their frontend publishing and release layout are configured.
Each successful deployment archives the Rust binaries and built frontend before atomically moving
the `current` symlink. SHA-256 checksums protect every archived binary and frontend file.
`rollback` verifies these checksums, switches the symlink to an earlier archived release, restarts
it, and runs smoke checks. If those fail, it attempts to restore the previously active application
release. Rollback never changes the database automatically; use the separately verified backup
only after explicitly deciding that an application-only rollback is insufficient. The newest five
artifact releases are retained by default, while the active release is always protected.
After a deployment, ordinary `start` and `restart` commands use the active archived release. Pass
`--source` to intentionally run the development commands from the current worktree instead.
If a deployment fails after application services were stopped, inspect `releases` and the service
logs, repair the reported phase, and run `rimbunctl <PROFILE> start` before retrying the deployment.
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
- `health` defines per-profile warning thresholds for projection queue status
- `state_namespace` decides where `.rimbun/<namespace>/...` runtime state is stored

If no custom file exists, `rimbunctl` provides a built-in `dev` profile assembled from internal fragments. You can add a repository-local `rimbunctl.toml` to define further fragments and profiles.

Projection warning thresholds can be reused in fragments and overridden per profile:

```toml
[fragments.production-health.health]
projection_max_age_seconds = 300
projection_max_failed_attempts = 3

[profiles.quran-prod]
extends = ["server-single-host", "production-runtime", "project_quran", "production-health"]

[profiles.quran-prod.health]
projection_max_age_seconds = 600
```

For existing profiles, add the fragment to their `extends` list and keep the other
profile settings. The project/environment fragments above are defined by this
repository's configuration.
Both settings must be positive integers; zero, negative values, fractions, and
unknown keys inside `health` are rejected when the file is loaded. Omitted settings
inherit independently. Later `extends` entries override earlier entries, then the
profile overrides its parents; an empty `health` table does not erase inherited
values. Defaults (applied only after inheritance) remain 300 seconds and 3 failed
attempts. A job reaching either threshold makes the queue degraded. `status` shows
the effective limits and uses them for both the queue marker and overall status.
These settings only affect status warnings, not worker retry policies or database
connection/query deadlines. Unknown database state still prevents a healthy status.

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

[fragments.project_demo.deployment]
build = ["cargo build --workspace", "npm run build --prefix web"]
migrate = "{release_dir}/bin/rimbun-migrate"
retention = 5

[fragments.project_demo.deployment.artifacts]
backend = "target/debug/rimbun-api"
embedding = "target/debug/rimbun-embedding-service"
migrate = "target/debug/rimbun-migrate"
static = "target/debug/rimbun-static-server"
frontend = "web/dist"

[fragments.project_demo.deployment.run]
backend = "{release_dir}/bin/rimbun-api"
embedding = "{release_dir}/bin/rimbun-embedding-service"
frontend = "{release_dir}/bin/rimbun-static-server {release_dir}/web {frontend_port}"

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

## Projection Recovery

Projection rebuilds are backed by a durable PostgreSQL queue. Submission and
moderation changes queue work in the same transaction; the backend processes it
automatically, including after a restart. Failed jobs retry with exponential
backoff (2 to 256 seconds), and interrupted jobs become available again after a
60-second lease. `rimbunctl <PROFILE> status` shows pending, running, and retrying
jobs for local and external PostgreSQL profiles. It also reports expired leases, the age of
the oldest outstanding job, the maximum consecutive failed attempts, and the last error
for the job with the most failures (including its section ID). Pending includes
running jobs; the other counts are subsets, not additional jobs.

A running profile is `degraded` when a job reaches either configured `health`
threshold (by default five minutes outstanding or three consecutive failures). A fresh queue,
including active processing or one-off retries, does not degrade the profile.
Queue age uses `queued_at`: new submission/moderation changes, worker claims,
retries, and restarts do not reset it. Completing an outdated revision or work from
a worker whose lease has been reclaimed does not clear the age either. Only
successful processing of the current job removes it; later input starts a fresh
waiting interval.
Unknown queue health (including an unavailable database) also prevents a `healthy`
result. Fully stopped profiles still report `stopped`. Migration and queue queries
use the configured `DATABASE_URL` with the read-only deadlines described above.

The existing immediate rebuild remains
in place for responsive publishing; a failed rebuild is repaired asynchronously.

Migration `0018` also queues existing sections with submissions for repair. No
additional service needs to be started. Recovery runs while the backend is running.
Migration `0019` avoids section locks when enqueueing, ignores base-reference-only
updates, and explicitly removes jobs when a section is deleted.
Migration `0020` adds the persistent enqueue timestamp. Existing jobs inherit
`updated_at`, their last known input timestamp; their original
enqueue time before migration cannot be reconstructed.

## Testing

Backend:

```bash
cargo test -p rimbun-api
```

To run the database tests, including crash recovery with a real backend process:

```bash
TEST_DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/rimbun_test \
RIMBUN_REQUIRE_TEST_DATABASE=1 cargo test -p rimbun-api -- --test-threads=1
```

Use a dedicated test database: the API tests reset its public schema. The process
recovery test uses a separate temporary schema and waits for the production
60-second worker lease to expire after killing the backend. It verifies the
persisted contribution, rebuilt main projection, empty queue, and reading API
after restart. CI runs this test automatically.

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
