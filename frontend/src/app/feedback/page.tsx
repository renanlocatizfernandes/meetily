'use client';

import { ArrowLeft, ArrowUpRight } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

const cards = [
  {
    title: 'Relatar um erro ou sugerir um recurso',
    description: 'Relate um erro ou sugira um recurso no GitHub. Seu feedback ajuda a melhorar o Meetily.',
    links: [{ label: 'Abrir issues no GitHub', url: 'https://github.com/Zackriya-Solutions/meetily/issues/new/choose' }],
  },
  {
    title: 'Entre no nosso Discord',
    description: 'Tire dúvidas, peça ajuda e troque ideias com quem usa e desenvolve o Meetily.',
    links: [{ label: 'Entrar no servidor', url: 'https://discord.gg/crRymMQBFH' }],
  },
  {
    title: 'Siga nas redes sociais',
    description: 'Siga o Meetily para ver novidades do produto e conversar com a comunidade.',
    links: [
      { label: 'X', url: 'https://x.com/meetilyai/', icon: 'x' },
      { label: 'Reddit', url: 'https://www.reddit.com/r/meetily/', icon: 'reddit' },
      { label: 'LinkedIn', url: 'https://www.linkedin.com/company/106363062/', icon: 'linkedin' },
      { label: 'YouTube', url: 'https://www.youtube.com/@meetily', icon: 'youtube' },
    ],
  },
  {
    title: 'Avaliando o Meetily para a sua equipe?',
    description: 'Conheça as opções de implantação e suporte para usar o Meetily em toda a organização.',
    links: [{ label: 'Ver opções para equipes', url: 'https://meetily.ai/enterprise?utm_source=open-source' }],
  },
  {
    title: 'Precisa de mais do Meetily?',
    description: 'Tenha identificação de participantes, detecção automática de reuniões e acesso por API, CLI e MCP com o Meetily PRO.',
    links: [{ label: 'Conhecer o PRO', url: 'https://meetily.ai/pro?utm_source=open-source' }],
  },
];

async function openExternalLink(url: string) {
  try {
    await invoke('open_external_url', { url });
  } catch (error) {
    console.error('Failed to open feedback link:', error);
    toast.error('Não foi possível abrir o link. Confira o navegador padrão e tente de novo.');
  }
}

export default function FeedbackPage() {
  const router = useRouter();

  return (
    <div className="h-full overflow-y-auto bg-background custom-scrollbar">
      <div className="mx-auto w-full max-w-[880px] px-8 py-6">
        <button
          type="button"
          onClick={() => router.back()}
          className="flex items-center gap-2 rounded text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
        >
          <ArrowLeft aria-hidden="true" className="h-4 w-4" />
          Voltar
        </button>
        <h1 className="mt-5 mb-6 text-lg font-semibold tracking-tight text-foreground">Enviar feedback</h1>

        <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,300px),1fr))] gap-5">
          {cards.map(card => (
            <article key={card.title} className="flex min-h-40 flex-col rounded-xl border border-border bg-card p-5 pb-3">
              <h2 className="text-base font-semibold leading-6 text-foreground">{card.title}</h2>
              <p className="mt-2 text-sm leading-5 text-muted-foreground">{card.description}</p>
              <div className="mt-auto flex flex-wrap items-center gap-x-6">
                {card.links.map(link => (
                  <button
                    key={link.url}
                    type="button"
                    onClick={() => void openExternalLink(link.url)}
                    aria-label={`${link.label}`}
                    title={`${link.label}`}
                    className={`inline-flex min-h-11 items-center justify-center gap-1 rounded text-sm font-semibold text-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 ${'icon' in link ? 'min-w-11' : ''}`}
                  >
                    {'icon' in link ? (
                      <svg aria-hidden="true" focusable="false" className="h-5 w-5">
                        <use href={`/social-icons.svg#${link.icon}`} />
                      </svg>
                    ) : (
                      <>
                        {link.label}
                        <ArrowUpRight aria-hidden="true" className="h-3 w-3" />
                      </>
                    )}
                  </button>
                ))}
              </div>
            </article>
          ))}
        </div>
      </div>
    </div>
  );
}
