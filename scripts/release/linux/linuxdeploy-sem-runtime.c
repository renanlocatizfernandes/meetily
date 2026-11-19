/*
 * Fica no lugar do linuxdeploy-x86_64.AppImage no cache do Tauri e chama a
 * cópia do linuxdeploy já extraída na imagem (/opt/appimage/linuxdeploy).
 *
 * Por que existe: o runtime de um AppImage lê /proc/self/exe, o que falha sob
 * emulação (qemu). E por que é um ELF, não um shell script: antes de cada
 * execução o Tauri zera os bytes 8 a 10 do arquivo (o "número mágico" do
 * AppImage), o que destruiria a linha "#!" de um script.
 */
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define APPDIR "/opt/appimage/linuxdeploy"

int main(int argc, char **argv) {
    char eu[PATH_MAX];
    char **args = calloc((size_t)argc + 1, sizeof(char *));
    int n = 0;

    if (args == NULL) return 1;
    /* O linuxdeploy procura os plugins na pasta de $APPIMAGE. */
    if (realpath(argv[0], eu) != NULL) setenv("APPIMAGE", eu, 1);
    setenv("APPDIR", APPDIR, 1);

    args[n++] = APPDIR "/AppRun";
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--appimage-extract-and-run") != 0) args[n++] = argv[i];
    }
    execv(args[0], args);
    perror("linuxdeploy-sem-runtime: execv");
    return 127;
}
