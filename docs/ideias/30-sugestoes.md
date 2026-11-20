# 30 sugestões para o Meetily Live Plus

Ordenadas por impacto para quem usa o app no dia a dia. Todas partem do que o código faz hoje (novembro de 2026) e seguem uma regra: **o app continua completo e local sem nada pago; o que é pago entra como benefício opcional, desligado por padrão, com a alternativa local ao lado.**

Legenda: esforço **P** (dias), **M** (1 a 3 semanas), **G** (mais de 3 semanas). Custo: **grátis/local**, **pago opcional** ou **misto** (local por padrão, nuvem como reforço).

## Onde o Jev entra

O Jev é o modelo de decisão “System One” da TypeSafe AI (`POST /v1/systemone`). Ele não escreve texto. Responde perguntas tipadas com probabilidade: sim/não (`noul`), escolha entre opções (`choice`) e nota numa escala (`score`). Pela listagem da Vercel AI Gateway, custa US$ 0,042 por milhão de tokens de entrada, com contexto de 32 mil tokens (valores a conferir antes de divulgar). É bem mais barato por chamada que um LLM de resumo; quanto, depende do modelo comparado.

O papel dele aqui é de **porteiro**: decidir rápido e barato se vale chamar o LLM caro, qual modelo chamar ou como classificar algo, e devolver a confiança para o app perguntar ao usuário quando ela estiver baixa. A conexão já existe em `frontend/src-tauri/src/jev/` (Configurações → Beta). Fica desligada por padrão e aceita também um servidor compatível local, como o Kev, para quem quer o mesmo recurso sem pagar. As sugestões que usam o Jev estão marcadas com **[Jev]**.

---

### 1. Pergunte às suas reuniões (busca semântica + respostas com fonte)
- **O que muda:** uma caixa “O que ficou combinado com o cliente X sobre prazo?” responde com trechos e links para o minuto exato das reuniões.
- **Por que é disruptiva:** hoje a busca (`assist_plus/search.rs`) só conta palavras. Memória pesquisável de todas as reuniões, local, é o que transforma um gravador num segundo cérebro.
- **Esforço:** G. FTS5 no SQLite (P), embeddings locais via Ollama (M) e respostas com citação (M).
- **Custo:** misto. Busca e embeddings locais. Respostas podem usar o provedor de resumo já configurado.
- **Opcional:** a indexação roda em segundo plano e tem um botão para desligar. A resposta por LLM pago só aparece se houver provedor pago configurado.

### 2. Copiloto ao vivo com porteiro: qualquer provedor, poucas chamadas [Jev]
- **O que muda:** a Ajuda ao vivo passa a aceitar Claude, OpenAI e OpenRouter, além do Ollama, sem multiplicar a conta: antes de cada sugestão, o Jev responde “isto merece sugestão agora?”.
- **Por que é disruptiva:** hoje o `live_assist` só fala com o Ollama, e os gatilhos são regex e pausa de 1,5 s. Um filtro calibrado corta a maior parte das chamadas inúteis, e sugestões de modelo de ponta ficam viáveis por centavos.
- **Esforço:** M.
- **Custo:** pago opcional (Jev + LLM). Sem eles, continuam o Ollama e os gatilhos atuais.
- **Opcional:** opção “Porteiro: desligado / heurística local / Jev” e um limite de chamadas por reunião.

### 3. Compromissos que não se perdem entre reuniões [Jev]
- **O que muda:** cada ação extraída vira um item persistente, com dono, prazo e status. Na próxima reunião com o mesmo cliente, o app mostra “3 pendências da última vez”.
- **Por que é disruptiva:** hoje as ações só viram `.ics` (`export_action_ics`) e a detecção é heurística. Fechar o ciclo entre reuniões é o que poucos concorrentes fazem bem.
- **Esforço:** M. Uma tabela nova, uma tela e o vínculo com o `organize` por cliente.
- **Custo:** grátis/local. Com o Jev opcional, `score` de urgência e `noul` para “isto é mesmo um compromisso?”.
- **Opcional:** o rastreamento liga no `feature_toggles` e o Jev só entra se a conexão estiver ligada.

