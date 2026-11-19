# Como lançar uma versão do Meetily Live Plus

Tudo é compilado neste Mac (Apple Silicon), sem GitHub Actions. O release fica no GitHub e é o canal oficial de atualização: o app instalado lê `latest.json` do release mais recente e se atualiza sozinho.

## Passo a passo

```bash
scripts/release/versao.sh 1.0.1        # troca a versão em todos os arquivos
$EDITOR docs/releases/1.0.1.md         # novidades (aparecem no aviso do app)
git commit -am "chore(release): 1.0.1" # e leve o commit para a main do GitHub
scripts/release/lancar.sh              # compila tudo em entrega/1.0.1/
# teste os instaladores
scripts/release/publicar.sh            # cria o release v1.0.1 e confere o canal
```

`scripts/release/lancar.sh --publicar` faz os dois últimos passos de uma vez; use só quando não houver o que testar à mão.

## O que cada script faz

| Script | Função |
| --- | --- |
| `scripts/release/versao.sh X.Y.Z` | Troca a versão em `tauri.conf.json`, `package.json`, `Cargo.toml` e telas; cria o arquivo de notas. |
| `scripts/release/mac.sh apple-silicon\|intel` | `.dmg` e pacote do atualizador para macOS, assinados com a identidade em `~/.config/meetily-live/assinatura`. |
| `scripts/release/linux.sh` | AppImage e `.deb` x86_64, compilados em Docker. |
| `scripts/release/windows.sh` | Instalador NSIS x64, por compilação cruzada. |
| `scripts/release/manifesto.py` | Gera `latest.json` e `SHA256SUMS.txt` a partir do que existe em `entrega/<versão>/`. |
| `scripts/release/lancar.sh` | Roda os quatro builds e o manifesto; `--so mac\|linux\|windows` limita a uma plataforma. |
| `scripts/release/publicar.sh` | Cria o release no GitHub com os arquivos e confere que o canal responde a versão nova. |
| `scripts/atualizar-mac.sh` | Compila e instala neste Mac (uso do dia a dia, não gera release). |

## Arquivos do release

Os nomes não têm versão, para que os links `releases/latest/download/<arquivo>` do site nunca mudem.

| Arquivo | Para quem |
| --- | --- |
| `Meetily-Live-Plus-macos-apple-silicon.dmg` | Mac M1 em diante |
| `Meetily-Live-Plus-macos-intel.dmg` | Mac Intel |
| `Meetily-Live-Plus-windows-x64-setup.exe` | Windows 10/11 64 bits |
| `Meetily-Live-Plus-linux-x86_64.AppImage` | Linux x86_64 |
| `Meetily-Live-Plus-linux-amd64.deb` | Debian/Ubuntu x86_64 |
| `*.app.tar.gz`, `*.sig`, `latest.json` | Usados pelo atualizador |
| `SHA256SUMS.txt` | Conferência dos downloads |

## Canal de atualização

- Endereço consultado pelo app: `https://github.com/renanlocatizfernandes/meetily/releases/latest/download/latest.json` (em `plugins.updater.endpoints` do `tauri.conf.json`).
- O app confere ao abrir e em Configurações → Sobre → Verificar atualizações. Se houver versão maior que a instalada, mostra as notas, baixa, confere a assinatura e reinicia.
- Só entra no manifesto a plataforma que tem pacote e assinatura; as outras continuam na versão anterior até o próximo release.
- Um release marcado como rascunho ou pré-lançamento não é "latest": serve para testar sem atualizar ninguém.

## Chave do atualizador

- Chave privada: `~/.config/meetily-live/atualizador.key` (fora do repositório, permissão 600, sem senha). A pública está em `tauri.conf.json`.
- **Se a chave se perder, os apps já instalados não aceitam mais atualizações** e cada pessoa precisa reinstalar à mão. Guarde uma cópia no Bitwarden.
- Nunca commite, imprima ou envie a chave privada. Os scripts só passam o caminho do arquivo.

## Limites conhecidos

- macOS: assinado com certificado de desenvolvimento, sem notarização; a primeira abertura precisa ser liberada em Privacidade e Segurança. A atualização pelo app não passa por essa barreira.
- Windows: sem assinatura de código; o SmartScreen avisa na instalação.
- Windows e Linux são compilados fora do sistema de destino: teste o instalador numa máquina real antes de publicar quando houver mudança no núcleo.
