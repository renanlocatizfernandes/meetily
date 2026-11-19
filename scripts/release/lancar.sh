#!/usr/bin/env bash
# Compila a versão atual para todas as plataformas e monta o release.
#
#   scripts/release/lancar.sh                 compila tudo e gera o manifesto
#   scripts/release/lancar.sh --so mac        só Mac (Apple Silicon e Intel)
#   scripts/release/lancar.sh --so linux      só Linux
#   scripts/release/lancar.sh --so windows    só Windows
#   scripts/release/lancar.sh --publicar      compila tudo e publica no GitHub
#
# Sem --publicar nada sai da máquina: os arquivos ficam em entrega/<versão>/.
# Uma plataforma que falhar não derruba as outras; o resumo final diz quais
# saíram, e o manifesto só inclui as que têm pacote assinado.
set -uo pipefail
source "$(dirname "$0")/comum.sh"

SO="tudo"; PUBLICAR=0
while [ $# -gt 0 ]; do
  case "$1" in
    --so) SO="${2:-}"; shift 2 ;;
    --publicar) PUBLICAR=1; shift ;;
    -h|--help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) falha "opção desconhecida: $1" ;;
  esac
done
case "$SO" in tudo|mac|linux|windows) ;; *) falha "--so aceita mac, linux ou windows." ;; esac

VERSAO="$(versao_app)"
SAIDA="$(dir_saida)"
LOGS="$SAIDA/logs"; mkdir -p "$LOGS"
AQUI="$(dirname "$0")"
[ -f "$RAIZ/docs/releases/$VERSAO.md" ] || falha "escreva as notas em docs/releases/$VERSAO.md antes de compilar."

declare -a RESUMO=()
roda() {
  local nome="$1"; shift
  passo "$nome (log em $LOGS/$nome.log)"
  local inicio=$SECONDS
  if "$@" > "$LOGS/$nome.log" 2>&1; then
    RESUMO+=("ok     $nome  $(( (SECONDS - inicio) / 60 )) min")
  else
    RESUMO+=("FALHOU $nome  veja $LOGS/$nome.log")
    tail -5 "$LOGS/$nome.log" | sed 's/^/    /'
  fi
}

if [ "$SO" = "tudo" ] || [ "$SO" = "mac" ]; then
  roda mac-apple-silicon "$AQUI/mac.sh" apple-silicon "$SAIDA"
  roda mac-intel "$AQUI/mac.sh" intel "$SAIDA"
fi
if [ "$SO" = "tudo" ] || [ "$SO" = "linux" ]; then
  # O build de Linux roda numa VM Colima própria (6 CPU, 8 GB), parada entre usos.
  if command -v colima >/dev/null && colima list 2>/dev/null | grep -q '^meetily-linux'; then
    colima start -p meetily-linux --activate=false >/dev/null 2>&1 || true
    export DOCKER_CONTEXT=colima-meetily-linux
  fi
  roda linux "$AQUI/linux.sh" "$SAIDA"
  if [ "${DOCKER_CONTEXT:-}" = "colima-meetily-linux" ]; then
    colima stop -p meetily-linux >/dev/null 2>&1 || true
    unset DOCKER_CONTEXT
  fi
fi
if [ "$SO" = "tudo" ] || [ "$SO" = "windows" ]; then
  roda windows "$AQUI/windows.sh" "$SAIDA"
fi

passo "Manifesto do atualizador"
"$AQUI/manifesto.py" "$SAIDA" || falha "não foi possível gerar o manifesto."

passo "Resumo da versão $VERSAO"
printf '  %s\n' "${RESUMO[@]}"
echo "Arquivos em $SAIDA"

if printf '%s\n' "${RESUMO[@]}" | grep -q '^FALHOU'; then
  [ "$PUBLICAR" = 1 ] && echo "Não publiquei: há plataforma com falha."
  exit 1
fi
if [ "$PUBLICAR" = 1 ]; then
  "$AQUI/publicar.sh" "$SAIDA"
else
  echo "Para publicar depois de testar: scripts/release/publicar.sh"
fi
