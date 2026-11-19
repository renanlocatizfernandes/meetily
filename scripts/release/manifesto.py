#!/usr/bin/env python3
"""Gera latest.json (manifesto do atualizador) e SHA256SUMS.txt para um release.

    scripts/release/manifesto.py [saida]

Lê os arquivos já gerados em <saida> (padrão entrega/<versão>/) e inclui no
manifesto só as plataformas que têm o pacote e a assinatura (.sig). O app
instalado consulta este arquivo em
https://github.com/<repo>/releases/latest/download/latest.json.
"""
import hashlib
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[2]
REPO = "renanlocatizfernandes/meetily"
PREFIXO = "Meetily-Live-Plus"

# plataforma do atualizador do Tauri -> arquivo que o atualizador baixa
PACOTES = {
    "darwin-aarch64": f"{PREFIXO}-macos-apple-silicon.app.tar.gz",
    "darwin-x86_64": f"{PREFIXO}-macos-intel.app.tar.gz",
    "windows-x86_64": f"{PREFIXO}-windows-x64-setup.exe",
    "linux-x86_64": f"{PREFIXO}-linux-x86_64.AppImage",
}


def main() -> int:
    conf = json.loads((RAIZ / "frontend/src-tauri/tauri.conf.json").read_text())
    versao = conf["version"]
    saida = Path(sys.argv[1]) if len(sys.argv) > 1 else RAIZ / "entrega" / versao
    if not saida.is_dir():
        print(f"ERRO: {saida} não existe", file=sys.stderr)
        return 1

    notas_arq = RAIZ / "docs" / "releases" / f"{versao}.md"
    if not notas_arq.is_file():
        print(f"ERRO: faltam as notas da versão em {notas_arq}", file=sys.stderr)
        return 1
    # O aviso de atualização do app mostra texto puro: tira a marcação Markdown.
    notas = notas_arq.read_text().strip()
    notas = re.sub(r"^#+\s*", "", notas, flags=re.M)
    notas = re.sub(r"\*\*(.+?)\*\*", r"\1", notas)
    notas = notas.replace("`", "")

    base = f"https://github.com/{REPO}/releases/download/v{versao}"
    plataformas = {}
    for plataforma, nome in PACOTES.items():
        pacote, sig = saida / nome, saida / f"{nome}.sig"
        if pacote.is_file() and sig.is_file():
            plataformas[plataforma] = {
                "signature": sig.read_text().strip(),
                "url": f"{base}/{nome}",
            }
        else:
            print(f"aviso: {plataforma} fora do manifesto (falta {nome} ou a assinatura)")
    if not plataformas:
        print("ERRO: nenhum pacote assinado encontrado", file=sys.stderr)
        return 1

    manifesto = {
        "version": versao,
        "notes": notas,
        "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": plataformas,
    }
    (saida / "latest.json").write_text(json.dumps(manifesto, indent=2, ensure_ascii=False) + "\n")

    linhas = []
    for arq in sorted(saida.iterdir()):
        if arq.is_file() and arq.name.startswith(PREFIXO) and not arq.name.endswith(".sig"):
            h = hashlib.sha256()
            with arq.open("rb") as f:
                for bloco in iter(lambda: f.read(1 << 20), b""):
                    h.update(bloco)
            linhas.append(f"{h.hexdigest()}  {arq.name}")
    (saida / "SHA256SUMS.txt").write_text("\n".join(linhas) + "\n")

    print(f"latest.json: versão {versao}, plataformas: {', '.join(plataformas)}")
    print(f"SHA256SUMS.txt: {len(linhas)} arquivos")
    return 0


if __name__ == "__main__":
    sys.exit(main())
