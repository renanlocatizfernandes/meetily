#!/usr/bin/env bash
# Troca o número da versão em todos os lugares do projeto.
#
#   scripts/release/versao.sh 1.0.1
#
# Depois: escreva docs/releases/<versão>.md, faça o commit e rode
# scripts/release/lancar.sh.
set -euo pipefail
source "$(dirname "$0")/comum.sh"

NOVA="${1:-}"
[[ "$NOVA" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || falha "uso: scripts/release/versao.sh X.Y.Z"
ATUAL="$(versao_app)"
[ "$NOVA" != "$ATUAL" ] || falha "a versão já é $ATUAL."

python3 - "$RAIZ" "$ATUAL" "$NOVA" <<'PY'
import pathlib, sys
raiz, atual, nova = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
trocas = [
    ("frontend/src-tauri/tauri.conf.json", f'"version": "{atual}"', f'"version": "{nova}"'),
    ("frontend/package.json", f'"version": "{atual}"', f'"version": "{nova}"'),
    ("frontend/src-tauri/Cargo.toml", f'version = "{atual}"', f'version = "{nova}"'),
    ("frontend/src/components/About.tsx", f"'{atual}'", f"'{nova}'"),
    ("frontend/src/components/AnalyticsProvider.tsx", f"'{atual}'", f"'{nova}'"),
    ("frontend/src/components/AnalyticsConsentSwitch.tsx", f"'{atual}'", f"'{nova}'"),
]
for arq, de, para in trocas:
    p = raiz / arq
    t = p.read_text()
    if t.count(de) != 1:
        sys.exit(f"ERRO: esperava uma ocorrência de {de} em {arq}, achei {t.count(de)}")
    p.write_text(t.replace(de, para))
    print(f"  {arq}")
PY
(cd "$RAIZ" && cargo update -q -p meetily --offline 2>/dev/null || true)

NOTAS="$RAIZ/docs/releases/$NOVA.md"
if [ ! -f "$NOTAS" ]; then
  mkdir -p "$(dirname "$NOTAS")"
  printf '## Novidades\n\n- \n' > "$NOTAS"
  echo "Criei $NOTAS: preencha as novidades (aparecem no aviso de atualização do app)."
fi
echo "Versão $ATUAL -> $NOVA."
