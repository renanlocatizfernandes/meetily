#!/bin/bash
# Gera o instalador Windows x64 (NSIS) do Meetily Live Plus num Mac com Apple
# Silicon, por compilação cruzada (cargo-xwin + clang-cl + lld-link), e assina
# o instalador para o atualizador do Tauri.
#
# Uso (a partir da raiz do repositório):
#   scripts/release/windows.sh                 # saída em entrega/<versão>/
#   scripts/release/windows.sh /outro/caminho  # saída no diretório informado
#
# O que sai no diretório de saída:
#   Meetily-Live-Plus-windows-x64-setup.exe      instalador NSIS
#   Meetily-Live-Plus-windows-x64-setup.exe.sig  assinatura do atualizador
#
# Escolhas deste build:
#   - Alvo x86_64-pc-windows-msvc, aceleração só por CPU (sem Vulkan/CUDA).
#   - `-C target-cpu=x86-64-v2` e GGML_NATIVE=OFF, como no workflow build-windows.yml:
#     nada de instruções da máquina de build. Whisper e llama.cpp saem com AVX2
#     (igual ao build oficial do Windows); MEETILY_WIN_SIMD=sse42 gera a variante
#     sem AVX, que roda em CPUs antigas mas transcreve bem mais devagar.
#   - Sem assinatura Authenticode (não há certificado): o SmartScreen avisa.
#   - Só NSIS; MSI depende do WiX, que só roda no Windows.
#
# Variáveis opcionais:
#   MEETILY_WIN_SIMD     avx2 (padrão) ou sse42
#   MEETILY_UPDATER_KEY  caminho da chave privada do atualizador
#                        (padrão ~/.config/meetily-live/atualizador.key, senha vazia)
#   XWIN_CACHE_DIR       onde o cargo-xwin guarda o SDK do Windows
#
# Não usa sudo. Instala o que faltar com Homebrew, rustup e cargo.
set -euo pipefail

RAIZ="$(cd "$(dirname "$0")/../.." && pwd)"
FRONT="$RAIZ/frontend"
TAURI_DIR="$FRONT/src-tauri"
CONF="$TAURI_DIR/tauri.conf.json"
ALVO="x86_64-pc-windows-msvc"
NOME_FINAL="Meetily-Live-Plus-windows-x64-setup.exe"
CHAVE="${MEETILY_UPDATER_KEY:-$HOME/.config/meetily-live/atualizador.key}"

passo() { printf '\n==> %s\n' "$*"; }
falha() { printf '\nERRO: %s\n' "$*" >&2; exit 1; }

case "${1:-}" in
  -h|--help) sed -n '2,29p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
esac

[ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "arm64" ] || falha "este script é só para Mac com Apple Silicon."

VERSAO="$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' "$CONF" | head -1)"
[ -n "$VERSAO" ] || falha "não consegui ler a versão em $CONF."
SAIDA="${1:-$RAIZ/entrega/$VERSAO}"
INICIO="$(date +%s)"

# ---------------------------------------------------------------------------
passo "Conferindo e instalando pré-requisitos (sem sudo)"
for bin in git cargo rustc rustup node pnpm cmake brew xcrun; do
  command -v "$bin" >/dev/null || falha "falta '$bin' no PATH (veja docs/BUILDING.md)."
done

RUST="$(rustc --version | awk '{print $2}')"
PNPM="$(pnpm --version)"
# Mesmo motivo do workflow: o cargo do Rust 1.100 mudou a profundidade do OUT_DIR,
# e tauri-build 2.6.1 / ort-sys 2.0.0-rc.10 ainda assumem a antiga.
[ "$RUST" = "1.99.1" ] || falha "Rust $RUST encontrado; o projeto exige 1.99.1 (rustup default 1.99.1)."
[ "$PNPM" = "9.15.9" ] || falha "pnpm $PNPM encontrado; o projeto exige 9.15.9."

