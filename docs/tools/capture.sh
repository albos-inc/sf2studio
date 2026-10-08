#!/bin/sh
# Takes the guide's screenshots again (docs/images/<en|ja>/*.png).
#
# Runs on macOS: the scenes play macOS's own instruments. The app opens in
# front for about a minute and ignores the mouse and keyboard meanwhile.
# With ImageMagick installed the PNGs are then made smaller (at most
# 1920 pixels wide, 256 colors).
set -e
cd "$(dirname "$0")/../.."
cargo run --features capture -- --capture docs/images
if command -v magick >/dev/null 2>&1; then
    for file in docs/images/en/*.png docs/images/ja/*.png; do
        magick "$file" -resize '1920x>' -dither FloydSteinberg -colors 256 "PNG8:$file"
    done
fi
