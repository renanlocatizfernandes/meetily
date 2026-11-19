#!/bin/bash
# Fica no lugar do linuxdeploy-plugin-appimage.AppImage que o Tauri baixaria:
# chama a cópia já extraída na imagem, sem passar pelo runtime do AppImage,
# que não funciona sob emulação.
set -euo pipefail
nome="$(basename "$0" .AppImage)"
export APPIMAGE="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
export APPDIR="/opt/appimage/$nome"
args=()
for a in "$@"; do [ "$a" = "--appimage-extract-and-run" ] || args+=("$a"); done
exec "$APPDIR/AppRun" ${args[@]+"${args[@]}"}
