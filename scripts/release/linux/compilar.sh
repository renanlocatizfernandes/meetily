#!/bin/bash
# Roda DENTRO do container de compilação (chamado por scripts/release/linux.sh).
# Copia as fontes de /repo (somente leitura) para /work, instala as dependências
# do frontend e compila o sidecar llama-helper e o app para x86_64 Linux.
set -euo pipefail

ALVO="x86_64-unknown-linux-gnu"
ACELERACAO="${ACELERACAO:-vulkan}"

passo() { printf '\n==> %s\n' "$*"; }

passo "Copiando as fontes para o volume de trabalho"
rsync -a --delete \
  --exclude '.git' --exclude 'node_modules' --exclude '/target' --exclude '.next' \
  --exclude '/frontend/out' --exclude '/frontend/src-tauri/binaries' \
  --exclude '/entrega' --exclude '/backend' --exclude '/.cache-linux' \
  /repo/ /work/

export CARGO_HOME=/cache/cargo
# O ort-sys guarda o ONNX Runtime baixado no cache do usuário, fora do target;
# sem persistir esse cache, o build seguinte não acha a biblioteca.
export XDG_CACHE_HOME=/work/.cache-linux
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc
export CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc
export CXX_x86_64_unknown_linux_gnu=x86_64-linux-gnu-g++
export AR_x86_64_unknown_linux_gnu=x86_64-linux-gnu-ar
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_LIBDIR_x86_64_unknown_linux_gnu=/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/share/pkgconfig
# Mesmas travas de portabilidade do build original (.github/workflows/build.yml).
# Com --target, o RUSTFLAGS vale só para o código do alvo, não para os build scripts.
export RUSTFLAGS="-C target-cpu=x86-64-v2"
# O arquivo abaixo inclui o .github/force-portable-ggml.cmake e restaura o padrão
# de instruções (AVX2 etc.) que a compilação cruzada desligaria.
export CMAKE_PROJECT_INCLUDE=/work/scripts/release/linux/portavel-cruzado.cmake
# O cpp-httplib empacotado não compila contra OpenSSL 4; o helper não usa HTTPS.
export CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=TRUE
# O whisper.cpp compila o gerador de shaders (vulkan-shaders-gen) para o alvo e
# o executa durante o build. Na compilação cruzada o CMake só faz isso se houver
# um "emulador"; o binfmt/qemu do Docker já roda x86_64, então basta o env.
export CMAKE_CROSSCOMPILING_EMULATOR=/usr/bin/env

RECURSOS=()
case "$ACELERACAO" in
  vulkan)
    export VULKAN_SDK=/opt/vulkan-sdk/x86_64
    export PATH="$VULKAN_SDK/bin:$PATH"
    RECURSOS=(--features vulkan)
    ;;
  openblas) ;;
  *) echo "ACELERACAO desconhecida: $ACELERACAO (use vulkan ou openblas)" >&2; exit 2 ;;
esac

echo "rustc $(rustc --version | awk '{print $2}'), node $(node --version), pnpm $(pnpm --version), $(cmake --version | head -1), aceleração: $ACELERACAO"

passo "Dependências do frontend"
cd /work/frontend
pnpm config set store-dir /cache/pnpm
CI=true pnpm install --frozen-lockfile

passo "Sidecar llama-helper"
cd /work
# No modo CPU o helper sai sem backend de GPU (como no build-linux.yml original).
cargo build --release -p llama-helper --target "$ALVO" ${RECURSOS[@]+"${RECURSOS[@]}"}
mkdir -p frontend/src-tauri/binaries
cp "target/$ALVO/release/llama-helper" "frontend/src-tauri/binaries/llama-helper-$ALVO"

passo "Compilando o app"
cd /work/frontend
RECURSOS_APP=("${RECURSOS[@]+"${RECURSOS[@]}"}")
[ "$ACELERACAO" = openblas ] && RECURSOS_APP=(--features openblas)
# Sem artefatos do atualizador: a chave privada nunca entra no container.
# A assinatura do AppImage é feita no host, depois do empacotamento.
CI=true pnpm tauri build --no-bundle --target "$ALVO" \
  --config '{"bundle":{"createUpdaterArtifacts":false}}' \
  ${RECURSOS_APP[@]+"${RECURSOS_APP[@]}"}

# O empacotamento altera o binário (troca o marcador abaixo pelo tipo de pacote).
# Guarda uma cópia intacta sempre que o cargo religar; o empacotar.sh parte dela.
BINARIO="/work/target/$ALVO/release/meetily"
if grep -q -a '__TAURI_BUNDLE_TYPE_VAR_UNK' "$BINARIO"; then
  cp -p "$BINARIO" "$BINARIO.sem-pacote"
elif [ ! -f "$BINARIO.sem-pacote" ]; then
  echo "ERRO: o binário já foi alterado por um empacotamento e não há cópia intacta. Apague $BINARIO e rode de novo." >&2
  exit 1
fi

passo "Conferindo a portabilidade do código nativo"
cd /work
node .github/verify-portable-ggml.cjs "target/$ALVO/release" --require-llama

ls -la "target/$ALVO/release/meetily" frontend/src-tauri/binaries/
