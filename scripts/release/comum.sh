# Funções e variáveis comuns aos scripts de release. Use com `source`.
# Não executa nada sozinho.

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FRONT="$RAIZ/frontend"
CONF="$FRONT/src-tauri/tauri.conf.json"
REPO_GITHUB="renanlocatizfernandes/meetily"
PREFIXO="Meetily-Live-Plus"
CHAVE_ATUALIZADOR="${MEETILY_CHAVE_ATUALIZADOR:-$HOME/.config/meetily-live/atualizador.key}"

passo() { printf '\n==> %s\n' "$*"; }
falha() { printf '\nERRO: %s\n' "$*" >&2; exit 1; }

versao_app() {
  python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['version'])" "$CONF"
}

# Diretório de saída: primeiro argumento ou entrega/<versão>.
dir_saida() {
  local d="${1:-$RAIZ/entrega/$(versao_app)}"
  mkdir -p "$d"
  (cd "$d" && pwd)
}

exige_chave_atualizador() {
  [ -f "$CHAVE_ATUALIZADOR" ] || falha "chave do atualizador não encontrada em $CHAVE_ATUALIZADOR. Sem ela os apps instalados recusam a atualização. Restaure a chave a partir do Bitwarden."
}

# Assina um arquivo para o atualizador do Tauri e grava <arquivo>.sig ao lado.
# A chave nunca é impressa: o CLI recebe só o caminho.
assina_atualizacao() {
  local arquivo="$1"
  exige_chave_atualizador
  (cd "$FRONT" && pnpm exec tauri signer sign -f "$CHAVE_ATUALIZADOR" -p "" "$arquivo" >/dev/null) \
    || falha "não foi possível assinar $arquivo"
  [ -s "$arquivo.sig" ] || falha "assinatura não gerada para $arquivo"
}

sha256_de() { shasum -a 256 "$1" | cut -d' ' -f1; }
