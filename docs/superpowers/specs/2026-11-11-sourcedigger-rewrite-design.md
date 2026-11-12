# SourceDigger rewrite — design

Rewrite of https://github.com/wazzaps/sourcedigger: browse the symbols (functions, defines, variables) of big C
projects and see in which version each one was added, removed or changed.

## Decided with the user

- Rust backend, Mantine + client-side-rendered React frontend; better looking and faster than the original.
- Storage: SQLite (rusqlite, bundled).
- Symbol extraction: tree-sitter, C only for now.
- No auth: it runs locally. Admin pages and endpoints are open; listen on `127.0.0.1:2468` by default.
- See when a source was last dug, manage sources (add, edit, delete), dig on demand.

## Terms

- **Source**: a configured repo: name, git URL (or local path), tag regex (its first capture group, if any, names
  and deduplicates versions), file regex (default `\.[ch]$`), source-viewer URL template with `{tag}`, `{path}`,
  `{line}`, optional refresh interval in hours.
- **Version**: a tag matching the tag regex. Versions are ordered by natural sort of their name.
- **Dig**: one fetch-and-index run of a source, with its times, status, progress and log.
- **Event**: a symbol `added`, `removed` or `modified` (signature or file changed) in a version, relative to the
  previous version. Line-only moves are not events.

## Behaviour

- A dig fetches a bare mirror into `<data-dir>/repos/`, lists versions, keeps the longest prefix of already indexed
  versions whose names and commits are unchanged, deletes the rest and indexes the remaining versions in order, one
  transaction per version.
- Indexing a version diffs its tree against the previous version's tree, parses the old and new blobs of changed
  files that match the file regex, and matches symbols by (name, kind): exact (path, signature) matches cancel;
  leftovers pair up as `modified` (same path first, then in path order); the rest are `added` / `removed`.
- One dig runs at a time (queue). Sources with a refresh interval are queued when their last dig is older than it.
  Digs left running when the server stopped are marked failed on startup.
- Query syntax as in the original: a regex over the symbol name, `*` and `+` without a preceding atom become `.*` /
  `.+`, anchored, smart case.

## Frontend

- Home: a card per source with version range and "dug N ago" or live progress.
- Search: search bar with autocomplete, filter chips for kind and action, results grouped by version, virtualized,
  each linking to the source viewer and to the symbol's history. State in the URL; `/` focuses the search.
- Symbol page: the symbol's history across versions.
- Admin: list of sources, add/edit form with a live preview of matching tags, "Dig now", dig history with its log.
- Light and dark mode.

## Builder's calls

- Repos are bare mirrors fetched with git2; no `ctags`/`rg` subprocesses.
- The server serves the built SPA from `--web-dir` when given; otherwise only the API.
- No migration of the original's flat-file data.
