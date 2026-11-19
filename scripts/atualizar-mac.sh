#!/bin/bash
# Atualiza o Meetily Live Plus neste Mac: puxa a branch, compila, assina,
# gera o .dmg, instala em /Applications e abre o app.
#
# Uso:
#   scripts/atualizar-mac.sh              # pull + build + instalar + abrir
#   scripts/atualizar-mac.sh --sem-pull   # compila o que está no disco
#   scripts/atualizar-mac.sh --nao-abrir  # instala sem abrir
#   scripts/atualizar-mac.sh --forcar     # fecha o app mesmo com gravação em andamento
#
# Assinatura: se existir uma identidade de assinatura, o app sai com ela e o
# macOS mantém as permissões (Microfone, Gravação de Tela) entre atualizações.
# A identidade vem, nesta ordem, de MEETILY_SIGN_ID, do arquivo
# ~/.config/meetily-live/assinatura (uma linha com o nome ou o hash SHA-1 da
# identidade; não é segredo) ou fica ad hoc ("-"), como no build original.
set -euo pipefail

RAIZ="$(cd "$(dirname "$0")/.." && pwd)"
FRONT="$RAIZ/frontend"
CONF="$FRONT/src-tauri/tauri.conf.json"
ALVO="aarch64-apple-darwin"
APP_ID="com.techfernandes.meetilylive"
DADOS="$HOME/Library/Application Support/$APP_ID"
NOMES_ANTIGOS=("Meetily Live")

PULL=1; ABRIR=1; FORCAR=0
for arg in "$@"; do
  case "$arg" in
    --sem-pull) PULL=0 ;;
    --nao-abrir) ABRIR=0 ;;
    --forcar) FORCAR=1 ;;
    -h|--help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Opção desconhecida: $arg" >&2; exit 2 ;;
  esac
done

passo() { printf '\n==> %s\n' "$*"; }
falha() { printf '\nERRO: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "arm64" ] || falha "este script é só para Mac com Apple Silicon."

passo "Conferindo ferramentas"
for bin in git cargo rustc node pnpm cmake hdiutil codesign; do
  command -v "$bin" >/dev/null || falha "falta '$bin' no PATH (veja docs/BUILDING.md)."
done
RUST="$(rustc --version | awk '{print $2}')"
PNPM="$(pnpm --version)"
[ "$RUST" = "1.99.1" ] || falha "Rust $RUST encontrado; o projeto exige 1.99.1 (rustup default 1.99.1)."
[ "$PNPM" = "9.15.9" ] || falha "pnpm $PNPM encontrado; o projeto exige 9.15.9 (npm install -g pnpm@9.15.9)."
echo "rustc $RUST, node $(node --version), pnpm $PNPM, $(cmake --version | head -1)"

NOME="$(/usr/bin/python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["productName"])' "$CONF")"
VERSAO="$(/usr/bin/python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$CONF")"
DESTINO="/Applications/$NOME.app"

cd "$RAIZ"
if [ "$PULL" = 1 ]; then
  passo "Buscando atualizações da branch $(git rev-parse --abbrev-ref HEAD)"
  if git rev-parse --abbrev-ref '@{u}' >/dev/null 2>&1; then
    git pull --ff-only
  else
    echo "A branch não tem remoto configurado; seguindo com o que está no disco."
  fi
fi
echo "Commit: $(git log --oneline -1)"

passo "Sidecar llama-helper"
HELPER="$FRONT/src-tauri/binaries/llama-helper-$ALVO"
MARCA="$RAIZ/target/.llama-helper-rev"
REV="$(git rev-parse HEAD:llama-helper 2>/dev/null || echo desconhecido)-$(git status --porcelain -- llama-helper | shasum | cut -c1-8)"
if [ ! -x "$HELPER" ] || [ "$(cat "$MARCA" 2>/dev/null || true)" != "$REV" ]; then
  # O cpp-httplib empacotado não compila contra OpenSSL 4; o helper não usa HTTPS.
  CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=TRUE cargo build --release -p llama-helper --features metal
  mkdir -p "$(dirname "$HELPER")"
  cp "$RAIZ/target/release/llama-helper" "$HELPER"
  mkdir -p "$(dirname "$MARCA")"; echo "$REV" > "$MARCA"
