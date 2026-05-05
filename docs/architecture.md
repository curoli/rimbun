# Architecture

Rimbun is structured as a Rust workspace plus a separate web frontend:

- `rimbun-core`: domain types and business rules
- `rimbun-api`: HTTP API with `axum`
- `rimbun-jobs`: background workers for embeddings and projections
- `rimbun-embedding-client`: local embedding service client
- `web/`: Vue + TypeScript frontend

The first implementation target is a vertical slice covering:

- authentication
- document and section browsing
- draft editing
- publishing
- trivial section projections before semantic clustering is integrated
