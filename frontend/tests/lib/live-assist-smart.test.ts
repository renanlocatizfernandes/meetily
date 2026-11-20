import { describe, expect, test } from 'bun:test';
import {
  MAX_ALERTS,
  SmartAlert,
  UsageSnapshot,
  addAlert,
  alertTitle,
  formatTokens,
  parseAlert,
  recapCostNote,
  usageLabel,
  usageTitle,
} from '../../src/lib/liveAssistSmart';

const alert = (over: Partial<SmartAlert> = {}): SmartAlert => ({
  id: 'a1',
  kind: 'pergunta',
  text: 'Qual é o prazo do projeto?',
  ...over,
});

const usage = (over: Partial<UsageSnapshot> = {}): UsageSnapshot => ({
  calls: 3,
  inputTokens: 4200,
  outputTokens: 400,
  grouped: 0,
  paid: true,
  economy: false,
  ...over,
});

describe('parseAlert', () => {
  test('accepts a well formed alert and drops empty optional fields', () => {
    expect(parseAlert({ id: 'x', kind: 'divergencia', text: ' Fica em 15 mil ', speaker: '', detail: 'Na reunião de 12/11: 12 mil' })).toEqual({
      id: 'x',
      kind: 'divergencia',
      text: 'Fica em 15 mil',
      speaker: undefined,
      detail: 'Na reunião de 12/11: 12 mil',
    });
  });

  test('rejects unknown kinds, empty text and non objects', () => {
    expect(parseAlert(null)).toBeNull();
    expect(parseAlert('texto')).toBeNull();
    expect(parseAlert({ id: 'x', kind: 'outro', text: 'a' })).toBeNull();
    expect(parseAlert({ id: 'x', kind: 'tarefa', text: '   ' })).toBeNull();
    expect(parseAlert({ kind: 'tarefa', text: 'a' })).toBeNull();
  });
});

describe('addAlert', () => {
  test('puts the newest first and ignores a repeat of the same kind and text', () => {
    const first = alert();
    const second = alert({ id: 'a2', kind: 'decisao', text: 'Ficou decidido migrar em dezembro.' });
    let list = addAlert([], first);
    list = addAlert(list, second);
    expect(list.map((item) => item.id)).toEqual(['a2', 'a1']);
    expect(addAlert(list, alert({ id: 'a3', text: 'QUAL É O PRAZO DO PROJETO?' }))).toBe(list);
    expect(addAlert(list, alert({ id: 'a1', text: 'outro texto' }))).toBe(list);
    // Same text under another kind is a different alert.
    expect(addAlert(list, alert({ id: 'a4', kind: 'tarefa' }))).toHaveLength(3);
  });

  test('caps the list', () => {
    let list: SmartAlert[] = [];
    for (let index = 0; index < MAX_ALERTS + 5; index += 1) {
      list = addAlert(list, alert({ id: `id${index}`, text: `Pergunta numero ${index}?` }));
    }
    expect(list).toHaveLength(MAX_ALERTS);
    expect(list[0].id).toBe(`id${MAX_ALERTS + 4}`);
  });
});

describe('usage text', () => {
  test('formats token counts', () => {
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(-4)).toBe('0');
    expect(formatTokens(950)).toBe('950');
    expect(formatTokens(1500)).toBe('1,5 mil');
    expect(formatTokens(42000)).toBe('42 mil');
  });

  test('local models say there is no cost', () => {
    expect(usageLabel(null)).toBe('');
    expect(usageLabel(usage({ paid: false, calls: 0 }))).toBe('Modelo local, sem custo');
    expect(usageLabel(usage({ paid: false, calls: 1 }))).toBe('Modelo local, sem custo · 1 chamada');
  });

  test('cloud use shows calls, estimated tokens and the economy flag', () => {
    expect(usageLabel(usage())).toBe('Nuvem · 3 chamadas · ~4,6 mil tokens');
    expect(usageLabel(usage({ economy: true }))).toContain('economia ligada');
    expect(usageTitle(usage({ grouped: 7 }))).toContain('7 disparos agrupados');
    expect(usageTitle(usage({ paid: false }))).toContain('nesta máquina');
  });

  test('a recap names its own cost', () => {
    expect(recapCostNote({ inputTokens: 1200, outputTokens: 100, paid: true })).toBe('Nuvem · ~1,3 mil tokens nesta consulta');
    expect(recapCostNote({ inputTokens: 1200, outputTokens: 100, paid: false })).toBe('Modelo local, sem custo');
  });
});

describe('alertTitle', () => {
  test('covers every kind in pt-BR', () => {
    expect(alertTitle('pergunta')).toBe('Pergunta para você');
    expect(alertTitle('divergencia')).toBe('Possível divergência');
    expect(alertTitle('decisao')).toBe('Decisão');
    expect(alertTitle('tarefa')).toBe('Tarefa');
  });
});
