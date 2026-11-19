#!/bin/bash
# Gera os instaladores Linux x86_64 do Meetily Live Plus a partir de um Mac com
# Apple Silicon, usando Docker (Colima, Docker Desktop ou OrbStack).
#
# Uso (da raiz do repositório):
#   scripts/release/linux.sh                 # saída em entrega/<versão>/
#   scripts/release/linux.sh /caminho/saida  # saída em outro diretório
#   scripts/release/linux.sh --sem-assinar   # não gera o .sig do atualizador
#
# Sai com:
#   Meetily-Live-Plus-linux-x86_64.AppImage (+ .sig, assinatura do atualizador)
#   Meetily-Live-Plus-linux-amd64.deb
#
# Como funciona:
#   1. Compilação cruzada num container arm64 (nativo, rápido) com as
#      bibliotecas amd64 do Ubuntu 22.04. Emular o compilador inteiro em
#      x86_64 levaria horas.
#   2. Empacotamento (`tauri bundle`) num container amd64 emulado, porque o
#      linuxdeploy precisa das bibliotecas x86_64 reais para montar o AppImage.
#   3. Assinatura do AppImage no host. A chave privada do atualizador
#      (~/.config/meetily-live/atualizador.key) nunca entra em container.
#
# As fontes entram no container somente para leitura e são copiadas para um
# volume: nada é escrito na árvore do repositório. Os volumes Docker
# meetily-linux-{fontes,target,cargo,pnpm} guardam o cache entre execuções.
#
# Variáveis:
#   MEETILY_LINUX_ACELERACAO  vulkan (padrão, como o build original) ou openblas (só CPU)
#   MEETILY_CHAVE_ATUALIZADOR caminho da chave privada do atualizador
#   DOCKER_CONTEXT            contexto do Docker a usar (padrão do próprio docker)
set -euo pipefail

RAIZ="$(cd "$(dirname "$0")/../.." && pwd)"
AQUI="$RAIZ/scripts/release/linux"
CONF="$RAIZ/frontend/src-tauri/tauri.conf.json"
ACELERACAO="${MEETILY_LINUX_ACELERACAO:-vulkan}"
CHAVE="${MEETILY_CHAVE_ATUALIZADOR:-$HOME/.config/meetily-live/atualizador.key}"
IMG_COMPILAR="meetily-linux-compilar:local"
IMG_EMPACOTAR="meetily-linux-empacotar:local"

ASSINAR=1; SAIDA=""
for arg in "$@"; do
  case "$arg" in
    --sem-assinar) ASSINAR=0 ;;
    -h|--help) sed -n '2,29p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "Opção desconhecida: $arg" >&2; exit 2 ;;
    *) [ -z "$SAIDA" ] || { echo "Só um diretório de saída é aceito." >&2; exit 2; }; SAIDA="$arg" ;;
  esac
done

