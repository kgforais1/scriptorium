#!/usr/bin/env bash
# Build the flipbook spike's mixed-aspect page set in tmp/fixtures/spike-pages/.
# Needs the Dulac and Princess corpora (tests/fixtures/README.md) and macOS `sips`.
# Pages 03 and 07 are derived crops: a landscape fold-out plate and a square plate.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
corpus="$root/tmp/fixtures/corpus"
out="$root/tmp/fixtures/spike-pages"
dulac="$corpus/dulac-picture-book/jpg"
goblin="$corpus/princess-and-goblin/jpg"
for d in "$dulac" "$goblin"; do
  [[ -d "$d" ]] || { echo "missing corpus: $d (see tests/fixtures/README.md)" >&2; exit 1; }
done
rm -rf "$out" && mkdir -p "$out"
cp "$dulac/0011.jpg" "$out/00.jpg"
cp "$dulac/0020.jpg" "$out/01.jpg"
cp "$goblin/0012.jpg" "$out/02.jpg"
sips -s format jpeg --cropToHeightWidth 1900 2808 "$dulac/0039.jpg" --out "$out/03.jpg" >/dev/null
cp "$dulac/0027.jpg" "$out/04.jpg"
cp "$goblin/0020.jpg" "$out/05.jpg"
cp "$dulac/0049.jpg" "$out/06.jpg"
sips -s format jpeg --cropToHeightWidth 2400 2400 "$dulac/0061.jpg" --out "$out/07.jpg" >/dev/null
cp "$goblin/0030.jpg" "$out/08.jpg"
cp "$dulac/0035.jpg" "$out/09.jpg"
for f in "$out"/*.jpg; do
  printf '%s ' "$(basename "$f")"; sips -g pixelWidth -g pixelHeight "$f" | awk 'NR>1{printf "%s ", $2}'; echo
done
# Browser-only dev mode (pnpm dev without Tauri) reads this list; the Tauri build lists the folder itself.
python3 -I -c 'import json,pathlib,sys; print(json.dumps(sorted(p.name for p in pathlib.Path(sys.argv[1]).glob("*.jpg"))))' "$out" > "$out/manifest.json"
