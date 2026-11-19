-- A ajuda ao vivo passa a vir desligada. Quem quiser usar liga em
-- Configurações → Ajuda ao vivo (ou pelo atalho). Roda uma única vez:
-- depois disso a escolha do usuário é respeitada.
UPDATE live_assist_settings SET enabled = 0;