### 4. Briefing automático 5 minutos antes
- **O que muda:** antes da reunião do calendário chega uma notificação com um resumo de uma página: última conversa com o cliente, pendências, decisões e glossário.
- **Por que é disruptiva:** já existem `prepare_from_calendar`, `recall_meetings` e `show_meeting_reminder`, mas nada os junta nem dispara. É preparação sem esforço.
- **Esforço:** M.
- **Custo:** grátis/local (Ollama ou BuiltInAI). Pode usar provedor pago para um texto melhor.
- **Opcional:** desligado por padrão e escolhido por calendário. Funciona sem LLM, com lista de tópicos.

### 5. Follow-up escrito de verdade
- **O que muda:** o e-mail pós-reunião sai pronto no tom certo (formal, direto, comercial), com decisões e próximos passos, e vira rascunho no Mail ou no Outlook.
- **Por que é disruptiva:** hoje o `draft_followup` é um modelo fixo. Esse e-mail é o momento em que o app economiza tempo visível.
- **Esforço:** P.
- **Custo:** misto. Local pelo provedor de resumo. Pago dá texto melhor.
- **Opcional:** botão sob demanda. O modelo fixo atual fica como opção “sem IA”.

### 6. Roteador de custo do resumo [Jev]
- **O que muda:** cada reunião é resumida pelo modelo adequado. Um papo rápido vai para Gemma local, uma negociação de 2 horas vai para Claude. O usuário vê o custo de cada resumo.
- **Por que é disruptiva:** hoje há um provedor fixo para tudo. Qualidade de modelo de ponta com conta de modelo barato.
- **Esforço:** M.
- **Custo:** pago opcional. O Jev (`choice`: simples/média/crítica) decide e o LLM caro só roda quando vale.
- **Opcional:** modo “Automático (econômico)” ao lado do modo atual “Sempre este modelo”, que continua o padrão.

### 7. Guarda de privacidade antes da nuvem [Jev]
- **O que muda:** antes de mandar uma transcrição a um provedor pago, o app mascara CPF, e-mails, telefones, valores e nomes de pacientes, e avisa “4 itens mascarados”.
- **Por que é disruptiva:** dá para usar IA de ponta em reunião sensível, como saúde, jurídico ou RH, sem expor dados. É um diferencial de confiança que concorrentes só na nuvem não têm.
- **Esforço:** M.
- **Custo:** grátis/local (regex + NER local). O Jev opcional (`noul`: “este trecho tem dado sensível?”) pega casos sutis.
- **Opcional:** ligado por padrão só no modo local de detecção. A checagem pelo Jev é opcional e também envia o trecho, por isso o aviso precisa ser claro.

### 8. Exportação e integrações de saída
- **O que muda:** exportar para Markdown, PDF e DOCX, enviar para Notion ou para um cofre do Obsidian, postar o resumo num canal do Slack ou do Teams.
- **Por que é disruptiva:** hoje só existe copiar para a área de transferência, além de `.ics` e rascunho de e-mail. O conteúdo precisa chegar onde o time trabalha.
- **Esforço:** M. Markdown e Obsidian (P) aproveitam o `blocknote-markdown.ts`. PDF, DOCX e APIs (M).
- **Custo:** grátis/local (arquivos). Integrações usam a conta do próprio usuário.
- **Opcional:** cada destino é configurado à parte e nada é enviado sem clique, ou sem regra criada pelo usuário.

### 9. Quem falou, com nomes, em português
- **O que muda:** os falantes aparecem com nome sem renomear à mão: o app cruza os participantes do calendário, a sua voz cadastrada e as vozes conhecidas.
- **Por que é disruptiva:** o CAM++ padrão foi treinado em chinês e inglês. Um modelo melhor para PT-BR com nomeação automática resolve a maior chateação de transcrição.
- **Esforço:** M.
- **Custo:** grátis/local.
- **Opcional:** a nomeação automática pede confirmação na primeira vez de cada pessoa.

