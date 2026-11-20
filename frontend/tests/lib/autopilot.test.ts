import { describe, expect, test } from 'bun:test';
import {
  DEFAULT_AUTOPILOT_SETTINGS,
  costNotice,
  emailToText,
  hasContent,
  listToText,
  normalizeResult,
  normalizeSettings,
  providerLabel,
  tasksToText,
  type AutopilotStatus,
} from '../../src/lib/autopilot';

const status = (patch: Partial<AutopilotStatus>): AutopilotStatus => ({
  provider: 'openai',
  model: 'gpt-4o',
  paid: true,
  hasCredentials: true,
  willUse: 'configured',
  willUseProvider: 'openai',
  willUseModel: 'gpt-4o',
  message: null,
  ...patch,
});

describe('autopilot settings', () => {
  test('is off by default with every step on', () => {
    expect(DEFAULT_AUTOPILOT_SETTINGS.enabled).toBe(false);
    expect(normalizeSettings(undefined)).toEqual(DEFAULT_AUTOPILOT_SETTINGS);
    expect(normalizeSettings({ enabled: 'yes', stepEmail: false })).toEqual({
      ...DEFAULT_AUTOPILOT_SETTINGS,
      stepEmail: false,
    });
  });
});

describe('costNotice', () => {
  test('warns about credits for a paid provider', () => {
    const notice = costNotice(status({}));
    expect(notice?.tone).toBe('warning');
    expect(notice?.text).toContain('consome créditos');
    expect(notice?.text).toContain('OpenAI');
  });

  test('says nothing for a local provider', () => {
    expect(costNotice(status({ provider: 'ollama', paid: false }))).toBeNull();
    expect(costNotice(null)).toBeNull();
  });

  test('explains the local fallback and the nothing-to-do case discreetly', () => {
    const fallback = costNotice(status({ hasCredentials: false, willUse: 'local_fallback', willUseModel: 'llama3.2:latest' }));
    expect(fallback?.tone).toBe('info');
    expect(fallback?.text).toContain('llama3.2:latest');
    const none = costNotice(status({ hasCredentials: false, willUse: 'none', message: 'Nada disponível.' }));
    expect(none).toEqual({ tone: 'info', text: 'Nada disponível.' });
  });
});

describe('normalizeResult', () => {
  test('keeps valid fields and drops malformed ones', () => {
    const result = normalizeResult({
      meetingId: 'm1',
      status: 'done',
      tasks: [{ text: 'Enviar proposta', owner: 'Ana', due: null }, { text: '' }, 7],
      decisions: ['Plano B', 3, ''],
      pending: [],
      suggestedTitle: 'Revisão do plano',
      suggestedTags: ['plano'],
      emailSubject: 'Resumo',
      emailBody: 'Olá',
      steps: { summary: 'done' },
    });
    expect(result?.tasks).toEqual([{ text: 'Enviar proposta', owner: 'Ana', due: null }]);
    expect(result?.decisions).toEqual(['Plano B']);
    expect(result?.steps.summary).toBe('done');
    expect(normalizeResult(null)).toBeNull();
    expect(normalizeResult({ status: 'done' })).toBeNull();
  });

  test('unknown status becomes failed', () => {
    expect(normalizeResult({ meetingId: 'm1', status: 'weird' })?.status).toBe('failed');
  });
});

describe('hasContent', () => {
  test('hides an empty finished result but shows a note or a running one', () => {
    expect(hasContent(null)).toBe(false);
    expect(hasContent(normalizeResult({ meetingId: 'm1', status: 'done' }))).toBe(false);
    expect(hasContent(normalizeResult({ meetingId: 'm1', status: 'skipped', note: 'Sem chave.' }))).toBe(true);
    expect(hasContent(normalizeResult({ meetingId: 'm1', status: 'running' }))).toBe(true);
  });
});

describe('copy text', () => {
  test('formats tasks, lists and the e-mail draft', () => {
    expect(
      tasksToText([
        { text: 'Enviar proposta', owner: 'Ana', due: '2026-12-01' },
        { text: 'Revisar contrato', owner: null, due: null },
      ]),
    ).toBe('- Enviar proposta (Ana) até 2026-12-01\n- Revisar contrato');
    expect(listToText('Decisões', ['A', 'B'])).toBe('Decisões\n- A\n- B');
    expect(listToText('Decisões', [])).toBe('');
    expect(emailToText({ emailSubject: 'Resumo', emailBody: 'Olá' })).toBe('Assunto: Resumo\n\nOlá');
    expect(emailToText({ emailSubject: null, emailBody: 'Olá' })).toBe('Olá');
  });

  test('provider labels fall back to the raw name', () => {
    expect(providerLabel('claude')).toBe('Claude (Anthropic)');
    expect(providerLabel('xyz')).toBe('xyz');
    expect(providerLabel(null)).toBe('');
  });
});
