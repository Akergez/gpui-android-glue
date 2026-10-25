#!/usr/bin/env bash
# Test the Java IME/native edit bridge (ImeEdits) without an Android SDK or a
# device: suggestions, autocorrection, composition, deletion and emoji.
set -Eeuo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
classes=$(mktemp -d)
trap 'rm -rf -- "$classes"' EXIT
javac -encoding UTF-8 -d "$classes" \
  "$root/android/src/main/java/dev/gpui/mobile/ImeEdits.java" \
  "$root/android/tests/ImeEditsTest.java"
java -cp "$classes" dev.gpui.mobile.ImeEditsTest
