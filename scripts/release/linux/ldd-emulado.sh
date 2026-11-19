#!/bin/sh
# Fica na frente do ldd do sistema dentro do container amd64 emulado. Sob qemu,
# o ldd de um executável estático (o ffmpeg) quebra com "exit code 139" em vez
# de responder "not a dynamic executable", e o linuxdeploy aborta. Aqui a
# resposta nativa é reproduzida; o resto vai para o ldd de verdade.
for ultimo; do :; done
if [ -f "${ultimo:-}" ] && file -b "$ultimo" | grep -q -E 'statically linked|static-pie linked'; then
  printf '\tnot a dynamic executable\n'
  exit 1
fi
exec /usr/bin/ldd "$@"
