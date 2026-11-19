# Usado como CMAKE_PROJECT_INCLUDE na compilação cruzada do whisper.cpp e do
# llama.cpp. Começa pela mesma trava do build original (GGML_NATIVE desligado).
include("${CMAKE_CURRENT_LIST_DIR}/../../../.github/force-portable-ggml.cmake")

# Num build nativo com GGML_NATIVE desligado, o ggml liga por padrão o conjunto
# de instruções abaixo (a "linha de base x64 com AVX2" do build original). Na
# compilação cruzada esse padrão vira tudo desligado, o que deixaria a
# transcrição e os modelos locais bem mais lentos na CPU. Aqui o padrão nativo
# é restaurado; AVX-512 continua desligado.
foreach(_instrucao GGML_SSE42 GGML_AVX GGML_AVX2 GGML_BMI2 GGML_FMA GGML_F16C)
    set(${_instrucao} ON CACHE BOOL "Mesmo padrão do build nativo x86_64" FORCE)
endforeach()
