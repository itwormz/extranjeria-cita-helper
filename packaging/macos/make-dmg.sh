#!/bin/bash
# Final macOS disk image: the app, a link to Applications and a LICENSE.txt, on the SantaCita background.
# The visible LICENSE.txt matters: with only the app and the Applications link macOS shows its own
# "Install this app?" sheet, which refuses an app that is not notarized ("could not verify the app").
# Needs the `bundle_dmg.sh` and `icon.icns` that `tauri build --bundles dmg` leaves next to its own image.
# Usage: make-dmg.sh <tauri dmg dir> <SantaCita.app> <LICENSE file> <background.png> <output.dmg>
set -euo pipefail
tool_dir=$1; app=$2; license=$3; background=$4; output=$5
for input in "$tool_dir/bundle_dmg.sh" "$tool_dir/icon.icns" "$app" "$license" "$background"; do
  [[ -e $input ]] || { echo "missing input: $input" >&2; exit 2; }
done
[[ -x $tool_dir/bundle_dmg.sh && -f $tool_dir/icon.icns && -d $app && -f $license && -f $background ]] || { echo "invalid DMG input type or tool permissions" >&2; exit 2; }
# bundle_dmg.sh runs from other directories: every path must be absolute.
abs() { echo "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"; }
tool_dir=$(abs "$tool_dir"); app=$(abs "$app"); license=$(abs "$license"); background=$(abs "$background"); output=$(abs "$output")
staging=$(mktemp -d); trap 'rm -rf "$staging"' EXIT
ditto "$app" "$staging/$(basename "$app")"   # keeps the signature intact
rm -f "$output"
"$tool_dir/bundle_dmg.sh" \
  --volname "SantaCita" --volicon "$tool_dir/icon.icns" \
  --background "$background" --window-size 600 460 --icon-size 110 \
  --icon "$(basename "$app")" 150 190 --hide-extension "$(basename "$app")" \
  --app-drop-link 450 190 \
  --add-file LICENSE.txt "$license" 300 335 \
  "$output" "$staging"
hdiutil verify "$output" >/dev/null
# The image must really contain the three visible items (a missing file silently brings the system sheet back).
mount=$(hdiutil attach -nobrowse -readonly -noautoopen "$output" | awk -F'\t' '/\/Volumes\//{print $NF}')
ok=1; for item in "$(basename "$app")" Applications LICENSE.txt; do [[ -e $mount/$item || -L $mount/$item ]] || { echo "missing in image: $item" >&2; ok=0; }; done
hdiutil detach -quiet "$mount"
[[ $ok == 1 ]]
