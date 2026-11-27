'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { FolderOpen, Loader2 } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Skeleton } from '@/components/ui/skeleton';
import { SettingsCard, SettingsStack, SettingSwitch } from '@/components/settings/SettingsCard';
import { errorText } from '@/lib/tauriSafe';
import {
  DEFAULT_WORKSPACE_SETTINGS,
  detectVaults,
  getWorkspaceSettings,
  pickVault,
  saveWorkspaceSettings,
  type VaultInfo,
  type WorkspaceSettings,
} from '@/lib/workspace';

/** Carrega as preferências e grava cada mudança na hora. */
function useWorkspaceSettings() {
  const [settings, setSettings] = useState<WorkspaceSettings>(DEFAULT_WORKSPACE_SETTINGS);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    getWorkspaceSettings().then((value) => {
      if (!active) return;
      setSettings(value);
      setLoading(false);
    });
    return () => {
      active = false;
    };
  }, []);

  const save = useCallback(async (patch: Partial<WorkspaceSettings>) => {
    // Lê a versão gravada para não sobrescrever o que o outro cartão mudou.
    const current = await getWorkspaceSettings();
    const next = { ...current, ...patch };
    setSettings(next);
    try {
      setSettings(await saveWorkspaceSettings(next));
    } catch (error) {
      toast.error('Não foi possível salvar', { description: errorText(error) });
      setSettings(current);
    }
  }, []);

  return { settings, setSettings, loading, save };
}

/** "Quem é você" e o que o app faz sozinho depois de cada reunião. */
export function IdentitySettings() {
  const { settings, setSettings, loading, save } = useWorkspaceSettings();
  const [aliases, setAliases] = useState('');

  useEffect(() => {
    setAliases(settings.userAliases.join(', '));
  }, [settings.userAliases]);

  if (loading) return <Skeleton className="h-44 w-full rounded-xl" />;

  return (
    <SettingsStack>
      <SettingsCard
        title="Você nas reuniões"
        description="Serve para separar o que ficou com você do que você está esperando de outras pessoas."
      >
        <div className="grid gap-4 sm:grid-cols-2">
          <div>
            <Label htmlFor="seu-nome" className="text-sm font-medium">Seu nome</Label>
            <Input
              id="seu-nome"
              className="mt-1.5"
              placeholder="Como chamam você nas reuniões"
              value={settings.userName}
              onChange={(event) => setSettings({ ...settings, userName: event.target.value })}
              onBlur={() => save({ userName: settings.userName })}
            />
          </div>
          <div>
            <Label htmlFor="seus-apelidos" className="text-sm font-medium">Outros nomes</Label>
            <Input
              id="seus-apelidos"
              className="mt-1.5"
              placeholder="Apelidos, separados por vírgula"
              value={aliases}
              onChange={(event) => setAliases(event.target.value)}
              onBlur={() => save({ userAliases: aliases.split(',').map((item) => item.trim()).filter(Boolean) })}
            />
          </div>
        </div>
      </SettingsCard>

      <SettingsCard title="Depois de cada reunião">
        <div className="divide-y">
          <SettingSwitch
            id="tarefas-automaticas"
            label="Extrair tarefas"
            hint="Quando o resumo termina, lista o que ficou combinado, com responsável e prazo."
            checked={settings.autoTasks}
            onCheckedChange={(autoTasks) => save({ autoTasks })}
          />
          <SettingSwitch
            id="participantes-automaticos"
            label="Buscar participantes no Calendário"
            hint="Lê os convidados do evento que estava acontecendo no horário da gravação. O macOS pede permissão na primeira vez."
            checked={settings.autoParticipants}
            onCheckedChange={(autoParticipants) => save({ autoParticipants })}
          />
        </div>
      </SettingsCard>
    </SettingsStack>
  );
}

