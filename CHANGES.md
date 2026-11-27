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
- A separação por pessoa usa o espectro do trecho, não um modelo neural. Vozes sobrepostas no mesmo trecho continuam uma pessoa só. Veja a seção "Quem está falando".
- A janela sempre visível é outra webview: fixar ou descartar num lado não altera o outro. Os nomes dados às pessoas ficam na reunião salva e valem para os trechos novos da gravação em andamento.
- O atalho global pode falhar se outro app já registrou Ctrl+Shift+L ou ⌘+Shift+L. O app registra na subida e segue mesmo se o registro falhar.
- Linux e Windows herdam o mesmo código, mas o `.dmg` e as permissões de áudio do sistema descritas acima são do macOS. A detecção de reunião só observa processos e janelas no macOS.

## Pesquisa na internet

A ajuda ao vivo procura termos que as outras pessoas citam (empresa, sigla, produto, nome entre aspas) e que ainda não estão no contexto da reunião. A busca usa duas APIs públicas, sem chave:

- Wikipédia: `GET https://{pt|en}.wikipedia.org/api/rest_v1/page/summary/{título}` com `User-Agent` próprio. O texto vem de `extract` e o link de `content_urls.desktop.page`.
- DuckDuckGo Instant Answer: `GET https://api.duckduckgo.com/?q={termo}&format=json&no_html=1&skip_disambig=1`. O texto vem de `AbstractText` e o link de `AbstractURL`.

Só o termo sai da máquina. O resultado entra em Contexto, com o link, e também vai para o modelo local (ou para o provedor de resumo, se ele for nuvem). Se a rede falhar, a sugestão segue sem a fonte. O interruptor "Pesquisa na internet" fica ligado por padrão. Há cache na memória e na tabela `lookup_cache`.

## Recursos

O botão Recursos fica na tela de gravação e no painel de ajuda. Os mesmos interruptores estão em Configurações → Ajuda ao vivo. O padrão é ligado, menos a tradução, que fica em "Automática".

| Recurso | O que faz | O que sai da máquina |
| --- | --- | --- |
| Tarefas | Depois da reunião, lê quem, o quê e o prazo e grava um `.ics` de tarefas (`VTODO`). O arquivo abre no app do sistema. Não usa EventKit: o cidre fixado neste repo não tem esse framework. | Nada |
| Memória | Busca reuniões salvas com o mesmo título ou assunto e mostra o trecho e as tarefas ainda escritas. | Nada |
| Agenda | No macOS, pede ao app Calendário (dicionário de script da Apple) os compromissos das próximas 24 horas e preenche o contexto com título, pauta e participantes. Também aceita um `.ics`. Contas do Google ou Outlook já sincronizadas no Calendário entram por ali. | Nada |
| Nomes | Ao renomear uma voz, guarda o espectro local e reaplica o nome numa reunião futura se a semelhança passar de 0,94. Dá para corrigir de novo. | Nada |
| Alertas | Avisa na hora quando alguém combina prazo, valor ou entrega. | Nada |
| E-mail | Monta um rascunho a partir do texto e abre `mailto:`. Não envia. | Nada, até você enviar pelo app de e-mail |
| Busca | Pergunta em linguagem natural, ranqueia trechos salvos e pede uma resposta curta ao modelo já configurado, citando data e trecho. | Os trechos só saem se o provedor de resumo for nuvem |
| Placar | Tempo de fala por pessoa, perguntas sem resposta em 20 s e cobertura da pauta. | Nada |
| Tradução | Em "Automática", pede tradução para pt-BR e resposta no idioma da reunião quando a fala parece inglês ou espanhol. "Sempre" e "Desligada" também existem. | O mesmo caminho da ajuda ao vivo |

Permissões novas no `Info.plist`: `NSCalendarsUsageDescription` e `NSRemindersUsageDescription`. A leitura da agenda usa o app Calendário e pode pedir permissão de Automação. O `.ics` de tarefas não exige essa permissão. O app continua visível no compartilhamento de tela. Não há modo de objeção de vendas.

## Gravação automática (Teams, Zoom e Meet)

No macOS, com a opção ligada (é o padrão, em Configurações → Gravação), o app observa a cada 2 segundos:

