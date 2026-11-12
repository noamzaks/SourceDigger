# SourceDigger Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rust + SQLite + tree-sitter backend and a Mantine SPA that show when C symbols were added, removed or modified across a repo's versions.

**Architecture:** `crates/sourcedigger-index` is a pure library (git versions, changed files, C symbol extraction, symbol diff). `crates/sourcedigger-server` owns the SQLite database, the dig queue and the axum JSON API, and optionally serves `web/dist`. `web/` is a Vite React TypeScript app.

**Tech Stack:** git2, tree-sitter + tree-sitter-c, rayon, regex, rusqlite (bundled, functions), axum 0.8, tokio, tower-http, clap; React, Mantine, React Router, TanStack Query, TanStack Virtual.

**Spec:** `docs/superpowers/specs/2026-11-11-sourcedigger-rewrite-design.md`

## Global Constraints

- Stable Rust, clippy and rustfmt clean; no comments in code; errors propagated with `anyhow` in binaries, never swallowed.
- No system dependencies beyond what `README.md` lists; everything configurable is a CLI flag with a default.
- TypeScript strict, `tsc` and `vite build` pass.
- One script `./test.sh` builds and runs every test.

## Review Focus

- Non-UTF-8 source files: extraction must not fail; signatures are lossy-decoded.
- A tag inserted between already indexed versions: later versions are re-indexed, earlier kept.
- A tag regex that matches nothing or is invalid: the dig fails with a clear error / the form shows it.
- Broad queries (`*`) on a big source: response stays paginated and fast.
- Server restarted during a dig: the dig is marked failed, not left "running" forever.

---

### Task 1: Workspace and C symbol extraction

**Files:** `Cargo.toml`, `crates/sourcedigger-index/{Cargo.toml,src/lib.rs,src/c.rs}`

**Produces:** `pub enum Kind { Function, Define, Variable }` (serde lowercase), `pub struct Symbol { name, kind, line: u32, signature: String }`, `pub fn extract_symbols(code: &[u8]) -> Vec<Symbol>` in `c.rs`.

- [ ] Tests in `c.rs`: function definitions (incl. pointer return, `static`), object-like and function-like defines (with `#ifdef` branches), global variables (initializer dropped, `extern` and prototypes skipped, `int a, b;` gives two), locals ignored, invalid UTF-8 tolerated.
- [ ] Implement with tree-sitter; signature = node text up to body / without initializer, whitespace collapsed, capped at `MAXIMUM_SIGNATURE_LENGTH`.

### Task 2: Symbol diff

**Files:** `crates/sourcedigger-index/src/diff.rs`

**Produces:** `pub struct Definition { path, symbol: Symbol }`, `pub enum Action { Added, Removed, Modified }`, `pub struct Event { action, name, kind, path, line, signature, old: Option<(path, line, signature)> }`, `pub fn diff(removed: Vec<Definition>, added: Vec<Definition>) -> Vec<Event>`.

- [ ] Tests: unchanged-with-line-move gives nothing; signature change → modified; file move → modified with old path; extra definition → added; deterministic order.

### Task 3: Git versions and changed files

**Files:** `crates/sourcedigger-index/src/git.rs`, `tests/git.rs`

**Produces:** `pub struct Version { name, tag, commit: Oid }`, `pub fn fetch(url, path) -> Result<Repository>`, `pub fn versions(repo, tag_pattern: &Regex) -> Result<Vec<Version>>`, `pub fn version_events(repo, previous: Option<&Version>, version: &Version, file_pattern: &Regex) -> Result<Vec<Event>>` (parses in parallel with rayon).

- [ ] Test builds a temp repo with tags `v1`, `v2`, `v10`, `other`; checks natural order, dedup by capture, events between versions, and fetch from a local path.

### Task 4: Database

**Files:** `crates/sourcedigger-server/src/db.rs`

Schema: `sources`, `digs`, `versions(source_id, name, tag, commit, position)`, `names(source_id, name)`, `events(version_id, name_id, kind, action, path, line, signature, old_path, old_line, old_signature)`; WAL; `REGEXP` function.

- [ ] Tests: insert/list sources, prefix truncation of versions deletes their events, regex name search.

### Task 5: Dig worker

**Files:** `crates/sourcedigger-server/src/dig.rs`

- [ ] Queue (digs with status `queued`), one worker thread, scheduler for refresh intervals, stale `running` digs marked failed on startup; progress and per-version log lines written to the dig row.

### Task 6: HTTP API

**Files:** `crates/sourcedigger-server/src/{main.rs,api.rs,query.rs}`, `tests/api.rs`

Endpoints as in the spec (`/api/sources`, `/names`, `/events` with keyset cursor, `/symbols/{name}`, `/digs`, `/preview-tags`).

- [ ] End-to-end test: start the binary on a free port with a temp data dir, add a source pointing at a fixture repo, dig, query events, autocomplete and history; kill the server.

### Task 7: Frontend

**Files:** `web/` (Vite React TS), pages `Home`, `Search`, `Symbol`, `Admin`, `SourceForm`, `Dig`.

- [ ] `npm run build` and `tsc` pass; manual check against a real source (musl) with screenshots.

### Task 8: Real-world check, README, test script

- [ ] Dig musl and git (and Linux if disk/time allow), measure dig and query times, fix slow paths; README with Fedora quick start; `./test.sh`.
