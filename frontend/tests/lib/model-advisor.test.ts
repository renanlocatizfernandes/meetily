import { describe, expect, test } from 'bun:test';
import {
  actionFor,
  bytesText,
  downloadKey,
  fitText,
  hardwareText,
  memoryText,
  summariesUseOllama,
  usedInSummaries,
  type AdvisorModel,
  type AdvisorReport,
} from '../../src/lib/modelAdvisor';

function model(overrides: Partial<AdvisorModel> = {}): AdvisorModel {
  return {
    id: 'gemma4:e4b',
    role: 'assistant',
    engine: 'ollama',
    name: 'Gemma 4 E4B',
    about: '',
    downloadBytes: 6_583_656_264,
    memoryMb: 7_924,
    license: 'Apache-2.0',
    licenseNote: null,
    source: 'https://ollama.com/library/gemma4',
    installed: false,
    active: false,
    downloading: false,
    fit: 'fits',
    ...overrides,
  };
}

function report(overrides: Partial<AdvisorReport> = {}): AdvisorReport {
  return {
    hardware: { ramGb: 16, totalMemoryBytes: 17_179_869_184, chip: 'Apple M2', appleSilicon: true, cpuCores: 8, freeDiskBytes: 120_000_000_000 },
    language: 'pt',
    tier: 'medium',
    budgetMb: 10_100,
    totalMemoryMb: 9_774,
    voiceModelEnabled: true,
    ollama: { endpoint: 'http://localhost:11434', running: true, installedModels: [] },
    roles: [{ role: 'assistant', current: { provider: 'ollama', model: 'llama3.2:latest', name: 'Llama 3.2 3B', inCatalog: true }, recommended: model(), reason: '', others: [] }],
    ...overrides,
  };
}

describe('model advisor text', () => {
  test('sizes use decimal units and Brazilian separators', () => {
    expect(bytesText(51_223_453)).toBe('51 MB');
    expect(bytesText(672_858_975)).toBe('673 MB');
    expect(bytesText(8_021_618_699)).toBe('8,0 GB');
    expect(bytesText(0)).toBe('');
    expect(bytesText(null)).toBe('');
    expect(memoryText(1_500)).toBe('1,5 GB');
    expect(memoryText(undefined)).toBe('');
  });

  test('the machine line shows chip, memory and free disk', () => {
    expect(hardwareText(report().hardware)).toBe('Apple M2 · 16 GB de memória · 120,0 GB livres em disco');
    expect(hardwareText({ ...report().hardware, chip: '', freeDiskBytes: null })).toBe('16 GB de memória');
  });

  test('every reason a model does not fit has a sentence without a dash', () => {
    for (const fit of ['tooLittleMemory', 'doesNotFitTogether', 'needsAppleSilicon', 'otherLanguage', 'noDiskSpace'] as const) {
      expect(fitText(fit).length).toBeGreaterThan(10);
      expect(fitText(fit)).not.toContain('—');
    }
    expect(fitText('fits')).toBe('');
  });
});

describe('model advisor actions', () => {
  test('nothing is downloaded or switched without a button: the action only names it', () => {
    expect(actionFor(model(), report(), 'llama3.2:latest')).toBe('download');
    expect(actionFor(model({ installed: true }), report(), 'llama3.2:latest')).toBe('use');
    expect(actionFor(model({ installed: true }), report(), 'gemma4:e4b')).toBe('active');
  });

  test('an Ollama model needs Ollama running to be downloaded', () => {
    const stopped = report({ ollama: { endpoint: 'http://localhost:11434', running: false, installedModels: [] } });
    expect(actionFor(model(), stopped, null)).toBe('needsOllama');
  });

  test('a model that does not fit is not offered for download', () => {
    expect(actionFor(model({ fit: 'doesNotFitTogether' }), report(), null)).toBe('unavailable');
    expect(actionFor(model({ fit: 'tooLittleMemory', installed: true }), report(), null)).toBe('use');
  });

  test('speech and voices follow the active flag from the core', () => {
    const speech = model({ id: 'parakeet-tdt-0.6b-v3-int8', role: 'speech', engine: 'parakeet', installed: true, active: true });
    expect(actionFor(speech, report(), 'parakeet-tdt-0.6b-v3-int8')).toBe('active');
    expect(actionFor({ ...speech, active: false }, report())).toBe('use');
    expect(downloadKey('parakeet', speech.id)).toBe('parakeet:parakeet-tdt-0.6b-v3-int8');
  });

  test('summaries are looked at separately from the live help', () => {
    expect(summariesUseOllama(report())).toBe(true);
    expect(usedInSummaries(model(), report())).toBe(false);
    expect(usedInSummaries(model({ id: 'llama3.2:latest' }), report())).toBe(true);
    const cloud = report({ roles: [{ role: 'assistant', current: { provider: 'claude', model: 'x', name: 'x', inCatalog: false }, recommended: model(), reason: '', others: [] }] });
    expect(summariesUseOllama(cloud)).toBe(false);
  });
});