- se o Teams novo (`com.microsoft.teams2`) ou o Teams clássico (`com.microsoft.teams`) tem o microfone em uso (`kAudioProcessPropertyIsRunningInput`, Core Audio, macOS 14.2 ou mais novo);
- ou se a janela do Teams tem título de reunião, mesmo com o microfone no mudo ou numa reunião só sua. O app aberto na lista de processos não esconde mais esse título. Telas ociosas em português (Bate-papo, Calendário, Chamadas, Atividade) não disparam;
- se o Zoom (`us.zoom.xos`) tem o microfone em uso ou a janela diz “Zoom Meeting”;
- se o Chrome, Safari, Edge, Firefox, Brave ou Arc está numa chamada do Meet (`Meet - código`) ou do Teams.

O título da janela vem de `CGWindowListCopyWindowInfo` (Core Graphics). O nome da janela de outro app só aparece se a Gravação de Tela já estiver concedida. Essa permissão o app já pede (`NSScreenCaptureUsageDescription`). Não há chave nova no Info.plist. Os títulos não são gravados em log nem enviados ao modelo.

A chamada precisa permanecer cerca de 12 segundos antes de a gravação começar, e precisa sumir por cerca de 20 segundos antes de parar. Uma gravação iniciada à mão não é parada por essa lógica; quem cuida dela é a detecção de fim de reunião, descrita abaixo. Se você parar a gravação automática, ela não recomeça enquanto a chamada ainda estiver ativa. Desligar a opção impede uma nova gravação automática; a que já estiver no ar ainda para quando a chamada termina.

Quando a reunião é reconhecida, o topo mostra um contador de cerca de 12 segundos e depois o aviso com Parar. Em Configurações → Gravação, “Status agora” explica o que o Mac está vendo, sem gravar o título da reunião. “Verificar agora” lê de novo. O roteiro na mesma tela pede um teste no Teams (Reunir agora, pode ser sozinho), no Zoom e no Meet.

A notificação “Recording Start Notification” fica desligada por padrão. Quem já tinha gravado a preferência como ligada continua vendo o lembrete. O app não se esconde do compartilhamento de tela. O texto na tela pede para avisar as outras pessoas de que a reunião está sendo gravada.

No Linux e no Windows essa observação devolve lista vazia, então a gravação automática não dispara. O interruptor continua visível.

Limites: uma mensagem de voz no Teams que use o microfone por mais de 12 segundos pode iniciar uma gravação. Sem a permissão de Gravação de Tela, o macOS não entrega o título da janela de outro app; nesse caso só o microfone em uso começa a gravação, e o status avisa isso. Uma reunião cujo título continua só “Microsoft Teams” também depende do microfone sem mudo.

## Fim da reunião

O app também percebe quando a reunião acaba, para qualquer gravação, inclusive a iniciada pelo botão. A opção fica em Configurações → Gravação, no cartão "Fim da reunião", e vale mesmo com a gravação automática desligada.

Dois indícios, avaliados a cada 2 segundos no mesmo laço da gravação automática:

- **Chamada encerrada**: havia uma chamada do Teams, do Zoom ou do Meet durante a gravação e ela sumiu por cerca de 20 segundos. A gravação que o próprio app iniciou continua parando sozinha nesse prazo, como antes, sem aviso.
- **Silêncio**: nenhuma fala no microfone nem no áudio do sistema pelo tempo configurado (padrão 3 minutos, de 1 a 60). Enquanto a chamada ainda aparece, o prazo é três vezes maior (9 minutos no padrão), porque uma sala de espera também fica em silêncio. Gravação pausada não conta.

A fala vem dos dois detectores de voz que o pipeline já roda (um por fonte): cada janela com fala grava um carimbo de tempo atômico em `audio/speech_activity.rs`. Nada muda no áudio da gravação nem no que vai para a transcrição.

Três modos: "Perguntar" (padrão), "Parar sozinho" e "Desligado". No modo perguntar aparece um aviso no app, abaixo da barra superior, com contagem regressiva de 60 segundos, "Encerrar agora" e "Continuar gravando", e uma notificação do sistema. Sem resposta, a gravação encerra. "Continuar gravando" adia novos avisos por 10 minutos e a mesma chamada encerrada não avisa de novo. Se a fala voltar durante um aviso de silêncio, ou se uma chamada aparecer, o aviso some. O encerramento usa o mesmo caminho da parada automática e da bandeja (`stop_recording`, com drenagem e flush), e a reunião segue o fluxo normal de salvar, resumir e organizar. O log registra só o motivo (`signal`, `silence`, `timeout` ou `user`).

Limites: a notificação do sistema não tem botões (o plugin de notificações no desktop não oferece), então a resposta é dada no app. Música ou vídeo com voz no áudio do sistema conta como fala. Uma conversa presencial que continua depois de a chamada cair recebe o aviso de chamada encerrada e precisa de "Continuar gravando". Fora do macOS não há sinal de chamada; só o silêncio funciona.

