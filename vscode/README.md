# SourceDigger for VS Code

See the history of the C symbol under your cursor, from a SourceDigger server that has indexed your project's
versions.

## Features

**Hover** a function, define, variable, struct, enum, typedef or exported symbol to see when it was added, in how many
versions it changed, and its definitions and exports now. In Linux:

> **call_rcu** · function, define, invocation
>
> added in 2.6.12 · changed in 24 versions
>
> ```c
> EXPORT_SYMBOL_GPL(call_rcu)
> void call_rcu(struct rcu_head *head, rcu_callback_t func)
> #define call_rcu(x, y)
> ```
>
> History · Compare

**SourceDigger: History** opens a panel with every version the symbol's code changed in: the commits, and the diff of
each file it is defined in, with a link that opens that diff in VS Code's diff editor.

**SourceDigger: Compare Versions** picks a version in which the symbol's code changed, and a file when its code changed in
several, and diffs it with the latest version.

Both commands are in the editor's context menu and the command palette, and the hover links to them.

## Settings

- `sourcedigger.url`: the server, `http://127.0.0.1:2468` by default.
- `sourcedigger.source`: the source this workspace belongs to. When empty, the source whose repository is one of the
  workspace's git remotes is used, and otherwise you are asked once.

## Running a server

See the SourceDigger README for installing the server (there is an RPM for Fedora) and adding a source.