passo() { printf '\n==> %s\n' "$*"; }
falha() { printf '\nERRO: %s\n' "$*" >&2; exit 1; }
ler_json() { /usr/bin/python3 -c 'import json,sys; d=json.load(open(sys.argv[1]));
for k in sys.argv[2].split("."): d=d[k]
print(d)' "$1" "$2"; }

passo "Conferindo ferramentas"
for bin in docker git shasum; do
  command -v "$bin" >/dev/null || falha "falta '$bin' no PATH."
done
ARQ="$(docker version --format '{{.Server.Arch}}' 2>/dev/null)" || falha "o Docker não respondeu. Ele está ligado?"
[ "$ARQ" = "arm64" ] || [ "$ARQ" = "aarch64" ] || falha "este script espera um daemon Docker arm64 (achou $ARQ)."

VERSAO="$(ler_json "$CONF" version)"
[ -n "$SAIDA" ] || SAIDA="$RAIZ/entrega/$VERSAO"
mkdir -p "$SAIDA"
SAIDA="$(cd "$SAIDA" && pwd)"
CLI_TAURI="$(grep -A2 "'@tauri-apps/cli':" "$RAIZ/frontend/pnpm-lock.yaml" | awk '/version:/ {print $2; exit}')"
[ -n "$CLI_TAURI" ] || falha "não achei a versão do @tauri-apps/cli em frontend/pnpm-lock.yaml."
echo "Versão $VERSAO, commit $(git -C "$RAIZ" log --oneline -1), aceleração $ACELERACAO, Tauri CLI $CLI_TAURI"
echo "Saída: $SAIDA"

# O Tauri CLI do host assina o AppImage. Procura no próprio checkout e, se
# este for um worktree sem node_modules, no checkout principal.
TAURI_HOST=""
if [ "$ASSINAR" = 1 ]; then
  [ -f "$CHAVE" ] || falha "chave do atualizador não encontrada em $CHAVE (use --sem-assinar para pular)."
  PRINCIPAL="$(git -C "$RAIZ" worktree list --porcelain | awk '/^worktree / {print substr($0, 10); exit}')"
  for dir in "$RAIZ/frontend" "$PRINCIPAL/frontend"; do
    if [ -x "$dir/node_modules/.bin/tauri" ]; then TAURI_HOST="$dir"; break; fi
  done
  [ -n "$TAURI_HOST" ] || falha "não achei o Tauri CLI do host (rode 'pnpm install --frozen-lockfile' em frontend/)."
  command -v pnpm >/dev/null || falha "falta 'pnpm' no PATH para assinar."
fi

INICIO=$(date +%s)

passo "Imagens Docker (a primeira vez demora; depois vem do cache)"
docker build --platform linux/arm64 -t "$IMG_COMPILAR" -f "$AQUI/Dockerfile.compilar" "$AQUI"
docker build --platform linux/amd64 -t "$IMG_EMPACOTAR" -f "$AQUI/Dockerfile.empacotar" \
  --build-arg "TAURI_CLI_VERSION=$CLI_TAURI" "$AQUI"

VOLUMES=(
  -v meetily-linux-fontes:/work
  -v meetily-linux-target:/work/target
  -v meetily-linux-cargo:/cache/cargo
  -v meetily-linux-pnpm:/cache/pnpm
)

passo "Compilando para x86_64 (container arm64, compilação cruzada)"
docker run --rm --platform linux/arm64 "${VOLUMES[@]}" \
  -v "$RAIZ:/repo:ro" -e "ACELERACAO=$ACELERACAO" \
  "$IMG_COMPILAR" bash /repo/scripts/release/linux/compilar.sh

passo "Empacotando deb e AppImage (container amd64 emulado)"
docker run --rm --platform linux/amd64 "${VOLUMES[@]}" \
  -v "$RAIZ/scripts/release/linux:/scripts:ro" -v "$SAIDA:/saida" \
  "$IMG_EMPACOTAR" bash /scripts/empacotar.sh

APPIMAGE="$SAIDA/Meetily-Live-Plus-linux-x86_64.AppImage"
DEB="$SAIDA/Meetily-Live-Plus-linux-amd64.deb"
[ -f "$APPIMAGE" ] && [ -f "$DEB" ] || falha "os arquivos não apareceram em $SAIDA. No Colima, a saída precisa ficar dentro da sua pasta pessoal (só ela é montada na VM)."

if [ "$ASSINAR" = 1 ]; then
  passo "Assinando o AppImage para o atualizador (no host)"
  rm -f "$APPIMAGE.sig"
  # A senha da chave é vazia; a chave vai por caminho, nunca por valor.
  (cd "$TAURI_HOST" && TAURI_SIGNING_PRIVATE_KEY_PATH="$CHAVE" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    pnpm --silent tauri signer sign "$APPIMAGE" >/dev/null)
  [ -s "$APPIMAGE.sig" ] || falha "a assinatura não foi gerada."
  echo "Assinatura: $APPIMAGE.sig"
else
  echo "Sem assinatura do atualizador (--sem-assinar)."
fi

passo "Pronto em $(( ($(date +%s) - INICIO) / 60 )) min"
(cd "$SAIDA" && ls -lh Meetily-Live-Plus-linux-* && shasum -a 256 Meetily-Live-Plus-linux-*)
