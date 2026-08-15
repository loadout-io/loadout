// AC-2 dla S-2: decyzja wynika z dowodu, a nie z nadziei. „Używamy flagi" jest niedopuszczalne
// przy `enforced: no` — to jest dokładnie ta odpowiedź, przed którą ostrzega ARCHITECTURE §11,
// i to ona kończy się suwakiem „ile tur", który nie robi nic (niezmiennik 16).
//
// Trzy pola muszą się zgadzać naraz: `decision`, oba `*.enforced` i `agent-field`. Sprawdzenie
// samego `decision` przechodzi na dokumencie, który mówi „egzekwowania nie potwierdziliśmy, ale
// i tak wystawimy suwak tur"; wymuszona implikacja między tymi trzema polami nie przechodzi.
// Zdania „przy enforced: no w kreatorze agenta NIE MA pola ile tur" ten test celowo nie szuka
// w prozie — `toContain('enforced')` byłoby asercją o obecności stringa (niezmiennik 20).
// Maszynowym zapisem tego zdania jest pole `agent-field`, i to ono jest tu sprawdzane.
//
// Kształt bloku ```answer opisuje sąsiedni S2-turn-and-budget-flags.test.ts; ten plik czyta
// z niego `decision`, `agent-field` i oba `*.enforced`.
//
// Plik czytamy przez existsSync(...) ? readFileSync(...) : '' — test ma paść na asercji
// o treści, nie na odczycie (AGENTS.md §2a p. 5, TASK.md §„Jak zaczerwienić before uczciwie").
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const HERE = dirname(fileURLToPath(import.meta.url));
const ANSWER = resolve(HERE, 'S2-turn-and-budget-flags.md');

// Zamknięty zbiór z AC-2. Uwaga na jego kształt: nie ma w nim wariantu „tylko budżet", więc
// wynik `max-budget-usd.enforced: yes` przy `max-turns.enforced: no` nie ma tu poprawnej
// decyzji — `use-max-turns` i `use-both` wymagają tur, a `wall-clock-only` wymaga, żeby ŻADNA
// flaga nie była egzekwowana. Zgłoszone człowiekowi przy pisaniu specyfikacji (2026-08-15,
// AGENTS.md §7): jeśli sonda tak wypadnie, ten zbiór musi dostać czwartą wartość, zanim
// dokument da się napisać uczciwie. Do tego czasu test odwzorowuje kryterium dosłownie —
// rozszerzenie zbioru na własną rękę byłoby luzowaniem kryterium, a nie jego spełnieniem.
const DECISIONS = ['wall-clock-only', 'use-max-turns', 'use-both'];

// Pole, które T-11 przepisze do kreatora agenta jeden do jednego.
const AGENT_FIELDS = ['none', 'turns', 'budget', 'turns+budget'];

const ENFORCED = ['yes', 'no', 'not-tested'];

const TURNS = 'max-turns.enforced';
const BUDGET = 'max-budget-usd.enforced';

// ── Format bloku ```answer: `klucz: wartość` w jednej linii albo `klucz: |` i wcięte ciało. ──
// Ten sam parser co w S2-turn-and-budget-flags.test.ts. Powielony świadomie: każde kryterium
// wskazuje dokładnie jeden plik testu (AGENTS.md §2a p. 1), więc plik ma stać sam.

function answerBlock(markdown: string): string {
  const lines = markdown.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    if ((lines[i] ?? '').trim() !== '```answer') continue;
    for (let j = i + 1; j < lines.length; j++) {
      if ((lines[j] ?? '').trim() === '```') return lines.slice(i + 1, j).join('\n');
    }
  }
  return '';
}

function dedent(lines: string[]): string {
  const body = lines.filter((l) => l.trim() !== '');
  if (body.length === 0) return '';
  const indent = Math.min(...body.map((l) => l.length - l.trimStart().length));
  return lines
    .map((l) => l.slice(indent))
    .join('\n')
    .trim();
}

function parseFields(block: string): Map<string, string> {
  const out = new Map<string, string>();
  const lines = block.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? '';
    const m = /^([a-z][a-z0-9-]*(?:\.[a-z][a-z0-9-]*)*):[ \t]*(.*)$/.exec(line);
    if (m === null) continue;
    const key = m[1] ?? '';
    const inline = (m[2] ?? '').trim();
    if (inline !== '|') {
      out.set(key, inline);
      continue;
    }
    const body: string[] = [];
    while (i + 1 < lines.length) {
      const next = lines[i + 1] ?? '';
      if (next.trim() !== '' && next.trimStart() === next) break;
      body.push(next);
      i++;
    }
    out.set(key, dedent(body));
  }
  return out;
}

