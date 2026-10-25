#!/usr/bin/env bash
# A release is a tag, and the tag is the version: `v1.2.3` may only be put on
# a commit whose crate says 1.2.3. The Android library takes its version from
# the tag itself (android/build.gradle.kts), so the crate is all there is to
# check, and the two halves cannot disagree.
#
#   build-aux/check-version.sh v1.2.3
#
# Prints the bare version.
set -Eeuo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"

tag=${1:?usage: ${0##*/} v<major>.<minor>.<patch>}
if [[ ! $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "'$tag' is not a release tag: expected v<major>.<minor>.<patch>, like v1.0.0" >&2
  exit 1
fi
version=${tag#v}

found=$(cargo metadata --no-deps --locked --format-version 1 \
  | grep -o '"name":"gpui-android-glue","version":"[^"]*"' \
  | sed 's/.*"version":"\([^"]*\)"/\1/')
if [[ $found != "$version" ]]; then
  echo "gpui-android-glue is at $found, but the tag says $version" >&2
  echo "set version in Cargo.toml, run 'cargo update -p gpui-android-glue', commit, and tag that commit" >&2
  exit 1
fi

echo "$version"
