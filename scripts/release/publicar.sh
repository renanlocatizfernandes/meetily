#!/usr/bin/env bash
# Publica o release da versão atual no GitHub. É o passo que faz os apps
# instalados enxergarem a atualização: só rode depois de testar os pacotes.
#
#   scripts/release/publicar.sh [saida]
#
# Exige: árvore limpa, commit atual já no GitHub, notas em
# docs/releases/<versão>.md e latest.json gerado por manifesto.py.
set -euo pipefail
source "$(dirname "$0")/comum.sh"

SAIDA="$(dir_saida "${1:-}")"
VERSAO="$(versao_app)"
TAG="v$VERSAO"
NOTAS="$RAIZ/docs/releases/$VERSAO.md"

passo "Conferindo"
command -v gh >/dev/null || falha "falta o gh (GitHub CLI)."
[ -f "$NOTAS" ] || falha "faltam as notas em $NOTAS."
[ -f "$SAIDA/latest.json" ] || falha "falta $SAIDA/latest.json (rode scripts/release/manifesto.py)."
[ "$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['version'])" "$SAIDA/latest.json")" = "$VERSAO" ] \
  || falha "latest.json é de outra versão."
cd "$RAIZ"
[ -z "$(git status --porcelain)" ] || falha "há alterações sem commit."
COMMIT="$(git rev-parse HEAD)"
git fetch -q origin
git branch -r --contains "$COMMIT" | grep -q "origin/main" || falha "o commit atual não está na main do GitHub. Faça o merge antes de publicar."
if gh release view "$TAG" -R "$REPO_GITHUB" >/dev/null 2>&1; then
  falha "o release $TAG já existe. Suba a versão (scripts/release/versao.sh) em vez de reescrever um release."
fi

ARQUIVOS=()
while IFS= read -r f; do ARQUIVOS+=("$f"); done < <(find "$SAIDA" -maxdepth 1 -type f \( -name "$PREFIXO-*" -o -name latest.json -o -name SHA256SUMS.txt \) | sort)
echo "Release $TAG no commit ${COMMIT:0:7} com ${#ARQUIVOS[@]} arquivos:"
for f in "${ARQUIVOS[@]}"; do printf '  %s  %s\n' "$(du -h "$f" | cut -f1)" "$(basename "$f")"; done

passo "Publicando"
gh release create "$TAG" -R "$REPO_GITHUB" --target "$COMMIT" \
  --title "Meetily Live Plus $VERSAO" --notes-file "$NOTAS" --latest "${ARQUIVOS[@]}"

passo "Conferindo o canal de atualização"
python3 - "$VERSAO" "$REPO_GITHUB" <<'PY'
import json, sys, urllib.request
versao, repo = sys.argv[1], sys.argv[2]
url = f"https://github.com/{repo}/releases/latest/download/latest.json"
m = json.load(urllib.request.urlopen(url, timeout=30))
assert m["version"] == versao, f"canal responde {m['version']}, esperado {versao}"
print(f"Canal oficial em {versao}: {', '.join(m['platforms'])}")
PY