function shown(value: string | undefined): string {
  return value === undefined ? '<the key is absent from the answer block>' : JSON.stringify(value);
}

const fields = parseFields(answerBlock(existsSync(ANSWER) ? readFileSync(ANSWER, 'utf8') : ''));

describe('S-2 — the decision follows from the evidence, not from hope', () => {
  it('records a decision from the closed set the plan branches on', () => {
    const decision = fields.get('decision');
    expect(
      DECISIONS,
      'decision has to be one of wall-clock-only / use-max-turns / use-both; the answer block says: ' +
        shown(decision),
    ).toContain(decision);
  });

  it('records an agent-field from the closed set the agent editor branches on', () => {
    const field = fields.get('agent-field');
    expect(
      AGENT_FIELDS,
      'agent-field has to be one of none / turns / budget / turns+budget — it is the sentence T-11 copies into the agent editor; the answer block says: ' +
        shown(field),
    ).toContain(field);
  });

  it('the decision agrees with what the two enforced fields measured', () => {
    const decision = fields.get('decision');
    const turns = fields.get(TURNS);
    const budget = fields.get(BUDGET);

    // Trzy strażniki, bez których implikacja jest prawdziwa dla trzech nieobecnych pól i cały
    // ten test przechodzi na pustym dokumencie. Zmierzone na S-1 (2026-08-15): równoważność
    // dwóch `undefined` jest prawdą, więc jedyny test łapiący sprzeczność przechodził na
    // dokumencie, w którym nie było nic.
    expect(
      DECISIONS,
      'the implication needs a recorded decision to check; the answer block says: ' +
        shown(decision),
    ).toContain(decision);
    expect(
      ENFORCED,
      'the implication needs a recorded ' + TURNS + '; the answer block says: ' + shown(turns),
    ).toContain(turns);
    expect(
      ENFORCED,
      'the implication needs a recorded ' + BUDGET + '; the answer block says: ' + shown(budget),
    ).toContain(budget);

    if (decision === 'use-max-turns') {
      expect(
        turns,
        'decision is use-max-turns while ' +
          TURNS +
          ' is ' +
          shown(turns) +
          ': building on a flag whose stopping was never observed is the outcome ARCHITECTURE §11 forbids',
      ).toBe('yes');
      return;
    }

    if (decision === 'use-both') {
      expect(
        [turns, budget],
        'decision is use-both, so both flags have to be recorded as enforced; the answer block says ' +
          TURNS +
          '=' +
          shown(turns) +
          ' and ' +
          BUDGET +
          '=' +
          shown(budget),
      ).toEqual(['yes', 'yes']);
      return;
    }

    // wall-clock-only: żadna flaga nie może być egzekwowana. Odwrotna strona tej samej monety —
    // dokument, który zmierzył działający limit i mimo to odkłada go na półkę, też jest
    // niezgodny z dowodem, tyle że w drugą stronę.
    expect(
      [turns, budget].filter((v) => v === 'yes'),
      'decision is wall-clock-only, yet an enforced flag is recorded (' +
        TURNS +
        '=' +
        shown(turns) +
        ', ' +
        BUDGET +
        '=' +
        shown(budget) +
        '): a limit the run demonstrably obeys is evidence for using it, not against',
    ).toEqual([]);
  });

  it('carries no agent field exactly when the decision is wall-clock-only', () => {
    const decision = fields.get('decision');
    const field = fields.get('agent-field');
    // Znowu obie wartości muszą najpierw BYĆ i należeć do swoich zbiorów — patrz strażniki wyżej.
    expect(
      DECISIONS,
      'the equivalence needs a recorded decision to compare against; the answer block says: ' +
        shown(decision),
    ).toContain(decision);
    expect(
      AGENT_FIELDS,
      'the equivalence needs a recorded agent-field to compare against; the answer block says: ' +
        shown(field),
    ).toContain(field);
    // To jest to jedno zdanie, które T-11 przepisze bez myślenia: `agent-field: none` znaczy,
    // że w kreatorze agenta NIE MA pola „ile tur". `wall-clock-only` obok `agent-field: turns`
    // to kontrolka bez handlera przebrana za limit (niezmiennik 16) — a `use-max-turns` obok
    // `agent-field: none` to zmierzony limit, którego użytkownik nie ma jak ustawić.
    expect(
      field === 'none',
      'decision ' +
        shown(decision) +
        ' and agent-field ' +
        shown(field) +
        ' contradict each other: none belongs to a wall-clock-only answer and to no other',
    ).toBe(decision === 'wall-clock-only');
  });
});