else
  echo "Sem mudanças; reaproveitando o binário já compilado."
fi

passo "Assinatura"
IDENT="${MEETILY_SIGN_ID:-}"
if [ -z "$IDENT" ] && [ -f "$HOME/.config/meetily-live/assinatura" ]; then
  IDENT="$(head -1 "$HOME/.config/meetily-live/assinatura" | tr -d '[:space:]')"
fi
if [ -n "$IDENT" ] && ! security find-identity -v -p codesigning | grep -qF "$IDENT"; then
  echo "AVISO: a identidade configurada não está no chaveiro; usando assinatura ad hoc."
  IDENT=""
fi
if [ -n "$IDENT" ]; then
  echo "Identidade fixa: $(security find-identity -v -p codesigning | grep -F "$IDENT" | head -1 | sed 's/^ *[0-9]*) [0-9A-F]* //')"
  export APPLE_SIGNING_IDENTITY="$IDENT"
else
  echo "Ad hoc (o macOS pode pedir as permissões de novo a cada atualização)."
  unset APPLE_SIGNING_IDENTITY || true
fi

passo "Compilando o app (leva alguns minutos)"
cd "$FRONT"
pnpm install --frozen-lockfile
# CI=true faz o Tauri pular o AppleScript do Finder ao montar o .dmg, que
# falha quando o terminal não tem permissão de Automação.
CI=true pnpm tauri:build

DMG="$(ls -t "$RAIZ"/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1 || true)"
[ -n "$DMG" ] && [ -f "$DMG" ] || falha "o build terminou sem gerar o .dmg."
echo "Pacote: $DMG"

passo "Fechando o app aberto"
aberto() { pgrep -f "/Applications/.*\.app/Contents/MacOS/meetily" >/dev/null; }
if aberto; then
  if [ -f "$DADOS/recording.active" ] && [ "$FORCAR" = 0 ]; then
    falha "há uma gravação em andamento. Pare a gravação e rode de novo, ou use --forcar."
  fi
  osascript -e "tell application id \"$APP_ID\" to quit" >/dev/null 2>&1 || true
  for _ in $(seq 1 45); do aberto || break; sleep 1; done
  aberto && falha "o app não fechou. Feche manualmente e rode de novo com --sem-pull."
  echo "Fechado."
else
  echo "Não estava aberto."
fi

passo "Instalando em /Applications"
MONTAGEM="$(hdiutil attach "$DMG" -nobrowse -readonly 2>/dev/null | grep -o '/Volumes/.*' | tail -1)"
[ -d "$MONTAGEM/$NOME.app" ] || { hdiutil detach "$MONTAGEM" >/dev/null 2>&1 || true; falha "o .dmg não contém $NOME.app."; }
CARIMBO="$(date +%Y%m%d-%H%M%S)"
for antigo in "$NOME" "${NOMES_ANTIGOS[@]}"; do
  if [ -d "/Applications/$antigo.app" ]; then
    mv "/Applications/$antigo.app" "$HOME/.Trash/$antigo (antes de $CARIMBO).app"
    echo "Versão anterior ($antigo) movida para a Lixeira."
  fi
done
ditto "$MONTAGEM/$NOME.app" "$DESTINO"
hdiutil detach "$MONTAGEM" >/dev/null 2>&1 || true
xattr -dr com.apple.quarantine "$DESTINO" 2>/dev/null || true
codesign --verify --deep --strict "$DESTINO" || falha "a assinatura do app instalado não confere."
echo "Instalado: $DESTINO (versão $VERSAO, $(git -C "$RAIZ" rev-parse --short HEAD))"
codesign -dv "$DESTINO" 2>&1 | grep -E '^(Authority|TeamIdentifier|Signature)=' | head -3 || true

if [ "$ABRIR" = 1 ]; then
  passo "Abrindo"
  open "$DESTINO"
fi
echo
echo "Pronto."
