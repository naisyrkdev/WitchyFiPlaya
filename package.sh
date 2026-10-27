#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(git -C "$here" rev-parse --show-toplevel)"
version="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$here/Cargo.toml" | head -n1)}"
id="witchy-fi-playa"

cargo build --release -p "$id" --manifest-path "$root/Cargo.toml"

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/bin/linux"
cp "$root/target/release/$id" "$stage/bin/linux/$id"
command -v strip >/dev/null && strip "$stage/bin/linux/$id"
cp "$here/assets/icon.png" "$here/assets/rectangle_cover.png" "$here/assets/square_cover.png" "$stage/"
cp -r "$here/content" "$stage/content"

run_windows=""
if [[ -n "${WINDOWS_EXE:-}" ]]; then
    mkdir -p "$stage/bin/windows"
    cp "$WINDOWS_EXE" "$stage/bin/windows/$id.exe"
    run_windows=",
    \"windows\": { \"command\": \"bin/windows/$id.exe\" }"
fi

commit="$(git -C "$root" rev-parse HEAD)"
branch="$(git -C "$root" rev-parse --abbrev-ref HEAD)"
repo="$(git -C "$root" remote get-url origin 2>/dev/null | sed -e 's/\.git$//' -e 's#^git@\([^:]*\):#https://\1/#' || true)"
tag="$(git -C "$root" describe --tags --exact-match 2>/dev/null || true)"
dirty=false
[[ -n "$(git -C "$root" status --porcelain -- "$here")" ]] && dirty=true

json_or_null() { [[ -n "$1" ]] && printf '"%s"' "$1" || printf 'null'; }

cat > "$stage/yasaffConfig.json" <<EOF
{
  "yasaff": "app",
  "formatVersion": 1,
  "id": "$id",
  "name": "WitchyFiPlaya",
  "version": "$version",
  "kind": "app",
  "author": "Krystian Grzelak",
  "summary": "A simple music player you control with a gamepad",
  "description": "Plays the music files (MP3, FLAC, WAV, OGG, M4A, AIFF and more) in your Music folder (or any folder you pick with the controller), while a little witch dances along (the animations live in the content folder).\n\nA plays a track, X pauses, LB/RB skip, the D-pad seeks, the triggers change the volume, Y shuffles and View repeats. Menu opens the settings (music folder, Xbox or PlayStation button prompts). Xbox and PlayStation controllers both work. Press Guide to go back to YASAFF while the music keeps playing; B quits.",
  "icon": "icon.png",
  "artwork": {
    "cover": "rectangle_cover.png",
    "squareCover": "square_cover.png"
  },
  "run": {
    "linux": { "command": "bin/linux/$id" }$run_windows
  },
  "install": {
    "executables": ["bin/linux/$id"]
  },
  "git": {
    "repository": $(json_or_null "$repo"),
    "branch": $(json_or_null "$branch"),
    "commit": "$commit",
    "tag": $(json_or_null "$tag"),
    "dirty": $dirty
  }
}
EOF

mkdir -p "$here/dist"
out="$here/dist/$id-$version.zip"
rm -f "$out"
(cd "$stage" && zip -qr "$out" .)
echo "$out"
