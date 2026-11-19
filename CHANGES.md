# Meetily Live — ajuda ao vivo

Fork local do Meetily. A licença MIT e o aviso de copyright originais permanecem. Nenhum serviço pago foi adicionado e nenhuma chave de API foi incluída.

## O que mudou

- O áudio do microfone e o áudio do sistema passam por detectores de voz separados. Cada trecho da transcrição ao vivo sai com `source` `mic` (você) ou `system` (as outras pessoas). A interface mostra "Você" e "Participantes".
- O arquivo gravado continua misturado, como antes. O resumo depois da reunião continua usando o texto salvo da transcrição.
- A transcrição em si continua numa fila só, no modelo já carregado (Whisper ou Parakeet). Não há um segundo modelo na memória. Quando os dois lados falam ao mesmo tempo, os trechos esperam nessa fila.
- Durante a gravação, o módulo `live_assist` guarda cerca de 75 segundos de texto e um resumo curto do que ficou para trás. Ele chama o modelo ao fim de uma frase do canal do sistema, quando detecta uma pergunta, ou depois de 1,5 s sem texto novo. Há um intervalo mínimo (frequência baixa 8 s, média 4 s, alta 2 s). Uma chamada nova cancela a anterior. O tempo limite da chamada ao vivo é 8 segundos.
- A resposta precisa ser JSON `{"perguntas":[],"respostas":[],"contexto":[]}`, no máximo 2 itens por lista e 25 palavras por item. JSON inválido é descartado. Itens repetidos ou muito parecidos também.
- O painel "Ajuda ao vivo" tem copiar, fixar e descartar. O atalho global é Ctrl+Shift+L (⌘+Shift+L no Mac). Dá para abrir o mesmo painel numa janela sempre visível, para ficar ao lado do Teams, Meet ou Zoom. Essa janela não se esconde do compartilhamento de tela.
- Por padrão o modelo é o Ollama local `llama3.2:latest`. O texto só sai da máquina se o provedor de resumo já configurado for um serviço na nuvem (OpenAI, Claude, Groq, OpenRouter ou OpenAI compatível).

## Como configurar