## Quem está falando

Não há modelo de diarização baixado. O app calcula um espectro de 16 bandas (FFT de 512 pontos, `realfft`, que já era dependência) de cada trecho de voz e agrupa trechos parecidos no mesmo canal. O primeiro grupo do microfone fica "Você". Os outros ficam "Pessoa 1", "Pessoa 2" e assim por diante. Um trecho mais curto que 0,8 s herda a pessoa anterior daquele canal.

Isso não separa duas pessoas falando ao mesmo tempo no áudio do sistema: o trecho inteiro vira uma pessoa. Também não é um embedding treinado, então vozes parecidas ou um trecho com ruído podem cair na pessoa errada. Não há arquivo de modelo para baixar e o cálculo roda na CPU, inclusive no Apple Silicon.

Clique no nome na transcrição para renomear. O nome vale para aquela reunião (`speaker_aliases` e a coluna `transcripts.speaker`; a chave estável fica em `speaker_key`). Reuniões já salvas, sem essa coluna preenchida, continuam como estavam. O rótulo entra no texto da ajuda ao vivo, para a resposta mirar quem perguntou, e no texto enviado ao resumo depois da reunião.

## Meetily Live Plus (0.5.0, branch `feature/live-plus`)

O app passou a se chamar **Meetily Live Plus**, com um "+" no canto do ícone. O identificador (`com.techfernandes.meetilylive`) não mudou, então reuniões, modelos e configurações continuam no mesmo lugar.

### Instalar e atualizar

- `scripts/atualizar-mac.sh` faz tudo: puxa a branch, compila o `llama-helper` só quando ele mudou, compila o app, gera o `.dmg`, instala em `/Applications`, tira a quarentena, confere a assinatura e abre. Opções: `--sem-pull`, `--nao-abrir`, `--forcar`.
- Assinatura fixa: se `MEETILY_SIGN_ID` ou o arquivo `~/.config/meetily-live/assinatura` (uma linha com o nome ou o SHA-1 da identidade do chaveiro; não é segredo) apontar para uma identidade válida, o app sai assinado com ela e o macOS mantém Microfone e Gravação de Tela entre atualizações. Sem isso, a assinatura é ad hoc, como antes.
- O script não fecha o app durante uma gravação: o núcleo mantém o arquivo `recording.active` na pasta de dados enquanto grava (`recording_marker.rs`).
- Saíram o atualizador do upstream (plugin `updater`, que apontava para as versões do projeto original) e o envio de telemetria para o PostHog (`init_analytics` não cria mais o cliente).

### Ajuda ao vivo

- As sugestões aparecem enquanto o modelo escreve (evento `live-assist-partial`), em vez de só no fim.
- O atalho global é configurável (Configurações → Ajuda ao vivo). Se outro app já usa a combinação, o app avisa em vez de falhar calado.
- A janela flutuante e a janela principal ficam sincronizadas (ligar/desligar, modelo, atalho).
- Medição de latência dentro do app: escolhe os modelos locais do Ollama, roda a mesma pergunta e mostra o tempo até o primeiro texto e o total.

### Contexto das ajudas

- A ajuda ao vivo agora se apoia em contexto local: dados da reunião (cliente, projeto, quem fala), um perfil do dono com glossário (Configurações → Ajuda ao vivo → Perfil e contexto das sugestões) e anotações das últimas 4 reuniões salvas do mesmo cliente (decisões e pendências tiradas dos resumos).
- Cada sugestão pode trazer a origem em uma etiqueta (campo opcional `origin` nos eventos `live-assist-update` e `live-assist-partial`): reunião de DD/MM, perfil, glossário, internet, pauta, conversa ou "sem fonte local".
- Termos do glossário ditos na conversa e a anotação anterior que bate com o que está sendo falado aparecem na hora, sem chamada ao modelo.
- O painel destaca a seção Contexto e mostra "Contexto em uso" (evento `live-assist-context`).
- A reunião em gravação ainda não está salva, então o cliente é reconhecido pelo título, pelo contexto digitado ou por duas menções na conversa, entre os clientes já conhecidos.
- Orçamento fixo: o contexto automático enviado ao modelo tem no máximo 900 caracteres (400 quando o contexto digitado é longo). O banco é lido no início da gravação, quando o cliente é identificado e ao salvar configurações; não há consulta por trecho.
- Com provedor na nuvem, perfil, glossário e reuniões anteriores não entram no prompt.

### Privacidade

