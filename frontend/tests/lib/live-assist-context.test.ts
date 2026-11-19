import { describe, expect, test } from 'bun:test';
import {
  LiveAssistContextSummary,
  contextHeadline,
  countGlossaryTerms,
  originLabel,
  originNeedsCare,
  originTitle,
  parseOrigin,
  pastMeetingsStatus,
  profileStatus,
} from '../../src/lib/liveAssistContext';
import { applyItemAction, itemKey } from '../../src/lib/liveAssistSync';

const summary: LiveAssistContextSummary = {
  meetingName: 'Aurora · renovação',
  client: 'Aurora',
  project: 'Portal',
  category: 'Cliente',
  tags: ['contrato'],
  speakers: ['Você', 'Ana'],
  pastMeetings: 2,
  memoryNotes: 6,
  usePastMeetings: true,
  profileEnabled: true,
  profileFilled: true,
  glossaryTerms: 3,
  localOnlyBlocked: false,
};

describe('live assist origin', () => {
  test('accepts a well formed origin and ignores anything else', () => {
    expect(parseOrigin({ kind: 'reuniao', label: ' Reunião de 12/11 ', detail: 'Aurora', meetingId: 'm2' })).toEqual({
      kind: 'reuniao',
      label: 'Reunião de 12/11',
      detail: 'Aurora',
      meetingId: 'm2',
    });
    expect(parseOrigin({ kind: 'perfil', label: 'Perfil' })).toEqual({ kind: 'perfil', label: 'Perfil' });
    expect(parseOrigin(undefined)).toBeUndefined();
    expect(parseOrigin('perfil')).toBeUndefined();
    expect(parseOrigin({ kind: 'perfil' })).toBeUndefined();
    expect(parseOrigin({ kind: 'perfil', label: '   ' })).toBeUndefined();
  });

  test('labels and tooltips say where the help came from', () => {
    expect(originLabel({ kind: 'glossario', label: 'Glossário', detail: 'SLA' })).toBe('Glossário · SLA');
    expect(originLabel({ kind: 'reuniao', label: 'Reunião de 12/11', detail: 'Aurora' })).toBe('Reunião de 12/11');
    expect(originTitle({ kind: 'reuniao', label: 'Reunião de 12/11', detail: 'Aurora' })).toContain('Aurora');
    expect(originTitle({ kind: 'internet', label: 'Wikipédia', detail: 'https://pt.wikipedia.org/wiki/SLA' })).toContain('https://');
    expect(originNeedsCare({ kind: 'modelo', label: 'Sem fonte local' })).toBe(true);
    expect(originNeedsCare({ kind: 'perfil', label: 'Perfil' })).toBe(false);
    expect(originNeedsCare({ kind: 'novo', label: 'Outro' })).toBe(true);
  });

  test('the origin does not change the key shared between windows', () => {
    const plain = { section: 'contexto', text: 'SLA é o acordo de nível de serviço.' };
    const tagged = { ...plain, origin: { kind: 'glossario', label: 'Glossário' } };
    expect(itemKey(tagged)).toBe(itemKey(plain));
    const item = { id: '1', section: 'contexto' as const, text: plain.text, pinned: false, createdAt: 1, origin: tagged.origin };
    const pinned = applyItemAction([item], { action: 'pin', key: itemKey(item), origin: 'main', item });
    expect(pinned[0].pinned).toBe(true);
    expect(pinned[0].origin).toEqual(tagged.origin);
  });
});

describe('context in use', () => {
  test('headline is compact', () => {
    expect(contextHeadline(summary)).toBe('Aurora · Portal · 2 reuniões anteriores · perfil ligado');
    expect(contextHeadline({ ...summary, project: null, pastMeetings: 1 })).toBe('Aurora · 1 reunião anterior · perfil ligado');
    expect(contextHeadline({ ...summary, client: null, project: null, pastMeetings: 0, profileEnabled: false })).toBe('perfil desligado');
    expect(contextHeadline({ ...summary, localOnlyBlocked: true })).toBe('Aurora · Portal · modelo na nuvem');
    expect(contextHeadline(null)).toBe('Sem contexto ainda');
  });

  test('status lines cover every case', () => {
    expect(profileStatus({ ...summary, profileFilled: false, glossaryTerms: 0 })).toBe('perfil vazio');
    expect(profileStatus({ ...summary, profileFilled: false, glossaryTerms: 2 })).toBe('perfil ligado');
    expect(pastMeetingsStatus(summary)).toBe('2 reuniões, 6 anotações');
    expect(pastMeetingsStatus({ ...summary, pastMeetings: 1, memoryNotes: 1 })).toBe('1 reunião, 1 anotação');
    expect(pastMeetingsStatus({ ...summary, usePastMeetings: false })).toBe('Desligadas');
    expect(pastMeetingsStatus({ ...summary, client: null })).toBe('Cliente ainda não identificado');
    expect(pastMeetingsStatus({ ...summary, pastMeetings: 0 })).toBe('Nenhuma reunião salva deste cliente');
  });

  test('glossary lines are counted like the app reads them', () => {
    const glossary = 'SLA: acordo de nível de serviço\nFinOps = gestão de custo de nuvem\nlinha sem separador\n- RPO - ponto de recuperação\nsla: repetido\nvazio:';
    expect(countGlossaryTerms(glossary)).toBe(3);
    expect(countGlossaryTerms('')).toBe(0);
  });
});
