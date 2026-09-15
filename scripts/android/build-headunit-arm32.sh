#!/bin/sh
# Dedicated experimental 32-bit entry point. Never publishes or installs an APK.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
sh "$repo/scripts/android/build-headless.sh" headunit-arm32
exec python3 "$repo/scripts/android/build-tech2-app.py" "$@" --profile headunit-arm32