### 10. Coach de reunião [Jev]
- **O que muda:** depois de cada reunião, um painel mostra tempo de fala, monólogos, interrupções, perguntas feitas e “o cliente terminou mais animado ou mais frio?”.
- **Por que é disruptiva:** o `meeting_scoreboard` já mede tempo de fala. É o tipo de análise de vendas de ferramentas caras, feita no seu Mac.
- **Esforço:** M.
- **Custo:** grátis/local para as métricas. O Jev (`score` de temperatura do cliente por trecho) é opcional.
- **Opcional:** aba própria, desligada por padrão. Nada sai do computador sem o Jev ligado.

### 11. Destaques ao vivo: decisão, risco, ação [Jev]
- **O que muda:** durante a reunião, uma linha do tempo marca “Decisão”, “Risco”, “Ação” e “Pergunta sem resposta”. No fim, o resumo já parte desses marcos.
- **Por que é disruptiva:** classificar cada trecho com um LLM é caro e lento. Com o Jev (`choice` de 5 opções por trecho), sai barato e com confiança.
- **Esforço:** M.
- **Custo:** pago opcional (Jev). A alternativa local é um modelo pequeno no Ollama, mais lento.
- **Opcional:** desligado por padrão, com escolha “local” ou “Jev”.

### 12. Organização com confiança, e pergunta quando tiver dúvida [Jev]
- **O que muda:** cliente, categoria e tags continuam automáticos, mas, quando a confiança é baixa, o app pergunta “Isto foi com a ACME ou com a Beta?” em vez de chutar.
- **Por que é disruptiva:** o `organize` usa o LLM de resumo e cai num fallback determinístico, sem noção de confiança. O `choice` do Jev devolve probabilidades por opção.
- **Esforço:** P.
- **Custo:** pago opcional (Jev). Fora dele, segue como hoje.
- **Opcional:** opção “Classificar com o Jev” em Organização.

### 13. Gravação automática que não erra [Jev]
- **O que muda:** o app distingue uma reunião de verdade de um vídeo no YouTube ou de um teste de microfone antes de gravar, e passa a funcionar também no Windows e no Linux.
- **Por que é disruptiva:** o `auto_record` usa janelas e processos e só funciona no macOS. Menos falso positivo dá a confiança de deixar no modo “automático”.
- **Esforço:** M (heurística + Windows). G para Linux.
- **Custo:** grátis/local. O Jev (`noul` sobre título da janela e primeiros segundos transcritos) é opcional.
- **Opcional:** os modos continuam Desligado, Perguntar e Automático. O Jev só refina a decisão.

### 14. Resumo pronto no segundo em que a reunião acaba
- **O que muda:** o resumo é montado aos poucos durante a reunião. Ao parar, ele aparece na hora.
- **Por que é disruptiva:** hoje o resumo começa depois da parada e leva minutos em modelo local. Resumo instantâneo muda o hábito de “ler depois”.
- **Esforço:** M.
- **Custo:** grátis/local. Fica mais rápido com provedor pago.
- **Opcional:** opção “Resumo contínuo” desligada em máquinas fracas, conforme o `model_advisor`.

### 15. Agente pós-reunião com fila de aprovação
- **O que muda:** ao terminar, uma fila sugere: “criar 3 tarefas”, “enviar follow-up”, “marcar retorno na terça”. Cada item é aprovado com um clique.
- **Por que é disruptiva:** passa de anotar para executar, sem perder o controle: nada é enviado sem aprovação.
- **Esforço:** G.
- **Custo:** misto.
- **Opcional:** desligado por padrão. Cada tipo de ação é ligado à parte.