/** Conexão com um cofre do Obsidian. */
export function ObsidianSettings() {
  const { settings, setSettings, loading, save } = useWorkspaceSettings();
  const [vaults, setVaults] = useState<VaultInfo[]>([]);
  const [picking, setPicking] = useState(false);

  useEffect(() => {
    detectVaults().then(setVaults);
  }, []);

  const choose = async () => {
    setPicking(true);
    try {
      const vault = await pickVault();
      if (!vault) return;
      await save({ obsidianVault: vault.path });
      if (!vault.isVault) {
        toast.info('Essa pasta ainda não é um cofre do Obsidian', {
          description: 'As notas serão gravadas mesmo assim. Abra a pasta como cofre no Obsidian para vê-las lá.',
        });
      }
    } catch (error) {
      toast.error('Não foi possível escolher a pasta', { description: errorText(error) });
    } finally {
      setPicking(false);
    }
  };

  if (loading) return <Skeleton className="h-56 w-full rounded-xl" />;

  const connected = settings.obsidianVault !== '';
  const others = vaults.filter((vault) => vault.path !== settings.obsidianVault);

  return (
    <SettingsCard
      title="Obsidian"
      description="Grava cada reunião como uma nota Markdown no seu cofre: resumo, tarefas, participantes e conversa. Tudo local, sem conta."
      action={
        <Button variant="outline" size="sm" onClick={choose} disabled={picking}>
          {picking ? <Loader2 className="animate-spin" /> : <FolderOpen />}
          {connected ? 'Trocar cofre' : 'Escolher cofre'}
        </Button>
      }
    >
      {connected ? (
        <p className="break-all rounded-lg bg-muted px-3 py-2 font-mono text-xs">{settings.obsidianVault}</p>
      ) : (
        <p className="text-[13px] text-muted-foreground">Nenhum cofre conectado.</p>
      )}

      {others.length > 0 && (
        <div className="mt-3">
          <p className="text-[13px] text-muted-foreground">Cofres encontrados neste computador:</p>
          <ul className="mt-1.5 flex flex-wrap gap-1.5">
            {others.map((vault) => (
              <li key={vault.path}>
                <Button variant="outline" size="sm" title={vault.path} onClick={() => save({ obsidianVault: vault.path })}>
                  {vault.name}
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {connected && (
        <>
          <div className="mt-4">
            <Label htmlFor="pasta-obsidian" className="text-sm font-medium">Pasta dentro do cofre</Label>
            <Input
              id="pasta-obsidian"
              className="mt-1.5 max-w-xs"
              value={settings.obsidianFolder}
              onChange={(event) => setSettings({ ...settings, obsidianFolder: event.target.value })}
              onBlur={() => save({ obsidianFolder: settings.obsidianFolder })}
            />
          </div>
          <div className="mt-4 divide-y">
            <SettingSwitch
              id="obsidian-automatico"
              label="Enviar automaticamente"
              hint="Grava a nota quando o resumo da reunião termina."
              checked={settings.obsidianAuto}
              onCheckedChange={(obsidianAuto) => save({ obsidianAuto })}
            />
            <SettingSwitch
              id="obsidian-conversa"
              label="Incluir a conversa"
              hint="A transcrição completa, com o nome de quem falou."
              checked={settings.obsidianTranscript}
              onCheckedChange={(obsidianTranscript) => save({ obsidianTranscript })}
            />
            <SettingSwitch
              id="obsidian-links"
              label="Criar links para cliente e pessoas"
              hint="Usa [[links]] do Obsidian, para as reuniões aparecerem na página de cada cliente e pessoa."
              checked={settings.obsidianLinks}
              onCheckedChange={(obsidianLinks) => save({ obsidianLinks })}
            />
          </div>
          <p className="mt-3 text-[13px] text-muted-foreground">
            O que você escrever abaixo da linha marcada no fim da nota é mantido quando a reunião é enviada de novo.
          </p>
        </>
      )}
    </SettingsCard>
  );
}
