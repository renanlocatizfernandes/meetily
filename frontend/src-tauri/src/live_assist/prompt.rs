use super::window::TranscriptWindow;

/// Ceiling of the system prompt, checked by a test.
#[cfg(test)]
pub const SYSTEM_PROMPT_MAX_CHARS: usize = 1600;

pub fn system_prompt() -> &'static str {
    "Você é um assistente de reunião ao vivo e ajuda o dono da reunião. Responda APENAS com um objeto JSON, sem markdown e sem texto fora do JSON. O objeto tem exatamente estas chaves e nenhuma outra: {\"perguntas\":[],\"respostas\":[],\"contexto\":[]}. Cada lista tem no máximo 2 strings e cada string tem no máximo 25 palavras. \
contexto é a lista mais importante: explique em uma frase um termo, sigla, pessoa, empresa, produto, número ou combinado anterior que apareceu na fala recente, usando o glossário, o perfil do dono, as reuniões anteriores e as fontes da internet recebidas. Diga a origem na própria frase, por exemplo: Na reunião de 12/11 ficou pendente..., Pelo glossário, SLA é..., Segundo a Wikipédia... Não copie os blocos nem liste o que não foi falado agora; se nada da fala recente tiver explicação nos blocos, contexto fica vazio. \
perguntas são as que a pessoa do microfone (rótulo Você ou mic) deveria fazer agora, de preferência sobre pendências e decisões anteriores. respostas só existem quando outra pessoa (qualquer rótulo que não seja Você nem mic, como Pessoa 1, Participantes ou system) fez uma pergunta; dizem para quem servem e usam o perfil do dono quando a pergunta é sobre a empresa ou os serviços dele. Não invente um link que não esteja nas fontes; não invente fatos ausentes do trecho e dos blocos; não repita sugestões já listadas; se a fala recente não for português, a resposta pode sair no idioma da reunião e o contexto pode começar com Tradução: seguido do sentido em pt-BR; escreva as strings no idioma pedido."
}

/// `auto_context` is the block built by `context::render_context` (meeting
/// data, owner profile, glossary, earlier meetings). The recent transcript
/// goes last so a small model reads it right before answering.
pub fn user_prompt(
    language: &str,
    meeting_context: &str,
    auto_context: &str,
    window: &TranscriptWindow,
    already_shown: &[String],
    sources: &str,
) -> String {
    let mut recent = String::new();
    for piece in window.recent() {
        recent.push_str(&format!("[{}] {}\n", piece.source, piece.text.trim()));
    }
    let shown = if already_shown.is_empty() {
        "(nenhuma)".to_string()
    } else {
        already_shown
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let auto = if auto_context.trim().is_empty() {
        String::new()
    } else {
        format!("{}\n\n", auto_context.trim())
    };
    format!(
        "Idioma das sugestões: {language}\n\nContexto da reunião (pauta):\n{context}\n\n{auto}Resumo do que ficou de fora da janela recente:\n{earlier}\n\nFontes da internet (use o link se explicar um item de contexto):\n{sources}\n\nSugestões já mostradas:\n{shown}\n\nTrecho recente (o rótulo entre colchetes é quem falou; Você ou mic é o usuário):\n{recent}\n",
        language = if language.trim().is_empty() { "pt-BR" } else { language.trim() },
        context = if meeting_context.trim().is_empty() { "(não informado)" } else { meeting_context.trim() },
        auto = auto,
        earlier = if window.earlier_summary().trim().is_empty() { "(nada)" } else { window.earlier_summary().trim() },
        recent = if recent.trim().is_empty() { "(vazio)" } else { recent.trim() },
        sources = if sources.trim().is_empty() { "(nenhuma)" } else { sources.trim() },
        shown = shown,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_assist::window::{TranscriptPiece, TranscriptWindow};

    fn window() -> TranscriptWindow {
        let mut window = TranscriptWindow::default();
        window.push(TranscriptPiece {
            at: 1.0,
            source: "system".to_string(),
            text: "O SLA é de 99,9%.".to_string(),
        });
        window
    }

    #[test]
    fn prompt_carries_language_context_and_sources() {
        let prompt = user_prompt("pt-BR", "Cliente Aurora, fechar proposta", "", &window(), &[], "");
        assert!(prompt.contains("pt-BR"));
        assert!(prompt.contains("Cliente Aurora"));
        assert!(prompt.contains("[system] O SLA é de 99,9%."));
        assert!(system_prompt().contains("\"perguntas\""));
        // No automatic context: no empty block is left behind.
        assert!(!prompt.contains("\n\n\n"));
    }

    #[test]
    fn system_prompt_asks_for_grounded_context_with_origin() {
        let system = system_prompt();
        assert!(system.contains("contexto é a lista mais importante"));
        // Plain strings: asking a 3B model for objects made it ramble and break the JSON.
        assert!(!system.contains("origem\""));
        for label in ["glossário", "perfil do dono", "reunião de 12/11", "Wikipédia", "reuniões anteriores"] {
            assert!(system.contains(label), "missing origin label {label}");
        }
        assert!(system.contains("não invente fatos"));
        // Paid on every call: keep it short for a 3B model on an 8 GB machine.
        assert!(system.chars().count() <= SYSTEM_PROMPT_MAX_CHARS, "{}", system.chars().count());
        // No dash punctuation in text the model may copy to the owner.
        assert!(!system.contains('—') && !system.contains('–'));
    }

    #[test]
    fn automatic_context_sits_before_the_transcript() {
        let auto = "Dados desta reunião:\nCliente: Aurora | Projeto: Portal\n\nReuniões anteriores com Aurora:\n- [reunião de 12/11] Pendência: enviar a proposta";
        let prompt = user_prompt(
            "pt-BR",
            "Renovação",
            auto,
            &window(),
            &["Qual é o prazo?".to_string()],
            "- [Wikipédia] SLA é um acordo https://pt.wikipedia.org/wiki/SLA",
        );
        let position = |needle: &str| prompt.find(needle).unwrap_or_else(|| panic!("missing {needle}"));
        assert!(position("Contexto da reunião (pauta)") < position("Dados desta reunião"));
        assert!(position("[reunião de 12/11] Pendência") < position("Fontes da internet"));
        assert!(position("[Wikipédia]") < position("Sugestões já mostradas"));
        assert!(position("- Qual é o prazo?") < position("Trecho recente (o rótulo"));
        assert!(prompt.trim_end().ends_with("[system] O SLA é de 99,9%."));
    }
}