BREW="$(brew --prefix)"
precisa_brew=()
[ -x "$BREW/opt/llvm/bin/clang-cl" ] || precisa_brew+=(llvm)   # clang-cl, llvm-rc, llvm-lib
[ -x "$BREW/opt/lld/bin/lld-link" ] || precisa_brew+=(lld)     # o linker MSVC só existe no Windows
command -v ninja >/dev/null || [ -x "$BREW/bin/ninja" ] || precisa_brew+=(ninja)
export PATH="$BREW/opt/llvm/bin:$BREW/opt/lld/bin:$BREW/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
command -v makensis >/dev/null || precisa_brew+=(makensis)
if [ "${#precisa_brew[@]}" -gt 0 ]; then
  echo "Instalando com Homebrew: ${precisa_brew[*]}"
  HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_ENV_HINTS=1 brew install "${precisa_brew[@]}"
fi
for bin in clang-cl lld-link llvm-rc llvm-lib llvm-readobj ninja makensis; do
  command -v "$bin" >/dev/null || falha "'$bin' não apareceu no PATH depois da instalação."
done

rustup target list --installed | grep -qx "$ALVO" || rustup target add "$ALVO"
cargo xwin --version >/dev/null 2>&1 || cargo install --locked cargo-xwin

[ -f "$CHAVE" ] || falha "chave do atualizador não encontrada em $CHAVE (defina MEETILY_UPDATER_KEY)."

# O bindgen 0.69 usado pelo whisper-rs-sys gera `whisper_full_params` opaco com o
# libclang do LLVM do Homebrew (23.x) para o alvo MSVC; com o libclang do Xcode
# os bindings saem corretos. Por isso o bindgen usa o libclang do Xcode, e o
# LLVM do Homebrew fica só com clang-cl, lld-link e llvm-rc.
LIBCLANG_DIR="$(dirname "$(dirname "$(xcrun --find clang)")")/lib"
[ -f "$LIBCLANG_DIR/libclang.dylib" ] || falha "libclang.dylib do Xcode não encontrado em $LIBCLANG_DIR."

echo "rustc $RUST, node $(node --version), pnpm $PNPM, $(cmake --version | head -1), $(cargo xwin --version)"
echo "$(clang-cl --version | head -1); makensis $(LC_ALL=en_US.UTF-8 makensis -VERSION)"

# ---------------------------------------------------------------------------
# Ambiente comum da compilação cruzada.
export CARGO_TARGET_DIR="$RAIZ/target"
export LIBCLANG_PATH="$LIBCLANG_DIR"
# Não especializar o código para a CPU da máquina de build (igual ao workflow).
export RUSTFLAGS="-C target-cpu=x86-64-v2"
case "${MEETILY_WIN_SIMD:-avx2}" in
  avx2)  export CMAKE_PROJECT_INCLUDE="$RAIZ/scripts/release/windows-portable-avx2.cmake" ;;
  sse42) export CMAKE_PROJECT_INCLUDE="$RAIZ/.github/force-portable-ggml.cmake" ;;
  *) falha "MEETILY_WIN_SIMD deve ser avx2 ou sse42." ;;
esac
# Se a variante mudou desde o último build, descarta os caches do CMake: o
# toolchain do cargo-xwin guarda as flags no cache e o cargo não refaz sozinho.
MARCA="$CARGO_TARGET_DIR/$ALVO/.meetily-simd"
if [ "$(cat "$MARCA" 2>/dev/null || true)" != "${MEETILY_WIN_SIMD:-avx2}" ]; then
  cargo clean --release --target "$ALVO" -p whisper-rs-sys -p llama-cpp-sys-2 >/dev/null 2>&1 || true
  mkdir -p "$(dirname "$MARCA")" && echo "${MEETILY_WIN_SIMD:-avx2}" > "$MARCA"
fi
# O cpp-httplib do llama.cpp não compila com OpenSSL 4; o helper não usa HTTPS.
export CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=TRUE
# O caminho SSE do ggml passa ponteiro de struct para _mm_prefetch; no MSVC é
# aviso, no clang recente virou erro. O cargo-xwin repassa CFLAGS ao clang-cl.
export CFLAGS="-Wno-incompatible-pointer-types${CFLAGS:+ $CFLAGS}"
# O makensis do macOS aborta (std::bad_alloc) sem um locale UTF-8.
export LC_ALL=en_US.UTF-8 LANG=en_US.UTF-8
export CI=true

# ---------------------------------------------------------------------------
passo "Instalando dependências do frontend"
(cd "$FRONT" && pnpm install --frozen-lockfile)