- Na primeira abertura, o app pergunta como tratar a gravação automática e a pesquisa na internet. O padrão é conservador: a gravação automática **pergunta antes** de gravar e a pesquisa de termos fica desligada até você ligar.
- Quando uma reunião é detectada no modo "perguntar", aparece um aviso com Gravar / Agora não.

### Organização das reuniões

- Título automático objetivo: cliente, assunto e data, conforme o modelo de título em Configurações → Organização (por exemplo `{cliente} · {assunto} · {data}`). Sai da transcrição e do resumo, usando o modelo local já configurado; sem modelo, usa regras simples.
- Cada reunião ganha cliente, projeto, categoria e etiquetas. A barra lateral agrupa por data, cliente ou categoria e tem busca (⌘K).
- Classificação manual: menu de contexto da reunião → Classificar. O que você define à mão não é sobrescrito pela organização automática; o mesmo vale para um título que você renomeou.
- "Organizar reuniões antigas" processa em lote, com progresso e cancelamento. Atenção: reuniões criadas antes desta versão entram como "título padrão", então um título renomeado à mão **antes** da 0.5.0 pode ser trocado nesse lote.

### Quem está falando (modelo de voz)

- Opcional: Configurações → Ajuda ao vivo → modelo de voz. Baixa o CAM++ (3D-Speaker, ONNX, 28 MB, Apache-2.0) do release do sherpa-onnx, com SHA-256 conferido, e roda no próprio Mac. Com ele, a separação de pessoas usa um embedding treinado; sem ele, continua o espectro de 16 bandas descrito acima.
- O modelo foi treinado com chinês e inglês e os limiares foram ajustados com vozes sintéticas; em português, confira os rótulos e renomeie quando errar.

### Visual

- Layout novo no estilo dos apps de assistente (Claude Desktop, Codex, Antigravity): barra lateral recolhível com as conversas, barra superior com título editável, controle de gravação em pílula na base, configurações em duas colunas.
- Tema claro e escuro (Sistema / Claro / Escuro), com tokens de cor únicos.
- Interface em português.

### Banco de dados

Quatro migrações novas rodam na primeira abertura: `20261120000000_assist_upgrades`, `20261120010000_meeting_organization`, `20261120020000_voice_model`, `20261121000000_live_assist_context`. Elas só acrescentam colunas e tabelas.

### Modelos recomendados para este Mac

- Cartão novo no topo de Configurações → Transcrição e → Resumo. O app lê a memória e o chip da máquina e recomenda, por função (fala, vozes, assistente), um conjunto que cabe junto na memória. Nada é baixado nem trocado sem clique em "Baixar e usar".
- Faixas (português): 8 GB (MacBook Air M2): Parakeet v3 int8, CAM++ e `llama3.2`; 12 GB: `qwen3.5:4b`; 16 GB: ReDimNet2-B6 nas vozes e `gemma4:e4b`; 24 GB ou mais: `gemma4:12b`; 48 GB ou mais: `gemma4:26b`. Desde 23/11/2026 essas faixas valem para **resumo e organização**; a ajuda ao vivo fica no `llama3.2` em qualquer faixa (ver "Ajuda ao vivo leve", no fim).
- Vozes: novo modelo opcional ReDimNet2-B6 (51 MB, MIT). Em 20 falantes em português (fala lida) errou menos que o CAM++ (EER 0,47% contra 0,59% em trechos de 3 s), mas custa cerca de 0,5 s de CPU por turno; por isso só é recomendado a partir de 16 GB.
- Fala: entraram como alternativas o Parakeet v3 TAGARELA (ajustado para pt-BR, CC-BY-4.0) e o Whisper `large-v3-turbo-q8_0`. No teste com 80 trechos em português o TAGARELA empatou com o Parakeet v3 que já era o padrão (WER 8,4% contra 8,5%), então o recomendado não mudou.
- Os modelos do Ollama são baixados pelo próprio Ollama local (`/api/pull`), com progresso e cancelamento. Só o `llama3.2` foi executado nos testes; os demais foram conferidos no registro (tamanho e licença), não em uso. Meça a latência em Configurações → Ajuda ao vivo antes de adotar um modelo maior.
- Migrações novas desta rodada: `20261121000000_live_assist_context`, `20261121010000_voice_model_choice`, `20261121020000_meeting_end_settings`.

### Ajustes de 19/11/2026

