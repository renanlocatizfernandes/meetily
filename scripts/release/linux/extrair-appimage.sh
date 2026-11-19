#!/bin/sh
# Uso: extrair-appimage <arquivo.AppImage> <destino>
# Extrai um AppImage sem executá-lo: o runtime do AppImage lê /proc/self/exe,
# o que falha sob emulação (qemu). O squashfs começa logo depois do ELF.
set -eu
inicio="$(python3 -c 'import struct, sys
cabecalho = open(sys.argv[1], "rb").read(64)
tabela = struct.unpack_from("<Q", cabecalho, 0x28)[0]
tamanho, quantidade = struct.unpack_from("<HH", cabecalho, 0x3A)
print(tabela + tamanho * quantidade)' "$1")"
unsquashfs -q -n -f -o "$inicio" -d "$2" "$1" >/dev/null