passo "Compilando o sidecar llama-helper para $ALVO (CPU)"
(cd "$RAIZ" && cargo xwin build --release -p llama-helper --target "$ALVO")
mkdir -p "$TAURI_DIR/binaries"
cp "$CARGO_TARGET_DIR/$ALVO/release/llama-helper.exe" "$TAURI_DIR/binaries/llama-helper-$ALVO.exe"

passo "Compilando e empacotando o app (NSIS)"
# O build.rs baixa o ffmpeg e o ONNX Runtime do alvo. O ONNX Runtime vem do
# release oficial da Microsoft no GitHub e é conferido por SHA-256; se o download
# falhar, o build para aqui. Uma pasta frontend/src-tauri/binaries/onnxruntime/
# já preenchida com os arquivos de hash esperado é aceita sem baixar de novo.
CONF_EXTRA="$(mktemp -t meetily-windows-conf).json"
trap 'rm -f "$CONF_EXTRA"' EXIT
# Antes de empacotar, confere que Whisper e llama-helper saíram portáveis
# (GGML_NATIVE e AVX-512 desligados), como faz o workflow original.
cat > "$CONF_EXTRA" <<JSON
{
  "build": {
    "beforeBundleCommand": {
      "script": "node .github/verify-portable-ggml.cjs target/$ALVO/release --require-llama",
      "cwd": "$RAIZ"
    }
  }
}
JSON
rm -rf "$CARGO_TARGET_DIR/$ALVO/release/bundle/nsis"
(
  cd "$FRONT"
  # A chave vai por caminho: o conteúdo nunca passa por argumento nem aparece no log.
  TAURI_SIGNING_PRIVATE_KEY="$CHAVE" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" \
    pnpm tauri build --runner cargo-xwin --target "$ALVO" --bundles nsis --config "$CONF_EXTRA"
)

# ---------------------------------------------------------------------------
passo "Conferindo os artefatos"
PACOTE_DIR="$CARGO_TARGET_DIR/$ALVO/release/bundle/nsis"
INSTALADOR="$(find "$PACOTE_DIR" -maxdepth 1 -name '*-setup.exe' | head -1)"
[ -n "$INSTALADOR" ] && [ -f "$INSTALADOR" ] || falha "instalador não encontrado em $PACOTE_DIR."

if [ ! -f "$INSTALADOR.sig" ]; then
  echo "Assinatura não veio do build; assinando com 'tauri signer sign'."
  (cd "$FRONT" && pnpm tauri signer sign -f "$CHAVE" -p "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" "$INSTALADOR" >/dev/null)
fi
[ -s "$INSTALADOR.sig" ] || falha "assinatura do atualizador não foi gerada."

file "$INSTALADOR" | grep -q 'PE32' || falha "o instalador não é um executável PE."
PRINCIPAL="$CARGO_TARGET_DIR/$ALVO/release/meetily.exe"
file "$PRINCIPAL" | grep -q 'PE32+ executable.*x86-64' || falha "o executável principal não é PE32+ x86-64."

for item in "binaries/llama-helper-$ALVO.exe" "binaries/ffmpeg-$ALVO.exe" \
            "binaries/onnxruntime/onnxruntime.dll" "binaries/onnxruntime/onnxruntime_providers_shared.dll"; do
  [ -s "$TAURI_DIR/$item" ] || falha "falta $item, que deveria ter entrado no instalador."
done

echo "DLLs importadas pelo executável principal:"
llvm-readobj --coff-imports "$PRINCIPAL" | sed -n 's/^ *Name: //p' | sort -u | sed 's/^/  /'

# ---------------------------------------------------------------------------
passo "Copiando para $SAIDA"
mkdir -p "$SAIDA"
cp "$INSTALADOR" "$SAIDA/$NOME_FINAL"
cp "$INSTALADOR.sig" "$SAIDA/$NOME_FINAL.sig"
(cd "$SAIDA" && ls -l "$NOME_FINAL" "$NOME_FINAL.sig" && shasum -a 256 "$NOME_FINAL" "$NOME_FINAL.sig")

FIM="$(date +%s)"
printf '\nPronto em %d min %d s. Versão %s, alvo %s, aceleração CPU (%s).\n' \
  $(( (FIM - INICIO) / 60 )) $(( (FIM - INICIO) % 60 )) "$VERSAO" "$ALVO" "${MEETILY_WIN_SIMD:-avx2}"
echo "Aviso: o app não foi executado; não há como rodar um binário Windows neste Mac."