- A ajuda ao vivo vem **desligada por padrão** (a migração `20261122000000_live_assist_off_by_default` desliga uma vez; depois vale a escolha do usuário).
- As sugestões ao vivo usam a API nativa do Ollama (`/api/chat`) com `num_ctx` de 4096 e sem modo de raciocínio. Com o `gemma4:12b` num Mac de 24 GB, o modelo recém-carregado caiu de 16 GB para 7,8 GB. Esse número só vale logo depois da carga: em uso contínuo a memória volta a subir (medição de 23/11/2026, no fim). Se a chamada nativa não for aceita, o app volta para a rota compatível com OpenAI. Resumo e organização continuam com o contexto padrão do Ollama.
- Barra lateral: durante a gravação o rodapé mostra só o tempo, sem empurrar os ícones; os filtros de conversas quebram em até duas linhas.
- O diálogo "Classificar reunião" não deixa mais os campos passarem da largura.

## Meetily Live Plus 1.0.0 (19/11/2026)

- Canal oficial de atualização: o app consulta `latest.json` do release mais recente deste repositório, avisa quando há versão nova e instala por conta própria (Configurações → Sobre → Verificar atualizações, ou o menu da bandeja). A conferência se repete enquanto o app fica aberto.
- Instaladores para Mac Apple Silicon, Mac Intel, Windows x64 e Linux x86_64, todos compilados neste Mac por `scripts/release/lancar.sh`. O processo está em `docs/RELEASE.md`.
- Metadados do pacote passam a indicar a Tech Fernandes como responsável por esta versão.

## Meetily Live Plus 1.1.0 (20/11/2026)

Versão de desempenho. Notas para o usuário em `docs/releases/1.1.0.md`.

- Relógio da gravação: o `RecordingStateContext` conta o tempo localmente a cada segundo e só consulta `get_recording_state` a cada 5 segundos; o `useRecordingStateSync` passou de 1 s para 5 s.
- Barras de nível animadas por CSS (`.level-bar`, com alternativa para movimento reduzido) em vez de um `setInterval` de 300 ms que redesenhava a página.
- Efeito de digitação da transcrição ao vivo a cada 33 ms (antes 15 ms).
- Editor BlockNote do resumo carregado sob demanda (`LazyBlockNoteSummaryView`): o primeiro carregamento de `/meeting-details` caiu de 812 kB para 273 kB.
- Perfil de release no `Cargo.toml` da raiz: `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`.
- Pipeline de áudio: uma cópia a menos por janela mixada e o RMS de diagnóstico só é calculado nos blocos que vão para o log.
- Removidos arquivos Rust e componentes React que não eram compilados nem importados (cerca de 6.900 linhas) e a dependência `lodash`.
- `scripts/release/lancar.sh` compila o Linux em paralelo com o Mac e o Windows; `scripts/release/mac.sh` usa o certificado Developer ID e notariza quando ele está no Keychain.

## Meetily Live Plus 1.2.0 (20/11/2026)

Versão de inteligência, com tudo novo desligado por padrão. Notas para o usuário em `docs/releases/1.2.0.md`; detalhes no PR #6.

- Provedores (`anthropic/`, `openai/`, `openrouter/`, `summary/llm_client.rs`): lista de modelos da API com cache e lista de reserva, `test_llm_provider_connection`, retry com backoff em 429/5xx/529, erros em pt-BR, prompt caching da Anthropic, `set_llm_reasoning_effort` e `set_llm_fallback_chain`.
- `autopilot/`: piloto automático pós-reunião (resumo, tarefas, decisões, título, etiquetas, rascunho de e-mail), disparado depois de `api_save_transcript`. Migração `20261125000000_autopilot.sql`.
- `live_assist/` e `assist_plus/`: alertas locais, recapitulação, sugestão de resposta, modo economia e consumo estimado. Migração `20261123000000_live_assist_smart.sql`.
- `ask/`: índice local de trechos (FTS5 ou LIKE, criado em tempo de execução) e `ask_meetings` com citações.
- `summary/templates/detect.rs` e `speakers/suggest.rs`: tipo de reunião, escolha do modelo de resumo e sugestão de nomes; cinco modelos novos em `frontend/src-tauri/templates/`.
- `jev/`: conexão opcional com o Jev da TypeSafe AI.

## Meetily Live Plus 1.2.1 (21/11/2026)

Versão de correções. Notas para o usuário em `docs/releases/1.2.1.md`; detalhes no PR #8.

### Correções sobre a 1.2.0

50 correções e micromelhorias. Nenhuma muda comando Tauri, evento ou formato de dados.

