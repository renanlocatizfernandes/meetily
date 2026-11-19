#!/usr/bin/env bash
# Compila o Meetily Live Plus para macOS e deixa os arquivos de release prontos.
#
#   scripts/release/mac.sh apple-silicon [saida]   Macs com chip Apple (M1 em diante)
#   scripts/release/mac.sh intel [saida]           Macs com processador Intel
#
# Gera em <saida> (padrão entrega/<versão>/):
#   Meetily-Live-Plus-macos-<variante>.dmg            instalador
#   Meetily-Live-Plus-macos-<variante>.app.tar.gz     pacote do atualizador
#   Meetily-Live-Plus-macos-<variante>.app.tar.gz.sig assinatura do atualizador
#
# Não instala nada e não publica nada.
set -euo pipefail
source "$(dirname "$0")/comum.sh"

VARIANTE="${1:-}"
case "$VARIANTE" in
  apple-silicon) ALVO="aarch64-apple-darwin" ;;
  intel)         ALVO="x86_64-apple-darwin" ;;
  *) falha "uso: scripts/release/mac.sh apple-silicon|intel [saida]" ;;
esac
SAIDA="$(dir_saida "${2:-}")"
VERSAO="$(versao_app)"
NOME="$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['productName'])" "$CONF")"

passo "Conferindo ferramentas"
[ "$(uname -s)" = "Darwin" ] || falha "este script só roda no macOS."
for bin in cargo rustc rustup node pnpm cmake hdiutil codesign; do
  command -v "$bin" >/dev/null || falha "falta '$bin' no PATH."
done
exige_chave_atualizador
rustup target list --installed | grep -qx "$ALVO" || rustup target add "$ALVO"
echo "Versão $VERSAO, alvo $ALVO"

passo "Sidecar llama-helper ($ALVO)"
HELPER="$FRONT/src-tauri/binaries/llama-helper-$ALVO"
MARCA="$RAIZ/target/.llama-helper-rev-$ALVO"
REV="$(git -C "$RAIZ" rev-parse HEAD:llama-helper 2>/dev/null || echo desconhecido)-$(git -C "$RAIZ" status --porcelain -- llama-helper | shasum | cut -c1-8)"
if [ ! -x "$HELPER" ] || [ "$(cat "$MARCA" 2>/dev/null || true)" != "$REV" ]; then
  (cd "$RAIZ" && CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=TRUE cargo build --release -p llama-helper --features metal --target "$ALVO")
  mkdir -p "$(dirname "$HELPER")"
  cp "$RAIZ/target/$ALVO/release/llama-helper" "$HELPER"
  echo "$REV" > "$MARCA"
else
  echo "Já está em dia."
fi
lipo -archs "$HELPER"

passo "Assinatura da Apple"
ARQ_IDENT="$HOME/.config/meetily-live/assinatura"
IDENT=""
[ -f "$ARQ_IDENT" ] && IDENT="$(tr -d '\n' < "$ARQ_IDENT")"
if [ -n "$IDENT" ] && security find-identity -v -p codesigning | grep -qF "$IDENT"; then
  export APPLE_SIGNING_IDENTITY="$IDENT"
  echo "Identidade fixa encontrada."
else
  unset APPLE_SIGNING_IDENTITY || true
  echo "Sem identidade fixa: assinatura ad hoc (o macOS pede as permissões de novo a cada versão)."
fi

passo "Compilando o app (leva alguns minutos)"
cd "$FRONT"
pnpm install --frozen-lockfile
export TAURI_SIGNING_PRIVATE_KEY="$CHAVE_ATUALIZADOR"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
# CI=true faz o Tauri pular o AppleScript do Finder ao montar o .dmg.
CI=true pnpm exec tauri build --target "$ALVO" --bundles app,dmg -- --features metal

BUNDLE="$RAIZ/target/$ALVO/release/bundle"
DMG="$(ls -t "$BUNDLE"/dmg/*.dmg 2>/dev/null | head -1 || true)"
TGZ="$BUNDLE/macos/$NOME.app.tar.gz"
[ -f "$DMG" ] || falha "o build terminou sem gerar o .dmg."
[ -f "$TGZ" ] && [ -f "$TGZ.sig" ] || falha "o build terminou sem o pacote assinado do atualizador."

passo "Conferindo"
APP="$BUNDLE/macos/$NOME.app"
codesign --verify --deep --strict "$APP" || falha "a assinatura do app não confere."
ARQS="$(for exe in "$APP/Contents/MacOS/"*; do lipo -archs "$exe"; done | tr '\n' ' ')"
echo "Arquiteturas dos executáveis: $ARQS"
ESPERADA="${ALVO%%-*}"; [ "$ESPERADA" = "aarch64" ] && ESPERADA="arm64"
for a in $ARQS; do [ "$a" = "$ESPERADA" ] || falha "executável com arquitetura $a no pacote $VARIANTE."; done
[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")" = "$VERSAO" ] || falha "versão do app diferente de $VERSAO."

BASE="$SAIDA/$PREFIXO-macos-$VARIANTE"
cp "$DMG" "$BASE.dmg"
cp "$TGZ" "$BASE.app.tar.gz"
cp "$TGZ.sig" "$BASE.app.tar.gz.sig"

passo "Pronto"
for f in "$BASE.dmg" "$BASE.app.tar.gz"; do
  printf '%s  %s  %s\n' "$(sha256_de "$f")" "$(du -h "$f" | cut -f1)" "$(basename "$f")"
done