### 16. Converse com a reunião aberta
- **O que muda:** perguntas como “o que o João disse sobre orçamento?” sobre a transcrição aberta, com resposta citando o minuto.
- **Por que é disruptiva:** é o recurso mais usado nos concorrentes de nuvem, e aqui funciona offline.
- **Esforço:** P a M (reaproveita o provedor de resumo).
- **Custo:** grátis/local. Pago opcional para respostas melhores.
- **Opcional:** painel recolhível, e o provedor é o mesmo escolhido para resumos.

### 17. Modelo de resumo escolhido sozinho [Jev]
- **O que muda:** o app escolhe o template certo (daily, retrospectiva, venda, padrão) pelo conteúdo, em vez de sempre o padrão.
- **Por que é disruptiva:** os templates existem (`summary/templates`), mas quase ninguém troca. O `choice` do Jev resolve com uma chamada mínima.
- **Esforço:** P.
- **Custo:** pago opcional (Jev). A alternativa local é a pergunta ao modelo do Ollama.
- **Opcional:** “Template: automático / fixo”, e fixo continua o padrão. Junto, corrigir o ID `psychatric_session`.

### 18. Vocabulário que aprende com você
- **O que muda:** nomes de clientes, siglas e jargões passam a sair certos na transcrição, porque o app aprende com as correções que você faz e com o glossário do contexto.
- **Por que é disruptiva:** erros em nomes próprios são a reclamação número um de transcrição local. O glossário já existe no contexto do `live_assist`.
- **Esforço:** M (prompt inicial do Whisper + dicionário de correção pós-transcrição).
- **Custo:** grátis/local.
- **Opcional:** liga e desliga em Transcrição, com lista editável.

### 19. Painel de custo e economia
- **O que muda:** uma tela mostra quanto você gastou por provedor no mês, define um teto e mostra “economia por usar local: US$ X”.
- **Por que é disruptiva:** torna os recursos pagos previsíveis, o que é condição para usar sem medo. Nenhum concorrente mostra isso.
- **Esforço:** P a M.
- **Custo:** grátis/local.
- **Opcional:** sempre visível, sem efeito para quem não usa nada pago.

### 20. Nota de qualidade do resumo [Jev]
- **O que muda:** cada resumo recebe uma checagem: “cobre as decisões?”, “tem dono para cada ação?”. Se falhar, o app sugere refazer com um modelo melhor.
- **Por que é disruptiva:** é controle de qualidade automático, barato com `score`, e alimenta o roteador da sugestão 6.
- **Esforço:** P.
- **Custo:** pago opcional (Jev). A alternativa local é a mesma pergunta ao Ollama.
- **Opcional:** desligado por padrão.

### 21. Legendas e tradução ao vivo
- **O que muda:** uma janela flutuante mostra legendas em português de reuniões em inglês ou espanhol, quase em tempo real.
- **Por que é disruptiva:** a opção de tradução existe nos `feature_toggles`, mas não há legenda. Isso abre o app para reuniões internacionais.
- **Esforço:** M.
- **Custo:** misto. Modelo de tradução local; provedor pago para mais qualidade.
- **Opcional:** só com o botão de legenda ligado.

### 22. Servidor MCP do Meetily
- **O que muda:** Claude Desktop, Claude Code e outros clientes MCP consultam suas reuniões (“liste as pendências da semana”) direto do app, sem copiar e colar.
- **Por que é disruptiva:** o app vira fonte de contexto para todos os agentes do usuário. Hoje não há nenhuma integração de entrada.
- **Esforço:** M.
- **Custo:** grátis/local (servidor em localhost).
- **Opcional:** desligado por padrão, somente leitura e com escolha de quais reuniões expor.

### 23. Termômetro do cliente e alerta de risco [Jev]
- **O que muda:** para cada cliente, uma linha mostra se o tom das conversas está melhorando ou piorando, com alerta quando cai duas reuniões seguidas.
- **Por que é disruptiva:** antecipa churn e problemas de projeto, com dados que o app já tem (cliente vem do `organize`).
- **Esforço:** M.
- **Custo:** pago opcional (Jev `score`, um por reunião, baratíssimo). Local com Ollama.
- **Opcional:** só aparece com o recurso ligado e no mínimo três reuniões com o cliente.