1. Ajuda proativa: os switches ficam travados até as configurações salvas carregarem (antes, mexer cedo gravava os padrões por cima).
2. Ajuda proativa: falha ao salvar volta o valor anterior, e respostas fora de ordem não sobrescrevem a escolha mais nova.
3. Alertas proativos da gravação anterior somem quando uma nova gravação começa.
4. Piloto automático: o aviso de término não se perde mais (os ouvintes eram recriados a cada renderização da página e o spinner podia ficar girando para sempre).
5. Piloto automático: cargas sobrepostas não mostram mais "trabalhando" depois que o piloto terminou.
6. Piloto automático: "Usar como título" e "Adicionar etiquetas" ficam desativados enquanto aplicam (sem envio duplo).
7. Piloto automático: listas com itens repetidos não geram chaves duplicadas no React.
8. Piloto automático: mensagens de cópia concordam em gênero e número ("Tarefas copiadas", "E-mail copiado").
9. Piloto automático: indicador "Em andamento" passa a ser anunciado por leitores de tela.
10. Piloto automático: aviso de modelo local sem "()" vazio quando o modelo não é conhecido.
11. Configurações do piloto automático: ligar e desligar rápido não deixa a tela diferente do que foi salvo.
12. "Testar conexão": o resultado ("Conexão OK") some ao trocar de provedor ou de chave.
13. Cadeia de alternativas: não aceita o mesmo provedor e modelo duas vezes.
14. Cadeia de alternativas: Enter não adiciona durante composição de acentos (IME) nem envia o formulário.
15. Cadeia de alternativas: botões e campo com nomes acessíveis específicos ("Remover <modelo>", "Adicionar alternativa").
16. Modelo de resumo sugerido: o aviso "Modelo sugerido" e o registro de uso não se perdem mais quando o modelo é trocado.
17. Pergunte às reuniões: "cerca de 1,2 mil tokens" (antes "mil de tokens").
18. Pergunte às reuniões: plural certo em "1 de 1 reunião indexada".
19. Pergunte às reuniões: "Redigir resposta" fica desativado quando não há modelo configurado (antes mostrava aviso de custo de uma chamada que não acontece).
20. Ajuda ao vivo: o aviso de nuvem mostra o nome do provedor ("Claude (Anthropic)") em vez do código interno ("claude", "custom-openai").
21. Alertas e painel da ajuda ao vivo: botões só com ícone ("Dispensar", "Recolher") ganham nome para leitor de tela.
22. Resumo dos últimos minutos: quando não há nada novo, mostra "Nada de novo nesse período." em vez de uma lista vazia.
23. Privacidade: a chave de API da transcrição e a do servidor OpenAI personalizado não são mais impressas no console (Configurações, contexto e janela de modelos).
24. Privacidade: o texto de cada trecho da transcrição ao vivo não é mais impresso no console; fica só o tamanho.
25. Página da reunião: removido um log que rodava a cada renderização.
26. Recuperação de reuniões: clicar em "Ver reunião" cancela a navegação automática (antes a página era aberta duas vezes), e o temporizador é limpo ao sair da tela.
27. Recuperação de reuniões: o aviso volta a aparecer numa próxima sessão quando não sobra nada a recuperar (a checagem lia a lista antiga e nunca limpava).
28. Recuperação de reuniões: uma falha mostra uma mensagem só (antes vinha o toast e mais um alert nativo); falha ao excluir vira toast.
29. Recuperação de reuniões: datas e horas em pt-BR mesmo com o Mac em inglês; "Excluindo…" e "Recuperando…" traduzidos.
30. Gravação: "Processando a gravação…" e "Parando…" traduzidos.
31. Resumo vazio: botão "Tentar de novo" em vez de "Retry summary".
32. Erro de transcrição: o aviso ganha título ("Erro na transcrição") em vez de uma linha vazia.
33. Copiar reunião: data em pt-BR.
34. Renomear reunião: depois de cancelar com Esc, o primeiro Enter da próxima edição volta a salvar.
35. Gravação: os avisos de erro de transcrição e de fala detectada eram removidos e registrados de novo a cada segundo; agora ficam registrados uma vez, sem janela em que um erro se perde nem ouvintes duplicados.
36. Resumo: um clique duplo em "Gerar resumo" não inicia mais duas gerações ao mesmo tempo.
37. Cronômetro da gravação (tela e barra lateral): mostra horas a partir de 1 hora ("1:15:03" em vez de "75:03"), com teste novo.
38. Busca na barra lateral: resultados de uma busca antiga não sobrescrevem os da busca mais nova, e o indicador de busca não some antes da hora.
39. Sobre: classes de tamanho de texto inexistentes ("text-s", "text-medium") trocadas pelas certas, então os textos ficam no tamanho pretendido.
40. Privacidade: com provedor principal local (Ollama ou IA embutida), a cadeia de alternativas não cai mais para um serviço fora da máquina (nuvem ou servidor OpenAI personalizado); o perfil e as notas de reuniões anteriores não saem sem aviso. O texto da tela explica a regra, com teste.
41. Custo no Claude: o conteúdo de cada chamada não é mais marcado para cache. Ele muda a cada chamada, então pagava a taxa de gravação de cache (1,25x) sem nunca reaproveitar. O cache do prompt de sistema continua.
42. Pergunte às reuniões: "contemplam" e "contemporâneo" não são mais lidos como "ontem" (o filtro de período passa a casar palavras inteiras), com teste.
43. Resumo dos últimos minutos e "Sugerir resposta": com provedor na nuvem, o erro mostra o motivo ("Sem crédito…", "Chave de API inválida…") em vez de uma mensagem genérica, com teste.
44. "Sugerir resposta" respeita a ajuda ao vivo desligada, como o resumo dos últimos minutos já fazia.
45. Resumo dos últimos minutos: segue o idioma escolhido na ajuda ao vivo (o prompt fixava português e contradizia o pedido).
46. Privacidade: o piloto automático registra no log só o tamanho do erro do modelo, não o texto (servidores locais podem devolver trechos da transcrição no erro).
47. Possíveis divergências: "12,5 mil" e "12.500" agora são o mesmo número (antes geravam alerta falso), com teste.
48. Modelo de resumo sugerido: horários como "11:15" e "21:10" não contam mais como "1:1", e "março" não conta mais como marco de projeto, com teste.
49. Sugestão de nomes de quem fala: palavras como "Olha", "Beleza", "Enfim" e "Tipo" não são mais sugeridas como nomes, com teste.
50. Lista de modelos da OpenAI: esconde modelos que só funcionam na Responses API (-pro, codex, deep-research), que davam erro ao gerar resumo.

