#!/usr/bin/env bash
# Test the Java IME/native edit bridge without an Android SDK or a device.
set -Eeuo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
classes=$(mktemp -d)
trap 'rm -rf -- "$classes"' EXIT
javac -d "$classes" \
  "$root/android/app/src/main/java/dev/gpui/mobile/ImeEdits.java" \
  "$root/android/tests/ImeEditsTest.java"
java -cp "$classes" dev.gpui.mobile.ImeEditsTest