### 24. Resumo semanal por cliente ou projeto
- **O que muda:** toda sexta chega um resumo: temas recorrentes, decisões da semana e pendências envelhecendo.
- **Por que é disruptiva:** passa da reunião isolada para a visão da relação. Usa o contexto por cliente que o `live_assist` já monta.
- **Esforço:** M.
- **Custo:** misto.
- **Opcional:** desligado por padrão, com escolha de dia e clientes.

### 25. Transcrição na nuvem para Mac fraco
- **O que muda:** em máquinas sem memória para o Whisper ou o Parakeet, uma opção usa transcrição em nuvem, como a do Groq, que já aparece no código mas não é usada como provedor.
- **Por que é disruptiva:** leva o app a quem hoje não consegue usá-lo, sem tirar o modo local de quem consegue.
- **Esforço:** M.
- **Custo:** pago opcional.
- **Opcional:** o `model_advisor` só sugere quando o hardware não aguenta. O padrão continua local.

### 26. Webhooks e automação (n8n, Zapier, Make)
- **O que muda:** os eventos “reunião resumida” e “ação criada” disparam um POST com JSON para a URL que o usuário escolher.
- **Por que é disruptiva:** com uma única peça, conecta o app a qualquer CRM ou fluxo, sem integrar um por um.
- **Esforço:** P.
- **Custo:** grátis/local.
- **Opcional:** nenhum webhook vem configurado. Cada um mostra exatamente quais campos envia.

### 27. Clipes de áudio e trechos compartilháveis
- **O que muda:** selecionar um trecho da transcrição gera um clipe de áudio com legenda para mandar a quem não estava.
- **Por que é disruptiva:** “ouça o cliente dizendo isso” convence mais que um resumo. O áudio misto já fica salvo.
- **Esforço:** M.
- **Custo:** grátis/local.
- **Opcional:** ação sob demanda.

### 28. Retenção e consentimento para reuniões sensíveis (LGPD)
- **O que muda:** um perfil “consultório/jurídico” apaga o áudio depois de X dias, mantém só o resumo, registra o consentimento e bloqueia provedores de nuvem.
- **Por que é disruptiva:** abre setores regulados, como saúde, que não podem usar ferramentas de nuvem.
- **Esforço:** M.
- **Custo:** grátis/local.
- **Opcional:** perfil escolhido por reunião ou por categoria.

### 29. Pergunta por voz ao copiloto com atalho
- **O que muda:** segurar o atalho global (que já existe no `live_assist`) e perguntar em voz baixa: “qual era o preço que passamos em março?”. A resposta aparece na janela flutuante.
- **Por que é disruptiva:** dá acesso à memória das reuniões sem tirar os olhos da conversa.
- **Esforço:** M (depende da sugestão 1).
- **Custo:** misto.
- **Opcional:** atalho desativado até ser configurado.

### 30. Sincronização entre seus computadores, cifrada
- **O que muda:** reuniões e resumos aparecem no Mac e no PC, cifrados ponta a ponta, na nuvem do próprio usuário (iCloud Drive, OneDrive ou outra pasta sincronizada).
- **Por que é disruptiva:** mantém a promessa local e resolve o “gravei no outro computador”.
- **Esforço:** G.
- **Custo:** grátis (armazenamento do próprio usuário).
- **Opcional:** desligado por padrão e escolhido por pasta. Sem a chave de cifra, ninguém lê.

---

## Próximo passo sugerido

As sugestões 2, 12 e 17 são as mais baratas para provar o valor do Jev. Já têm o ponto de entrada (`jev::decide_if_enabled`), usam uma chamada mínima e têm fallback local pronto. As sugestões 1, 3 e 19 são a base das outras.
