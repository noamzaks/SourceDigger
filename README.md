# SourceDigger

Browse the symbols (functions, defines and variables) of big C projects across their history: for every version,
see which symbols were added, removed or changed, and follow one symbol from the version that introduced it to the one
that removed it.

A rewrite of [SourceDigger](https://github.com/wazzaps/sourcedigger) by David Shlemayev, who created the idea and the
original sourcedigger.io: a Rust server that indexes git tags with tree-sitter into SQLite, and a React + Mantine web
interface.

## Quick start on Fedora

```sh
sudo dnf install rust cargo gcc openssl-devel nodejs npm git
npm ci --prefix web
npm run build --prefix web
cargo run --release -- --web-directory web/dist
```

Open http://127.0.0.1:2468, choose **Add source**, and fill in:

| Source | Repository                                                        | Tag pattern                            |
| ------ | ----------------------------------------------------------------- | -------------------------------------- |
| linux  | `https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git` | `^v(\d+\.\d+(?:\.\d+)?)$`              |
| musl   | `https://git.musl-libc.org/git/musl`                              | `^v(\d+\.\d+\.\d+)$`                   |
| git    | `https://github.com/git/git.git`                                  | `^v(\d+\.\d+)\.0$`                      |
| glibc  | `https://sourceware.org/git/glibc.git`                            | `^glibc-(\d+\.\d+)$`                    |
| vim    | `https://github.com/vim/vim.git`                                  | `^v(\d+\.\d)(?:\.0000)?$`              |

**Preview** shows which tags the pattern turns into versions; the first group of the pattern names the version.
Versions are ordered by their names, so prefer releases from one branch: maintenance tags of older branches (git's
`v2.48.1` after `v2.48.0`) make symbols seem to disappear and come back.
The source viewer is a link template such as `https://elixir.bootlin.com/linux/{tag}/source/{path}#L{line}`.

## Running

```
sourcedigger-server [--data-directory <path>] [--listen <address>] [--web-directory <path>]
```

- `--data-directory` holds the SQLite database and the fetched repositories (default `~/.local/share/sourcedigger`).
- `--listen` defaults to `127.0.0.1:2468`. There is no authentication: anyone who can reach the server can add,
  change and delete sources.
- `--web-directory` serves the built web interface; without it only the JSON API under `/api` is served.
- `RUST_LOG=debug` shows more detailed logs.

A dig fetches the repository's tags, then indexes each version that isn't indexed yet, so digging again after new
releases only indexes the new versions. Sources can also be dug automatically every few hours.

## Searching

The query is a regular expression over the whole symbol name: `kmalloc` finds exactly `kmalloc`, `vm_*` finds every
name starting with `vm_` (a bare `*` or `+` means `.*` or `.+`), and `sys_(open|close)` finds both. It is case
insensitive unless it contains an uppercase letter. An empty query lists the latest changes. Press `/` to focus the
search box and `Ctrl+K` to jump to any source or symbol.

A symbol's page shows its code in every version where it changed, and the diff of each change.

## API

Everything the web interface shows comes from a JSON API, which also suits scripts and agents. `GET /api` lists every
endpoint with its parameters. Errors are `{"error": "..."}` with a 4xx or 5xx status. For example, with a `musl`
source:

```sh
curl -s localhost:2468/api/sources/musl/names?q=memc
# ["memchr","memcmp","memcpy","memccpy"]

curl -s 'localhost:2468/api/sources/musl/events?q=memcpy&actions=modified'
# {"events": [{"version": "0.9.5", "action": "modified", "kind": "function",
#   "definition": {"signature": "void *memcpy(void *restrict dest, const void *restrict src, size_t n)", ...},
#   "previous": {"signature": "void *memcpy(void *dest, const void *src, size_t n)", ...}, ...}], "next": null}

curl -s 'localhost:2468/api/sources/musl/symbols/memcpy/code?version=1.2.0'
# {"version": "1.2.0", "definitions": [{"path": "src/string/memcpy.c", "line": 5, "end_line": 124,
#   "code": "void *memcpy(void *restrict dest, ...", ...}]}

curl -s localhost:2468/api/sources/musl/symbols/memcpy/revisions | jq '[.[].version]'
# ["0.5.0", "0.9.5", "0.9.13"]
```

`events` pages through results with `after=<next>`; `since` and `until` keep the changes between two versions.
`revisions` lists every version in which the code of a symbol changed, including changes to its body that leave the
signature alone. A symbol defined in more than 25 files (Linux has `PAGE_SIZE` in 157) needs `path=` on `code` and
`revisions`. `POST /api/sources/<source>/digs?from=scratch` forgets what was dug and digs every version again.

## Development

```sh
cargo run -- --data-directory /tmp/sourcedigger   # the API on port 2468
npm run dev --prefix web                          # the web interface with hot reload, proxying /api
./test.sh                                         # formatting, lints, type checks, tests and the web build
```

## License

[GNU Affero General Public License v3.0](LICENSE), like the original SourceDigger by David Shlemayev.
