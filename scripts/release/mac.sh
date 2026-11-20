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
# Com um certificado "Developer ID Application" da equipe, o app é assinado com
# ele e notarizado pela Apple (some o "Abrir Mesmo Assim"). A notarização usa a
# chave da API do App Store Connect: o .p8 em ~/.config/techfernandes/asc/ e os
# IDs (não secretos) em ~/.config/meetily-live/notarizacao, com as linhas
# APPLE_API_KEY=<id da chave> e APPLE_API_ISSUER=<id do emissor>.
EQUIPE="${MEETILY_EQUIPE_APPLE:-8HMZ5L98YQ}"
DEVID="$(security find-identity -v -p codesigning | sed -n "s/.*\"\(Developer ID Application: .*($EQUIPE)\)\"/\1/p" | head -1)"
NOTARIZA="$HOME/.config/meetily-live/notarizacao"
unset APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH || true
if [ -n "$DEVID" ]; then
  export APPLE_SIGNING_IDENTITY="$DEVID"
  echo "Assinando com o Developer ID da equipe $EQUIPE."
  if [ -f "$NOTARIZA" ]; then
    APPLE_API_KEY="$(sed -n 's/^APPLE_API_KEY=//p' "$NOTARIZA")"
    APPLE_API_ISSUER="$(sed -n 's/^APPLE_API_ISSUER=//p' "$NOTARIZA")"
    APPLE_API_KEY_PATH="$HOME/.config/techfernandes/asc/AuthKey_$APPLE_API_KEY.p8"
    [ -n "$APPLE_API_KEY" ] && [ -n "$APPLE_API_ISSUER" ] && [ -f "$APPLE_API_KEY_PATH" ] \
      || falha "dados de notarização incompletos em $NOTARIZA ou falta o .p8."
    export APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH
    echo "Notarização ligada (o Tauri envia o app à Apple e grampeia o tíquete)."
  else
    echo "Aviso: sem $NOTARIZA o app sai assinado, mas não notarizado."
  fi
elif [ -n "$IDENT" ] && security find-identity -v -p codesigning | grep -qF "$IDENT"; then
  export APPLE_SIGNING_IDENTITY="$IDENT"
  echo "Identidade fixa encontrada (sem Developer ID: não dá para notarizar)."
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
if [ -n "${APPLE_API_KEY:-}" ]; then
  xcrun stapler validate "$APP" || falha "o app não saiu notarizado (tíquete ausente)."
  spctl --assess --type execute "$APP" || falha "o Gatekeeper recusou o app notarizado."
fi
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
