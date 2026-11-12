---
name: sourcedigger
description: Answer questions about the history of C projects indexed by a SourceDigger server - when a function, define, variable, struct, enum, typedef or exported symbol was added, removed or changed, what its code was in a version, and whether it was exported (EXPORT_SYMBOL, EXPORT_SYMBOL_GPL) in a version. Use when asked how a C symbol evolved, what changed between versions, or for license checks of exported symbols.
---

# SourceDigger

A SourceDigger server indexes the git tags of C projects ("sources") and records, merge by merge, every symbol that
was added, removed or changed. Its JSON API serves everything its web pages show.

## Find the server

Use `$SOURCEDIGGER_URL` if set, else `http://127.0.0.1:2468`. Read `/llms.txt` first: it lists every endpoint with its
parameters, the search syntax, the symbol kinds, the actions and the limitations, and is the reference for anything not
covered here.

```sh
SD=${SOURCEDIGGER_URL:-http://127.0.0.1:2468}
curl -s $SD/llms.txt
curl -s $SD/api/sources | jq '.[] | {name, first_version, last_version}'
```

A web page URL maps to the API: `/sources/git/symbols/strbuf` is `GET /api/sources/git/symbols/strbuf`, and a search
page `/sources/git?q=…` is `GET /api/sources/git/events?q=…` with the same `q`.

## Recipes

A search is names (regular expressions matched whole; `*` is `.*`) and `field:value` terms: `kind:`, `action:`,
`version:`, `after:`, `before:`, `path:` (a regular expression for the start of the path) and `commit:`. A `-` or
`NOT` before a term leaves its matches out, and repeated terms of a field mean any of them:
`kmalloc* -kmalloc_node* path:mm/ path:include/ -path:mm/kasan/`. `AND`, `OR` and parentheses combine terms:
`(kind:function action:added) OR NOT path:mm/`. Let curl encode it with `-G --data-urlencode`. The response's
`query` shows how the search was read. Find names first when unsure:

```sh
curl -s "$SD/api/sources/git/names?q=strbuf_addf"
# ["strbuf_addf","strbuf_addf_ln","strbuf_addf_ret","strbuf_addftime","strbuf_addf_gently"]
```

Every change of one symbol, newest first:

```sh
curl -s $SD/api/sources/git/symbols/strbuf_addf \
  | jq -c '.[] | {version, action, kind, signature: .definition.signature}'
```

Its code in one version, every kind at once (a struct with its fields, a function next to the invocations naming it):

```sh
curl -s "$SD/api/sources/git/symbols/strbuf/code?version=2.45" \
  | jq -r '.definitions[] | "\(.kind) \(.path):\(.line)\n\(.code)"'
```

Whether a symbol is exported, and how, in a version: its `invocation` definitions show the export macro. No invocation
means it is not exported there.

```sh
curl -s "$SD/api/sources/linux/symbols/call_rcu/code?version=2.6.12" \
  | jq -r '.definitions[] | select(.kind == "invocation") | .code'
# EXPORT_SYMBOL(call_rcu);
```

When that changed (here to `EXPORT_SYMBOL_GPL`):

```sh
curl -s -G $SD/api/sources/linux/events --data-urlencode 'q=call_rcu kind:invocation' \
  | jq -c '.events[] | {version, action, now: .definition.signature, before: .previous.signature}'
# … newer versions first, then
# {"version":"2.6.18","action":"signature","now":"EXPORT_SYMBOL_GPL(call_rcu)","before":"EXPORT_SYMBOL_GPL_FUTURE(call_rcu)"}
# {"version":"2.6.17","action":"signature","now":"EXPORT_SYMBOL_GPL_FUTURE(call_rcu)","before":"EXPORT_SYMBOL(call_rcu)"}
# {"version":"2.6.12","action":"added","now":"EXPORT_SYMBOL(call_rcu)","before":null}
```

What changed between two versions, or in one, narrowed by kind, action, path or commit:

```sh
curl -s -G $SD/api/sources/git/events --data-urlencode 'q=kind:struct action:modified version:2.45' \
  | jq -c '.events[] | {name, path: .definition.path, commit: .commit.summary}'
curl -s -G $SD/api/sources/git/events \
  --data-urlencode 'q=strbuf_* action:added,removed after:2.40 before:2.45 path:strbuf.'
```

Actions: `added`, `removed`, `signature` (the declaration line changed) and `modified` (only the code changed: a
body, a struct's fields, or a move to another file).

## Notes

- Results come in pages: pass the `next` value of a page as `cursor=` until `next` is null.
- A bad parameter gives a 400 with `{"error": ...}` saying what is accepted; an unknown source or version gives 404.
- `revisions` gives the code of every version in which it changed, for reading a symbol's evolution in full.
- A symbol defined in more than 25 files needs `path=` on `code` and `revisions`.

## Limitations

Say so when a question needs something the index does not have:

- Only C is read, without a preprocessor: every `#if` branch is indexed, macros are not expanded, and code the parser
  cannot read (an `#if` inside an expression) can hide the indented file-scope definitions after it.
- Function prototypes and `extern` declarations are not indexed: where a function is declared in a header, and how
  that declaration changed, is not available.
- Comments are not indexed: no documentation comments and no `SPDX-License-Identifier` license headers.
- References are not indexed: there is no list of callers or users of a symbol.
- Definitions written through a macro (`SYSCALL_DEFINE3(open, …) { … }`) are not functions; they appear as an
  invocation named after the macro's first identifier argument (`open`).