### Ajuda ao vivo leve (23/11/2026)

Medição num MacBook Pro M5 Pro de 24 GB, Ollama 0.40.16, texto sintético, uma chamada a cada 2 a 4 segundos como a ajuda ao vivo faz:

| Modelo na ajuda ao vivo | Memória do Ollama |
|---|---|
| `gemma4:12b`, `gemma4:e4b`, `qwen3.5:9b` (rodam pelo MLX) | 15 a 16 GiB em 5 a 8 minutos, com qualquer `num_ctx` e qualquer formato de prompt |
| `llama3.2`, prompt antigo (janela deslizante) | de 0,6 para cerca de 9 GiB em 4 minutos |
| `llama3.2`, prompt novo | 3,2 GiB depois de 9 minutos e 144 chamadas |

A causa é o cache de prompts do Ollama, que cresce quando o começo do prompt muda a cada chamada; não é o tamanho do modelo. Uma chamada única de resumo com o `gemma4:12b` fica em cerca de 8,5 GiB, dentro do que o cartão de modelos estima.

- O cartão "Modelos recomendados" separa as duas funções. A lista do assistente passa a valer só para **resumo e organização** ("Usar nos resumos"). Um bloco novo, "Ajuda ao vivo (durante a reunião)", recomenda sempre o `llama3.2`. Antes, um clique em "Usar" punha o mesmo modelo grande nas duas funções.
- Quem já usa um modelo bem mais pesado na ajuda ao vivo vê um aviso, uma vez por modelo, com o botão "Usar Llama 3.2" (só aparece se ele já estiver baixado). Nada é trocado sem clique.
- O prompt da ajuda ao vivo foi reordenado: a conversa vem antes e só cresce no fim; contexto automático, fontes e sugestões já mostradas vêm depois dela. A janela guarda 75 segundos depois de um corte e só corta de novo ao passar de 180 segundos.
- Ao parar a gravação, o app pede ao Ollama para descarregar o modelo da ajuda ao vivo, a menos que ele seja o mesmo do resumo.
- Limites: o teto de 16 GiB só foi medido neste Mac de 24 GB. Com o prompt novo a memória do `llama3.2` ainda sobe um pouco a cada corte da janela. A qualidade das sugestões com a nova ordem do prompt não foi comparada com a anterior. Nada disso foi exercitado no app aberto, só em testes automatizados e num script de carga.

## Em desenvolvimento (branch `feature/agenda-tarefas-conversa`, 27/11/2026, ainda sem versão)

