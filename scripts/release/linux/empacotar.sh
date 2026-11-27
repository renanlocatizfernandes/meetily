#!/bin/bash
# Roda DENTRO do container de empacotamento amd64 (chamado por
# scripts/release/linux.sh). Gera o .deb e o AppImage a partir dos binários já
# compilados em /work/target e copia os dois para /saida com os nomes finais.
set -euo pipefail

ALVO="x86_64-unknown-linux-gnu"
PACOTES="/work/target/$ALVO/release/bundle"

passo() { printf '\n==> %s\n' "$*"; }

export CARGO_HOME=/cache/cargo
export XDG_CACHE_HOME=/work/.cache-linux
# Não há FUSE no container; caso algum AppImage ainda seja executado, que se extraia.
export APPIMAGE_EXTRACT_AND_RUN=1
# O plugin de AppImage baixaria o runtime "continuous" do GitHub a cada build
# (e o download falha sob emulação). Usa o runtime fixado que já está na imagem.
export LDAI_RUNTIME_FILE=/opt/appimage/runtime-x86_64

BINARIO="/work/target/$ALVO/release/meetily"
# O bundler grava o tipo de pacote dentro do binário (o atualizador usa isso).
# Parte sempre da cópia intacta guardada pelo compilar.sh e a devolve no fim.
restaurar() { cp -p "$BINARIO.sem-pacote" "$BINARIO"; }
[ -f "$BINARIO.sem-pacote" ] || { echo "ERRO: falta $BINARIO.sem-pacote; rode a compilação antes." >&2; exit 1; }
restaurar
trap restaurar EXIT

# O runtime de um AppImage não roda sob emulação. Põe no lugar do linuxdeploy e
# do plugin de AppImage, onde o Tauri os procura, invólucros que chamam as
# cópias já extraídas na imagem (assim o Tauri também não baixa nada disso).
mkdir -p "$XDG_CACHE_HOME/tauri"
cp /opt/appimage/linuxdeploy-sem-runtime "$XDG_CACHE_HOME/tauri/linuxdeploy-x86_64.AppImage"
cp /opt/appimage/appimage-sem-runtime.sh "$XDG_CACHE_HOME/tauri/linuxdeploy-plugin-appimage.AppImage"
chmod +x "$XDG_CACHE_HOME/tauri/linuxdeploy-x86_64.AppImage" "$XDG_CACHE_HOME/tauri/linuxdeploy-plugin-appimage.AppImage"

passo "Empacotando (deb e AppImage)"
rm -rf "$PACOTES"
cd /work/frontend
# Sempre com --verbose: sem ele o Tauri esconde a saída do linuxdeploy e, sob
# emulação, a chamada falhou de forma intermitente. A saída vai para um
# registro, mostrado só se der erro.
REGISTRO="$XDG_CACHE_HOME/empacotar.log"
if ! CI=true tauri bundle --verbose --bundles deb,appimage --target "$ALVO" \
     --config '{"bundle":{"createUpdaterArtifacts":false}}' > "$REGISTRO" 2>&1; then
  tail -60 "$REGISTRO" >&2
  echo "ERRO: o tauri bundle falhou (registro completo em $REGISTRO, no volume meetily-linux-fontes)." >&2
  exit 1
fi
grep -E 'Bundling|Finished' "$REGISTRO" || true

DEB="$(find "$PACOTES/deb" -maxdepth 1 -name '*.deb' | head -1)"
APPIMAGE="$(find "$PACOTES/appimage" -maxdepth 1 -name '*.AppImage' | head -1)"
[ -f "$DEB" ] || { echo "ERRO: o .deb não foi gerado." >&2; exit 1; }
[ -f "$APPIMAGE" ] || { echo "ERRO: o AppImage não foi gerado." >&2; exit 1; }

passo "Ajustando o AppImage (como no build original)"
# Tira a libwayland-client empacotada (conflita com a do sistema) e troca o
# runtime pelo type2 estático, que não depende da libfuse2 do usuário.
cd "$(dirname "$APPIMAGE")"
rm -rf squashfs-root
extrair-appimage "$APPIMAGE" squashfs-root
find squashfs-root -name 'libwayland-client.so*' -type f -print -delete
ARCH=x86_64 /opt/appimage/appimagetool/AppRun --no-appstream \
  --runtime-file /opt/appimage/runtime-x86_64 squashfs-root "$APPIMAGE" >> "$REGISTRO" 2>&1 \
  || { tail -30 "$REGISTRO" >&2; echo "ERRO: o appimagetool falhou." >&2; exit 1; }
rm -rf squashfs-root

passo "Copiando para a saída"
cp "$APPIMAGE" /saida/Meetily-Live-Plus-linux-x86_64.AppImage
cp "$DEB" /saida/Meetily-Live-Plus-linux-amd64.deb
chmod 755 /saida/Meetily-Live-Plus-linux-x86_64.AppImage
chmod 644 /saida/Meetily-Live-Plus-linux-amd64.deb
dpkg-deb -I /saida/Meetily-Live-Plus-linux-amd64.deb | grep -E '^ (Package|Version|Architecture|Depends):'
ls -la /saida/Meetily-Live-Plus-linux-*
