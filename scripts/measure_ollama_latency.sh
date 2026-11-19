#!/usr/bin/env bash
# Compare local Ollama models on the live-assist prompt shape.
# Untested recommendation for an M5 Pro: start with llama3.2:latest, the in-app default.
# If runs 2 and 3 are slower than 3 seconds, compare a smaller instruction model.
# This script only reports what this machine measures. It does not pick a winner in advance.
#
# Usage, from the repository root, with Ollama already running:
#   ./scripts/measure_ollama_latency.sh llama3.2:latest qwen2.5:3b
#
# Each model is called 3 times. The first call is reported separately because
# Ollama may still be loading the weights.

set -euo pipefail

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required" >&2
  exit 1
fi

if [[ $# -lt 1 ]]; then
  echo "Pass one or more local model names, for example: llama3.2:latest" >&2
  exit 1
fi

HOST="${OLLAMA_HOST:-http://127.0.0.1:11434}"
# Same path the app uses for Ollama: OpenAI-compatible /v1/chat/completions.
# reasoning_effort is left out so a server that rejects that field does not add a retry.
PROMPT='Responda APENAS com JSON {"perguntas":[],"respostas":[],"contexto":[]}. No maximo 2 itens por lista e 25 palavras por item. Trecho: [system] Qual e o prazo do relatorio? [mic] Posso confirmar amanha.'

echo "Host: ${HOST}"
echo "Calls per model: 3"
echo "This does not measure the Tauri app, only the Ollama HTTP call."
echo

for model in "$@"; do
  echo "== ${model} =="
  for run in 1 2 3; do
    body=$(python3 -c 'import json,sys; print(json.dumps({"model": sys.argv[1], "messages": [{"role": "user", "content": sys.argv[2]}]}))' "$model" "$PROMPT")
    start=$(python3 -c 'import time; print(time.perf_counter())')
    if ! curl --silent --show-error --max-time 60 \
      -H 'Content-Type: application/json' \
      -d "$body" \
      "${HOST}/v1/chat/completions" \
      -o "/tmp/meetily-live-assist-${run}.json"; then
      echo "run ${run}: request failed"
      continue
    fi
    end=$(python3 -c 'import time; print(time.perf_counter())')
    python3 - "$start" "$end" "$run" "/tmp/meetily-live-assist-${run}.json" <<'PY'
import json, sys
start, end, run, path = sys.argv[1:]
elapsed = float(end) - float(start)
with open(path, "r", encoding="utf-8") as handle:
    payload = json.load(handle)
text = payload.get("choices", [{}])[0].get("message", {}).get("content") or ""
ok = text.strip().startswith("{") and "perguntas" in text
print(f"run {run}: {elapsed:.2f}s valid_json={ok} chars={len(text)}")
PY
  done
  echo
done

echo "Tentative, untested on M5 Pro: prefer the smallest instruction model whose median of runs 2 and 3 is under 3 seconds and whose JSON is valid."
echo "llama3.2:latest is the in-app default because the summary settings already use that tag. A 3B model is the first one to compare against it on the Mac."
