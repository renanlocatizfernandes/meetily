use super::window::TranscriptWindow;

pub fn system_prompt() -> &'static str {
    "Você é um assistente de reunião ao vivo e ajuda o dono da reunião. Responda APENAS com um objeto JSON, sem markdown e sem texto fora do JSON. O objeto tem exatamente estas chaves: {\"perguntas\":[],\"respostas\":[],\"contexto\":[]}. Regras: no máximo 2 strings por lista; cada string tem no máximo 25 palavras; perguntas são perguntas que a pessoa do microfone (rótulo Você ou fonte mic) deveria fazer agora; respostas são sugestões curtas apenas quando outra pessoa (qualquer rótulo que não seja Você nem mic, como Pessoa 1, Participantes ou system) fez uma pergunta, e devem deixar claro para quem a resposta serve; ficam vazias se ninguém perguntou; contexto explica um termo, empresa, produto, tecnologia, norma, número ou pessoa que o dono possa não conhecer, de preferência com a fonte recebida, e fica vazio se não houver o que explicar; não invente um link que não esteja nas fontes; não invente fatos ausentes do trecho, das fontes ou do contexto da reunião; não repita sugestões já listadas; se a fala recente não for português, a resposta pode sair no idioma da reunião e o contexto pode começar com Tradução: seguido do sentido em pt-BR; escreva as strings no idioma pedido."
}

pub fn user_prompt(
    language: &str,
    meeting_context: &str,
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
    format!(
        "Idioma das sugestões: {language}\n\nContexto da reunião:\n{context}\n\nResumo do que ficou de fora da janela recente:\n{earlier}\n\nTrecho recente (o rótulo entre colchetes é quem falou; Você ou mic é o usuário):\n{recent}\nFontes consultadas na internet (só o termo saiu da máquina; use o link se explicar um item de contexto):\n{sources}\nSugestões já mostradas:\n{shown}\n",
        language = if language.trim().is_empty() { "pt-BR" } else { language.trim() },
        context = if meeting_context.trim().is_empty() { "(não informado)" } else { meeting_context.trim() },
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

    #[test]
    fn prompt_carries_language_context_and_sources() {
        let mut window = TranscriptWindow::default();
        window.push(TranscriptPiece {
            at: 1.0,
            source: "system".to_string(),
            text: "O SLA é de 99,9%.".to_string(),
        });
        let prompt = user_prompt("pt-BR", "Cliente Aurora, fechar proposta", &window, &[], "");
        assert!(prompt.contains("pt-BR"));
        assert!(prompt.contains("Cliente Aurora"));
        assert!(prompt.contains("[system] O SLA é de 99,9%."));
        assert!(system_prompt().contains("\"perguntas\""));
    }
}
