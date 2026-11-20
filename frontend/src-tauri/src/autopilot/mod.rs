//! Piloto automatico pos-reuniao (opcional, desligado por padrao).
//!
//! Quando a gravacao termina e a conversa e salva, e se o piloto estiver
//! ligado, roda em segundo plano, etapa por etapa (cada uma com o seu
//! interruptor): resumo, tarefas/decisoes/pendencias, titulo e etiquetas, e
//! rascunho de e-mail de follow-up. Nunca envia nada. O texto da conversa vai
//! para o mesmo provedor configurado para resumos; sem chave, usa o modelo
//! local se houver, senao nao faz nada.

pub mod commands;
mod model;
mod parse;
mod plan;
mod prompt;
mod service;
mod store;

pub use model::{AutopilotResult, AutopilotSettings, AutopilotStatus};
pub use service::schedule;
