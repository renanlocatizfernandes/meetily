import { describe, expect, test } from 'bun:test';
import { hourRange, matchesAgendaQuery, placeInLanes, toAgendaItems } from '../../src/lib/agenda';
import type { MeetingOrg } from '../../src/lib/organize';
import {
  buildTurns,
  dueState,
  formatClock,
  formatDuration,
  startOfWeek,
  toDayKey,
  weekDays,
} from '../../src/lib/workspace';

function meeting(id: string, local: Date, minutes: number | null, extra: Partial<MeetingOrg> = {}): MeetingOrg {
  return {
    id,
    title: `Reunião ${id}`,
    createdAt: local.toISOString(),
    client: null,
    project: null,
    category: null,
    tags: [],
    titleSource: 'auto',
    classificationSource: 'none',
    pinned: false,
    durationSeconds: minutes === null ? null : minutes * 60,
    ...extra,
  };
}

describe('datas da agenda', () => {
  test('a semana começa na segunda e tem sete dias', () => {
    const sunday = new Date(2026, 10, 29, 15, 0);
    expect(toDayKey(startOfWeek(sunday))).toBe('2026-11-23');
    const days = weekDays(new Date(2026, 10, 27));
    expect(days.map(toDayKey)).toEqual([
      '2026-11-23', '2026-11-24', '2026-11-25', '2026-11-26', '2026-11-27', '2026-11-28', '2026-11-29',
    ]);
  });

  test('duração e relógio', () => {
    expect(formatDuration(null)).toBe('');
    expect(formatDuration(20)).toBe('<1 min');
    expect(formatDuration(45 * 60)).toBe('45 min');
    expect(formatDuration(3600)).toBe('1 h');
    expect(formatDuration(5400)).toBe('1 h 30 min');
    expect(formatClock(65)).toBe('01:05');
    expect(formatClock(3725)).toBe('1:02:05');
  });

  test('prazo: atrasada, hoje, em breve, depois', () => {
    const today = new Date(2026, 10, 27, 10, 0);
    const state = (dueDate: string | null, status: 'open' | 'done' = 'open') => dueState({ dueDate, status }, today);
    expect(state('2026-11-26')).toBe('overdue');
    expect(state('2026-11-27')).toBe('today');
    expect(state('2026-12-03')).toBe('soon');
    expect(state('2026-12-20')).toBe('later');
    expect(state(null)).toBe('none');
    expect(state('2026-11-01', 'done')).toBe('none');
  });
});

describe('grade da agenda', () => {
  test('reunião cai no dia e hora locais e sem duração ganha meia hora', () => {
    const [item] = toAgendaItems([meeting('a', new Date(2026, 10, 27, 14, 30), null)]);
    expect(item.dayKey).toBe('2026-11-27');
    expect(item.startHour).toBe(14.5);
    expect(item.hours).toBe(0.5);
  });

  test('data inválida fica de fora', () => {
    expect(toAgendaItems([{ ...meeting('a', new Date(), 10), createdAt: '' }])).toEqual([]);
  });

  test('sobrepostas dividem a coluna; as seguintes voltam à largura toda', () => {
    const items = toAgendaItems([
      meeting('a', new Date(2026, 10, 27, 9, 0), 60),
      meeting('b', new Date(2026, 10, 27, 9, 30), 60),
      meeting('c', new Date(2026, 10, 27, 14, 0), 30),
    ]);
    const placed = placeInLanes(items);
    const by = Object.fromEntries(placed.map((item) => [item.meeting.id, item]));
    expect([by.a.lane, by.a.lanes]).toEqual([0, 2]);
    expect([by.b.lane, by.b.lanes]).toEqual([1, 2]);
    expect([by.c.lane, by.c.lanes]).toEqual([0, 1]);
  });

  test('faixa de horas estica para caber reuniões cedo e tarde', () => {
    expect(hourRange([])).toEqual({ from: 8, to: 19 });
    const items = toAgendaItems([
      meeting('a', new Date(2026, 10, 27, 6, 15), 30),
      meeting('b', new Date(2026, 10, 27, 21, 0), 90),
    ]);
    expect(hourRange(items)).toEqual({ from: 6, to: 23 });
  });

  test('busca ignora acentos e olha cliente, etiquetas e participantes', () => {
    const item = meeting('a', new Date(), 30, { client: 'Unimed', tags: ['migração'] });
    expect(matchesAgendaQuery(item, [], 'migracao unimed')).toBe(true);
    expect(matchesAgendaQuery(item, ['João Souza'], 'joao')).toBe(true);
    expect(matchesAgendaQuery(item, [], 'joao')).toBe(false);
    expect(matchesAgendaQuery(item, [], '   ')).toBe(true);
  });
});

describe('conversa', () => {
  const segment = (id: string, timestamp: number, speakerKey: string, text: string, speaker?: string) => ({
    id, timestamp, speakerKey, text, speaker,
  });

  test('falas seguidas da mesma voz viram um balão; o microfone é você', () => {
    const turns = buildTurns([
      segment('1', 0, 'mic', 'Bom dia.'),
      segment('2', 4, 'mic', 'Vamos começar?'),
      segment('3', 9, 'system-1', 'Vamos.', 'Ana'),
      segment('4', 12, 'mic', '  '),
      segment('5', 15, 'system-1', 'Tenho uma dúvida.', 'Ana'),
    ], 'Renan');
    expect(turns.map((turn) => [turn.speaker, turn.mine, turn.lines.length])).toEqual([
      ['Renan', true, 2],
      ['Ana', false, 2],
    ]);
  });

  test('pausa longa abre balão novo e voz sem nome vira Participante', () => {
    const turns = buildTurns([
      segment('1', 0, 'system', 'Um.'),
      segment('2', 120, 'system', 'Dois.'),
    ]);
    expect(turns.length).toBe(2);
    expect(turns[0].speaker).toBe('Participante');
    expect(turns[0].mine).toBe(false);
  });
});
