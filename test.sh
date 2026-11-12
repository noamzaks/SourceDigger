#!/bin/sh
set -eu

cargo fmt --check
cargo clippy --all-targets -- --deny warnings
cargo test

cd web
npm run check
npm run build