1. Instale o [Ollama](https://ollama.com/) e baixe o modelo padrão:

   ```bash
   ollama pull llama3.2
   ```

2. Abra o app, vá em Configurações → Ajuda ao vivo.
3. Deixe "Ativar ajuda ao vivo" ligado, escolha o modelo que o Ollama lista, o idioma (padrão pt-BR), a frequência e, se quiser, a pauta no campo "Contexto da reunião".
4. Grave uma reunião com microfone e áudio do sistema. O indicador "Gravando · Ajuda ao vivo" aparece enquanto os dois estão ativos.
5. Para manter tudo local, o provedor de resumo também precisa ser Ollama (ou o modelo local embutido não é usado pela ajuda ao vivo; ela prefere Ollama). Se o provedor de resumo for nuvem, a tela avisa que o texto da transcrição será enviado.

O atalho e a janela sempre visível não dependem de chave nenhuma.

## Teste manual

1. No Mac, abra o app assinado ad hoc (veja a seção de build).
2. Conceda Microfone e Gravação de Tela e Áudio do Sistema.
3. Num navegador, abra um vídeo de uma reunião ou uma call de teste com som pelas caixas (não por fone Bluetooth, se o áudio do sistema não aparecer).
4. No Meetily Live, escolha o microfone e o dispositivo de áudio do sistema e comece a gravar.
5. Fale uma frase curta no microfone e deixe o vídeo fazer uma pergunta.
6. Confira a transcrição: a sua fala como "Você" e a do vídeo como "Participantes".
7. Abra "Ajuda ao vivo" (atalho Ctrl+Shift+L ou ⌘+Shift+L). As seções Perguntas, Respostas e Contexto devem receber itens novos no topo; os antigos ficam mais claros. Copiar, fixar e descartar funcionam em cada item.
8. Use "Abrir em janela sempre visível" e confira que a janela fica por cima do navegador.
9. Pare a gravação e gere o resumo de sempre. O resumo precisa continuar sendo criado a partir da transcrição salva.
10. Com a ajuda ao vivo desligada nas configurações, grave de novo: a transcrição e o arquivo de áudio continuam, sem chamadas ao modelo.

## Medir latência de dois modelos locais

No Mac, com o Ollama no ar:

```bash
chmod +x scripts/measure_ollama_latency.sh
./scripts/measure_ollama_latency.sh llama3.2:latest qwen2.5:3b
```

O script mede só o HTTP do Ollama no mesmo caminho do app, `/v1/chat/completions`, três vezes por modelo. A primeira chamada costuma incluir a carga do modelo. Compare a mediana das chamadas 2 e 3.

Recomendação provisória, não medida neste ambiente e não medida num M5 Pro: comece por `llama3.2:latest`, que já é o padrão do app e das configurações de resumo. Se a mediana passar de 3 segundos, compare um modelo de instrução menor, em torno de 3B, com o mesmo script. Não há número de latência medido para o M5 Pro neste repositório.

## Build do .dmg no MacBook (M5 Pro, Apple Silicon)

Isto foi conferido em `docs/BUILDING.md`, `.github/workflows/build-macos.yml` e `frontend/src-tauri/build.rs`. O build baixa o FFmpeg sozinho na primeira compilação e precisa de rede. Este ambiente é Linux e não gera o `.dmg`.

Dependências:

- Xcode Command Line Tools (`xcode-select --install`), porque o Tauri no macOS liga AVFoundation, Cocoa e Foundation.
- Rust **1.99.1** com o alvo `aarch64-apple-darwin`. O workflow do repositório fixa essa versão: o Rust 1.100 mudou o `OUT_DIR` dos build scripts e o `tauri-build` 2.6.1 e o `ort-sys` 2.0.0-rc.10 ainda esperam o caminho antigo.
- Node.js 20.
- pnpm 9.15.9 (`npm install -g pnpm@9.15.9`).
- CMake (`brew install cmake`), usado pelo `llama-helper`.
- Ollama, à parte do build, para as sugestões.

```bash
rustup toolchain install 1.99.1
rustup default 1.99.1
rustup target add aarch64-apple-darwin

git checkout feature/live-assist
npm install -g pnpm@9.15.9

# O sidecar llama-helper. O workflow desliga OpenSSL porque o cpp-httplib
# empacotado não compila contra OpenSSL 4; o helper não usa HTTPS.
CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=TRUE cargo build --release -p llama-helper --features metal
mkdir -p frontend/src-tauri/binaries
cp target/release/llama-helper frontend/src-tauri/binaries/llama-helper-aarch64-apple-darwin

cd frontend
pnpm install --frozen-lockfile
pnpm tauri:build
```

O `.dmg` e o `.app` ficam em:

```text
frontend/src-tauri/target/release/bundle/dmg/
frontend/src-tauri/target/release/bundle/macos/
```

O nome do app é "Meetily Live". O identificador é `com.techfernandes.meetilylive`, então os dados ficam em `~/Library/Application Support/com.techfernandes.meetilylive` e não misturam com o Meetily original (`com.meetily.ai`).

`bundle.createUpdaterArtifacts` foi desligado. O updater ainda aponta para os releases do Meetily original e a assinatura dele pede a chave privada upstream, que este fork não tem. O `.dmg` local não tenta publicar atualização.

`macOS.signingIdentity` continua `"-"` no `tauri.conf.json`: assinatura ad hoc, sem conta de desenvolvedor Apple.

### Abrir o app não assinado

1. Mova o `Meetily Live.app` para `/Applications` se quiser.
2. Na primeira abertura, o macOS pode dizer que o app é de um desenvolvedor não identificado. Use o botão direito no app → Abrir → Abrir. Se ainda bloquear: Ajustes do Sistema → Privacidade e Segurança → Abrir Mesmo Assim.
3. Se o download tiver quarentena: `xattr -dr com.apple.quarantine "/Applications/Meetily Live.app"`.
4. Permissões, em Ajustes do Sistema → Privacidade e Segurança:
   - Microfone
   - Gravação de Tela e Áudio do Sistema (no macOS recente o áudio do sistema entra nessa permissão; as chaves do app são `NSMicrophoneUsageDescription`, `NSScreenCaptureUsageDescription` e `NSAudioCaptureUsageDescription`)
5. Reinicie o app depois de conceder. Sem a gravação de tela e áudio do sistema, o canal "Participantes" fica vazio.

## Limitações conhecidas

- Dois fluxos de VAD, um único modelo de transcrição. A separação não cobre duas pessoas no mesmo canal. O áudio do sistema é todo mundo que não é o microfone.
- A meta de primeira sugestão em até 3 s depende do modelo local e não foi medida aqui. O corte duro da chamada é 8 s.
- A ajuda ao vivo não faz streaming de tokens. Reusa `generate_summary`, que é um POST só, e corta em 8 s com cancelamento.
- Não há diarização por pessoa, só mic contra sistema.
- A janela sempre visível é outra webview: fixar ou descartar num lado não altera o outro.
- O atalho global pode falhar se outro app já registrou Ctrl+Shift+L ou ⌘+Shift+L. O app registra na subida e segue mesmo se o registro falhar.
- Linux e Windows herdam o mesmo código, mas o `.dmg` e as permissões de áudio do sistema descritas acima são do macOS.
