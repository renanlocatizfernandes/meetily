# Usado por scripts/release/windows.sh via CMAKE_PROJECT_INCLUDE (whisper-rs-sys e
# llama-cpp-sys-2 repassam variáveis CMAKE_* ao CMake).
#
# Faz o mesmo que .github/force-portable-ggml.cmake (nada de especializar o
# código para a CPU da máquina de build) e, além disso, liga AVX/AVX2/FMA/F16C.
# No Windows nativo o ggml já liga essas instruções por padrão ("the x64 baseline
# retains AVX2"); na compilação cruzada o ggml desliga todas (CMAKE_CROSSCOMPILING)
# e a transcrição ficaria só com SSE4.2, bem mais lenta que a do build oficial.
set(GGML_NATIVE OFF CACHE BOOL "Build without host CPU specialization" FORCE)
set(GGML_AVX ON CACHE BOOL "ggml: enable AVX" FORCE)
set(GGML_AVX2 ON CACHE BOOL "ggml: enable AVX2" FORCE)
set(GGML_FMA ON CACHE BOOL "ggml: enable FMA" FORCE)
set(GGML_F16C ON CACHE BOOL "ggml: enable F16C" FORCE)
set(GGML_AVX512 OFF CACHE BOOL "ggml: enable AVX512F" FORCE)
set(GGML_AVX512_VBMI OFF CACHE BOOL "ggml: enable AVX512-VBMI" FORCE)
set(GGML_AVX512_VNNI OFF CACHE BOOL "ggml: enable AVX512-VNNI" FORCE)
set(GGML_AVX512_BF16 OFF CACHE BOOL "ggml: enable AVX512-BF16" FORCE)