Tudo local: nenhum serviço novo, nenhuma conta, nenhuma chave. Migração nova `20261127000000_workspace.sql` (tabelas `tasks`, `task_runs`, `meeting_participants`, `clients`, `workspace_settings`, `obsidian_notes`).

**Telas novas no menu lateral**

- **Agenda**: reuniões por dia, por semana (grade de horas, blocos lado a lado quando se sobrepõem) e em linha do tempo. A busca olha título, cliente, projeto, categoria, etiquetas, participantes e o que foi dito (`/` leva o foco para a busca).
- **Tarefas**: "Com você", "Aguardando retorno", "Sem responsável" e "Concluídas", com prazo, atraso, filtro por cliente e criação à mão.
- **Clientes**: reuniões, horas, tarefas abertas e pessoas de cada cliente; apelidos e domínios de e-mail; renomear para um nome existente une os dois.

**Dentro da reunião**

- Aba **Conversa**: balões no estilo de aplicativo de mensagens, você à direita, cada pessoa com nome e cor, busca dentro da conversa. Clicar no nome diz quem é aquela voz (lista de participantes ou nome livre).
- Aba **Pessoas e tarefas**: participantes (Calendário do macOS, arquivo do Teams ou à mão) e as tarefas da reunião.
- Menu "Mais ações": exportar em `.txt` ou `.md`, enviar ao Obsidian, abrir no Obsidian.

**Tarefas inteligentes**: quando o resumo termina, o app extrai o que ficou combinado (modelo de resumo configurado; se falhar, regras sobre o resumo). Cada tarefa tem responsável e prazo; o nome em Configurações → Organização decide o que é seu e o que é devolução dos outros. Gerar de novo mantém o que foi editado, concluído ou criado à mão.

**Quem estava na reunião do Teams**

- O título da janela do Teams vira o nome da gravação automática.
- Os convidados vêm do evento do Calendário do macOS que cobre o horário da gravação.
- "Importar do Teams" lê a transcrição `.vtt`, a lista de presença `.csv` ou um convite `.ics`. Com o `.vtt`, o app alinha os textos e sugere o nome de cada voz.
- Não foi feito: login na Microsoft (Graph) e leitura da lista de participantes pela janela do Teams.

**Clientes e organização**: apelido vira o nome oficial; domínio de e-mail de um participante define o cliente; o nome original da reunião entra no contexto da classificação.

**Obsidian** (Configurações → Integrações): escolhe o cofre e a pasta; cada reunião vira uma nota Markdown com propriedades, links `[[cliente]]` e `[[pessoa]]`, tarefas `- [ ]` e a conversa. O que estiver abaixo da linha marcada no fim da nota é preservado ao reenviar. Envio automático opcional.

**Modelos**

- Whisper usa o número de threads do perfil da máquina (antes ficava no padrão da biblioteca) e reaproveita o estado entre trechos.
- O perfil de hardware lê a memória real (antes assumia 8 GB).
- Resumo com modelo embutido reserva espaço para a resposta ao dividir a transcrição; com Ollama, divide pelo contexto que o servidor realmente usa (antes mandava a reunião inteira e o Ollama cortava).
- Pedidos ao `llama-helper` não se cruzam mais quando chegam ao mesmo tempo.
- Importar e retranscrever copiam menos áudio na memória.
- Modelo padrão de reserva `large-v3-turbo` e idioma inicial `auto`, iguais aos da interface.
- Nenhuma velocidade foi medida; são mudanças de código, sem benchmark.

**Configurações**: todos os cartões com o mesmo formato, títulos e espaçamento; controles do kit no lugar de `select` e caixas nativas; cores por tokens; cabeçalhos repetidos removidos; nova aba Integrações (Obsidian e Jev); textos que estavam em inglês traduzidos; arquivos sem uso apagados (`SettingTabs`, `CustomDialog`, `MainNav`, `app/metadata.*`).

**Verificado (macOS arm64)**: `cargo test --lib` 716 passaram; `bun test` 137 passaram; `tsc --noEmit` e `next build` sem erros. **Visto no app em modo dev, com os dados reais**: barra lateral, Agenda (dia, semana, linha do tempo), Tarefas, Clientes, abas Conversa e Pessoas da reunião, geração de tarefas a partir do piloto automático (7 tarefas), menu de exportação e todas as abas de Configurações. **Não verificado**: gravar o .txt/.md e a nota do Obsidian (só os testes do Rust), leitura do Calendário, importação de arquivo do Teams, título real do Teams, e ganho de velocidade dos modelos.
