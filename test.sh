#!/bin/sh
set -eu

cargo fmt --check
cargo clippy --all-targets -- --deny warnings
cargo test

cd web
npm run check
npm test
npm run build

cd ../vscode
npm run check
npm test
npm run build
