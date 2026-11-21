<div align="center">
  <img src="docs/Meetily-6.png" width="520" alt="Meetily Live Plus" />
  <h1>Meetily Live Plus</h1>
  <p><b>Grava, transcreve e resume reuniões no seu computador, com ajuda ao vivo durante a conversa.</b></p>
  <p>
    <a href="https://github.com/renanlocatizfernandes/meetily/releases/latest"><img src="https://img.shields.io/github/v/release/renanlocatizfernandes/meetily?label=vers%C3%A3o&color=brightgreen" alt="Versão" /></a>
    <img src="https://img.shields.io/badge/sistemas-macOS%20%7C%20Windows%20%7C%20Linux-white" alt="Sistemas" />
    <img src="https://img.shields.io/badge/licen%C3%A7a-MIT-blue" alt="Licença MIT" />
  </p>
</div>

> Meetily Live Plus é uma versão modificada por Renan Locatiz Fernandes (Tech Fernandes), baseada no [Meetily](https://github.com/Zackriya-Solutions/meetily), projeto de código aberto da Zackriya Solutions. **Não é a versão oficial do Meetily** e não tem vínculo com a Zackriya Solutions. A licença MIT e o aviso de copyright originais continuam valendo.

## Sumário

- [O que o app faz](#o-que-o-app-faz)
- [Baixar e instalar](#baixar-e-instalar)
- [Primeiros passos](#primeiros-passos)
- [Privacidade](#privacidade)
- [Recursos](#recursos)
- [Atualizações](#atualizações)
- [Para desenvolvedores](#para-desenvolvedores)
- [Histórico de versões](#histórico-de-versões)
- [Licença e créditos](#licença-e-créditos)

## O que o app faz

- **Grava** o microfone e o áudio do sistema (Teams, Zoom, Meet ou qualquer outro app) num arquivo só.
- **Transcreve ao vivo** no próprio computador, com Whisper ou Parakeet, e separa "Você" (microfone) de "Participantes" (áudio do sistema).
- **Ajuda durante a reunião:** perguntas feitas a você, respostas sugeridas, decisões, tarefas, números que divergem de reuniões anteriores e o botão "O que eu perdi?".
- **Resume depois da reunião**, com modelo local (Ollama ou o modelo embutido) ou, se você quiser, com Anthropic, OpenAI, OpenRouter, Groq ou um servidor compatível com OpenAI.
- **Piloto automático (opcional):** ao parar a gravação, gera o resumo, as tarefas com responsável e prazo, as decisões, um título, etiquetas e um rascunho de e-mail de acompanhamento. O e-mail nunca é enviado.
- **Pergunte às suas reuniões:** um chat sobre todas as gravações, com citação da reunião e do trecho de cada resposta.

Tudo o que usa IA na nuvem vem desligado. Sem nenhuma chave configurada, o app funciona só com os modelos locais.

<p align="center">
  <img src="docs/home.png" width="650" alt="Tela inicial com a transcrição ao vivo" />
</p>

## Baixar e instalar

Os instaladores ficam na página do [release mais recente](https://github.com/renanlocatizfernandes/meetily/releases/latest). Os links abaixo sempre apontam para a última versão.

| Sistema | Arquivo | Como instalar |
|---|---|---|
| **Mac com Apple Silicon** (M1 em diante) | [Meetily-Live-Plus-macos-apple-silicon.dmg](https://github.com/renanlocatizfernandes/meetily/releases/latest/download/Meetily-Live-Plus-macos-apple-silicon.dmg) | Abra o .dmg e arraste o app para Aplicativos. |
| **Mac com Intel** | [Meetily-Live-Plus-macos-intel.dmg](https://github.com/renanlocatizfernandes/meetily/releases/latest/download/Meetily-Live-Plus-macos-intel.dmg) | Igual ao anterior. |
| **Windows 10/11 x64** | [Meetily-Live-Plus-windows-x64-setup.exe](https://github.com/renanlocatizfernandes/meetily/releases/latest/download/Meetily-Live-Plus-windows-x64-setup.exe) | Rode o instalador. No aviso do SmartScreen, escolha Mais informações → Executar assim mesmo. |
| **Linux x86_64 (Debian/Ubuntu)** | [Meetily-Live-Plus-linux-amd64.deb](https://github.com/renanlocatizfernandes/meetily/releases/latest/download/Meetily-Live-Plus-linux-amd64.deb) | `sudo apt install ./Meetily-Live-Plus-linux-amd64.deb` |
| **Linux x86_64 (outras)** | [Meetily-Live-Plus-linux-x86_64.AppImage](https://github.com/renanlocatizfernandes/meetily/releases/latest/download/Meetily-Live-Plus-linux-x86_64.AppImage) | `chmod +x` no arquivo e execute. |

Observações:

- **Mac:** o app é assinado e notarizado pela Apple e abre sem aviso de segurança. Ele se chama "Meetily Live Plus" e pode ficar instalado ao lado do Meetily original, sem conflito.
- **Windows:** o instalador não tem assinatura de código, por isso o SmartScreen avisa. Ele usa só a CPU (precisa de AVX2), sem Vulkan nem CUDA.
- **Conferência:** cada release traz `SHA256SUMS.txt` com o hash de todos os arquivos.

## Primeiros passos

1. Abra o app e conceda as permissões de **Microfone** e de **Gravação de Tela e Áudio do Sistema** (no Mac, é o que permite capturar o som da reunião).
2. Escolha o modelo de transcrição na primeira abertura. O download é feito uma vez só.
3. Para resumos locais, instale o [Ollama](https://ollama.com/) e baixe um modelo, por exemplo:

   ```bash
   ollama pull llama3.2
   ```

   Ou use o modelo embutido do app, ou configure um provedor na nuvem em Configurações → Resumo.
4. Escolha o microfone e o dispositivo de áudio do sistema e clique em gravar.
5. Durante a reunião, abra a **Ajuda ao vivo** com Ctrl+Shift+L (⌘+Shift+L no Mac). Ela também abre numa janela sempre visível, ao lado da chamada.
6. Ao parar, o resumo é gerado (ou o piloto automático, se estiver ligado).

## Privacidade

- Áudio, transcrições e resumos ficam no seu computador.
- O texto da transcrição só sai da máquina se você escolher um provedor na nuvem para o resumo ou para a ajuda ao vivo. A tela avisa quando isso acontece.
- Se o provedor principal roda local, o app nunca usa um provedor reserva na nuvem.
- As chaves de API ficam nas configurações locais do app e não aparecem nos registros.
- Nenhum serviço pago vem configurado e nenhuma chave vem incluída no app.

<p align="center">
  <img src="docs/settings.png" width="650" alt="Configurações de transcrição e armazenamento" />
</p>

## Recursos

### Transcrição

- Whisper ou Parakeet, rodando no computador. No Mac, com Metal e CoreML.
- Detectores de voz separados para o microfone e para o sistema, com rótulos "Você" e "Participantes".
- Importação de arquivos de áudio e nova transcrição de uma reunião gravada com outro modelo ou idioma.
- Recuperação de transcrições de gravações interrompidas.

### Resumo

- O app reconhece o tipo de reunião (daily, 1:1, vendas, entrevista, aula, consulta e outros) e escolhe o modelo de resumo; você pode trocar.
- Modelos de resumo em português e editor do resumo dentro do app.
- Sugestão do nome de cada falante; nada muda até você confirmar.
- Provedores: Ollama, modelo embutido, Anthropic, OpenAI, OpenRouter, Groq e servidor compatível com OpenAI. Cada provedor tem "Testar conexão", que não gasta créditos, e lista de provedores reserva.

<p align="center">
  <img src="docs/summary.png" width="650" alt="Resumo da reunião" />
</p>

### Ajuda ao vivo

- Perguntas, respostas e contexto atualizados enquanto a reunião acontece, com copiar, fixar e descartar.
- Alertas de pergunta para você, decisão, tarefa e divergência com reuniões anteriores.
- "O que eu perdi?" resume os últimos minutos.
- Sugestão de resposta, modo economia e consumo estimado.

### Depois da reunião

- Piloto automático com resumo, tarefas, decisões, título, etiquetas e rascunho de e-mail, cada etapa com seu interruptor.
- Busca e chat sobre todas as reuniões, feitos no computador, com citações.
- Gravação automática quando Teams, Zoom ou Meet iniciam uma chamada (opcional).

## Atualizações

O app confere sozinho se há versão nova, avisa e instala. Para conferir na hora: Configurações → Sobre → Verificar atualizações, ou o menu da bandeja. O canal é o `latest.json` do release mais recente deste repositório, e cada pacote é assinado: o app recusa um pacote sem a assinatura certa.

## Para desenvolvedores

O app fica em `frontend/`: a interface é Next.js/React e o núcleo é Rust/Tauri 2 (áudio, transcrição, banco local e resumos). A pasta `backend/` é um arquivo legado sem suporte.

```bash
cd frontend
pnpm install --frozen-lockfile
pnpm run tauri:dev
```

Antes do primeiro `tauri:dev`, prepare o sidecar `llama-helper` conforme o [guia de build](docs/BUILDING.md#first-run-sidecar-setup).

Testes:

```bash
cd frontend && bun test && npx tsc --noEmit -p .
```

```bash
cd frontend/src-tauri && cargo test --lib
```

Documentação:

- [Arquitetura e pipeline de áudio](docs/architecture.md)
- [Build e configuração por plataforma](docs/BUILDING.md) e [Linux](docs/building_in_linux.md)
- [Aceleração por GPU](docs/GPU_ACCELERATION.md)
- [Como lançar uma versão](docs/RELEASE.md): todos os instaladores são compilados num Mac por `scripts/release/lancar.sh`, sem GitHub Actions.
- [Guia para agentes de código](AGENTS.md) e [como contribuir](CONTRIBUTING.md)

## Histórico de versões

| Versão | Data | Destaque |
|---|---|---|
| [1.2.1](docs/releases/1.2.1.md) | 21/11/2026 | 50 correções: privacidade, telas mais confiáveis e textos em português |
| [1.2.0](docs/releases/1.2.0.md) | 20/11/2026 | IA opcional: piloto automático, ajuda ao vivo mais esperta, perguntas às reuniões |
| [1.1.0](docs/releases/1.1.0.md) | 20/11/2026 | Desempenho: app mais leve durante a gravação |
| 1.0.0 | 19/11/2026 | Primeira versão com atualização automática e instaladores para os três sistemas |

O histórico técnico completo está em [CHANGES.md](CHANGES.md).

## Licença e créditos

Licença MIT, a mesma do projeto original. Veja [LICENSE.md](LICENSE.md).

- [Meetily](https://github.com/Zackriya-Solutions/meetily), da Zackriya Solutions, é a base deste app.
- Trechos de código vêm do [whisper.cpp](https://github.com/ggerganov/whisper.cpp), do [Screenpipe](https://github.com/mediar-ai/screenpipe) e do [transcribe-rs](https://crates.io/crates/transcribe-rs).
- O modelo **Parakeet** é da NVIDIA, com a [conversão ONNX](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx) de istupakov.
