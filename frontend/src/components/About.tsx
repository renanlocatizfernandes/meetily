import React, { useState, useEffect } from "react";
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import Image from 'next/image';
import AnalyticsConsentSwitch from "./AnalyticsConsentSwitch";
import { UpdateDialog } from "./UpdateDialog";
import { updateService, UpdateInfo } from '@/services/updateService';
import { Button } from './ui/button';
import { Loader2, CheckCircle2 } from 'lucide-react';
import { toast } from 'sonner';


export function About() {
    const [currentVersion, setCurrentVersion] = useState<string>('1.3.0');
    const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
    const [isChecking, setIsChecking] = useState(false);
    const [showUpdateDialog, setShowUpdateDialog] = useState(false);

    useEffect(() => {
        // Get current version on mount
        getVersion().then(setCurrentVersion).catch(console.error);
    }, []);

    const handleContactClick = async () => {
        try {
            await invoke('open_external_url', { url: 'https://meetily.zackriya.com/#about' });
        } catch (error) {
            console.error('Failed to open link:', error);
        }
    };

    const handleCheckForUpdates = async () => {
        setIsChecking(true);
        try {
            const info = await updateService.checkForUpdates(true);
            setUpdateInfo(info);
            if (info.available) {
                setShowUpdateDialog(true);
            } else {
                toast.success('Você já está na versão mais recente');
            }
        } catch (error: any) {
            console.error('Failed to check for updates:', error);
            toast.error('Não foi possível verificar atualizações: ' + (error.message || 'erro desconhecido'));
        } finally {
            setIsChecking(false);
        }
    };

    return (
        <div className="p-4 space-y-4 h-[80vh] overflow-y-auto">
            {/* Compact Header */}
            <div className="text-center">
                <div className="mb-3">
                    <Image
                        src="icon_128x128.png"
                        alt="Logo do Meetily Live Plus"
                        width={64}
                        height={64}
                        className="mx-auto"
                    />
                </div>
                {/* <h1 className="text-xl font-bold text-foreground">Meetily</h1> */}
                <span className="text-sm text-muted-foreground"> v{currentVersion}</span>
                <p className="text-base text-muted-foreground mt-1">
                    Notas e resumos em tempo real que não saem do seu computador.
                </p>
                <p className="mt-2 text-sm text-foreground">
                    Versão modificada por <span className="font-semibold">Renan Locatiz Fernandes</span>, da <span className="font-semibold">Tech Fernandes</span>.
                </p>
                <p className="text-xs text-muted-foreground">
                    Não é a versão oficial do Meetily.
                </p>
                <div className="mt-3">
                    <Button
                        onClick={handleCheckForUpdates}
                        disabled={isChecking}
                        variant="outline"
                        size="sm"
                        className="text-xs"
                    >
                        {isChecking ? (
                            <>
                                <Loader2 className="h-3 w-3 mr-2 animate-spin" />
                                Verificando...
                            </>
                        ) : (
                            <>
                                <CheckCircle2 className="h-3 w-3 mr-2" />
                                Verificar atualizações
                            </>
                        )}
                    </Button>
                    {updateInfo?.available && (
                        <div className="mt-2 text-xs text-brand">
                            Atualização disponível: v{updateInfo.version}
                        </div>
                    )}
                </div>
            </div>

            {/* Features Grid - Compact */}
            <div className="space-y-3">
                <h2 className="text-sm font-medium text-foreground">O que torna o Meetily Live Plus diferente</h2>
                <div className="grid grid-cols-2 gap-2">
                    <div className="bg-muted rounded p-3 hover:bg-muted transition-colors">
                        <h3 className="font-bold text-sm text-foreground mb-1">Privacidade em primeiro lugar</h3>
                        <p className="text-xs text-muted-foreground leading-relaxed">Seus dados e o processamento de IA podem ficar no seu ambiente. Sem nuvem, sem vazamentos.</p>
                    </div>
                    <div className="bg-muted rounded p-3 hover:bg-muted transition-colors">
                        <h3 className="font-bold text-sm text-foreground mb-1">Use qualquer modelo</h3>
                        <p className="text-xs text-muted-foreground leading-relaxed">Prefere um modelo local de código aberto? Ótimo. Quer usar uma API externa? Também pode. Sem aprisionamento.</p>
                    </div>
                    <div className="bg-muted rounded p-3 hover:bg-muted transition-colors">
                        <h3 className="font-bold text-sm text-foreground mb-1">Custo sob controle</h3>
                        <p className="text-xs text-muted-foreground leading-relaxed">Evite cobrança por minuto rodando modelos no seu computador (ou pague só pelas chamadas que escolher).</p>
                    </div>
                    <div className="bg-muted rounded p-3 hover:bg-muted transition-colors">
                        <h3 className="font-bold text-sm text-foreground mb-1">Funciona em qualquer lugar</h3>
                        <p className="text-xs text-muted-foreground leading-relaxed">Google Meet, Zoom, Teams: online ou offline.</p>
                    </div>
                </div>
            </div>

            {/* Coming Soon - Compact */}
            <div className="bg-brand-soft rounded p-3">
                <p className="text-sm text-brand">
                    <span className="font-bold">Em breve:</span> uma biblioteca de agentes de IA locais para automatizar follow-ups, acompanhar tarefas e muito mais.
                </p>
            </div>

            {/* CTA Section - Compact */}
            <div className="text-center space-y-2">
                <h3 className="text-sm font-medium text-foreground">Quer levar sua empresa mais longe?</h3>
                <p className="text-sm text-muted-foreground">
                    Se você planeja criar agentes de IA sob medida, com privacidade em primeiro lugar, ou um produto feito para a sua <span className="font-bold">empresa</span>, a Zackriya pode ajudar.
                </p>
                <Button type="button" size="sm" onClick={handleContactClick}>
                    Falar com a equipe da Zackriya
                </Button>
            </div>

            {/* Footer - Compact */}
            <div className="pt-2 border-t border-border text-center">
                <p className="text-xs text-muted-foreground/80">
                    Meetily Live Plus: modificado por Renan Locatiz Fernandes (Tech Fernandes).
                    <br />
                    Baseado no Meetily, projeto de código aberto da Zackriya Solutions.
                </p>
            </div>
            <AnalyticsConsentSwitch />

            {/* Update Dialog */}
            <UpdateDialog
                open={showUpdateDialog}
                onOpenChange={setShowUpdateDialog}
                updateInfo={updateInfo}
            />
        </div>

    )
}