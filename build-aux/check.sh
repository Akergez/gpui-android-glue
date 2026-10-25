#!/usr/bin/env bash
# Everything CI checks, in one go: the crate on the desktop and for Android,
# the IME tests, and the Android library. Needs what
# build-aux/build.Containerfile has; see there for running it in that image.
set -Eeuo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"

cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo ndk -t arm64-v8a --platform 26 clippy --locked --all-features -- -D warnings
bash build-aux/ime-tests.sh
(cd android && gradle --no-daemon assembleRelease)
