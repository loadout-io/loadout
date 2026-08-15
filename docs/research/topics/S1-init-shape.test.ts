// AC-2 dla S-1: odpowiedź niesie dowód, że ktoś to naprawdę uruchomił, a nie przeczytał --help.
//
// Obserwablą spike'u jest pole `skills` w linii `system/init` [T1 §4.1]. Ten test sprawdza
// kształt tego, co z niej wklejono: musi być poprawnym JSON-em, musi być tablicą, i musi mieć
// DOKŁADNIE tyle elementów, ile mówi niezależnie zapisane `treatment-skills`. Asercja „pole
// jest niepuste" przeszłaby na wpisanym z palca `["alpha","beta"]`, którego nikt nie skopiował
// z wyjścia — a to jest dokładnie ten sposób, w jaki T1 §6.2 musiał oznaczyć schemat Codeksa
// jako [docs]/[3p], i przez który T-18 sparsuje kształt, którego nie ma. Żeby podrobić
// zgodność długości z liczbą, trzeba skłamać dwa razy spójnie.
//
// Plik czytamy przez existsSync(...) ? readFileSync(...) : '' — test ma paść na asercji
// o treści, nie na odczycie (AGENTS.md §2a p. 5).
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const HERE = dirname(fileURLToPath(import.meta.url));
const ANSWER = resolve(HERE, 'S1-skill-subsetting.md');

const VERDICTS = ['flag', 'generated-dir', 'not-possible'];
const DATE = /^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$/;

// ── Format bloku ```answer: `klucz: wartość` w jednej linii albo `klucz: |` i wcięte ciało. ──
// Ten sam parser co w S1-skill-subsetting.test.ts. Powielony świadomie: każde kryterium wskazuje
// dokładnie jeden plik testu (AGENTS.md §2a p. 1), więc plik ma stać sam.

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
    const m = /^([a-z][a-z0-9-]*):[ \t]*(.*)$/.exec(line);
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

// Bez znaku: to jest długość tablicy `skills`. Ten sam parser co w S1-skill-subsetting.test.ts.
function integer(value: string | undefined): number | null {
  if (value === undefined || !/^\d+$/.test(value.trim())) return null;
  return Number.parseInt(value.trim(), 10);
}

// `undefined` znaczy „to się nie sparsowało". JSON-owego `undefined` nie ma, więc sentinel
// nie koliduje z żadną poprawną wartością.
function asJson(value: string | undefined): unknown {
  if (value === undefined || value.trim() === '') return undefined;
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return undefined;
  }
}

function shown(value: string | undefined): string {
  return value === undefined ? '<the key is absent from the answer block>' : JSON.stringify(value);
}

const doc = existsSync(ANSWER) ? readFileSync(ANSWER, 'utf8') : '';
const fields = parseFields(answerBlock(doc));

describe('S-1 — the answer carries evidence that the probe actually ran', () => {
  it('pastes the skills field out of the system/init line as valid JSON', () => {
    const raw = fields.get('init-skills-raw');
    const parsed = asJson(raw);
    expect(
      parsed,
      'init-skills-raw has to be the JSON value of the `skills` key, copied verbatim out of the system/init line; the answer block says: ' +
        shown(raw),
    ).toBeDefined();
    expect(
      Array.isArray(parsed),
      'init-skills-raw has to parse to an array — T-18 and T-13 will read this exact shape and need to know whether its elements are strings or objects; got: ' +
        shown(raw),
    ).toBe(true);
  });

  it('the pasted array is exactly as long as the separately recorded treatment-skills', () => {
    const raw = fields.get('init-skills-raw');
    const parsed = asJson(raw);
    const treatment = integer(fields.get('treatment-skills'));
    expect(
      Array.isArray(parsed),
      'init-skills-raw has to parse to an array before its length can be compared; the answer block says: ' +
        shown(raw),
    ).toBe(true);
    expect(
      treatment,
      'treatment-skills has to be a plain integer for the array length to be checked against it; got: ' +
        shown(fields.get('treatment-skills')),
    ).not.toBeNull();
    expect(
      (parsed as unknown[]).length,
      'init-skills-raw holds a different number of entries than treatment-skills claims, so one of the two was typed rather than copied out of the run',
    ).toBe(treatment);
  });

  it('records the day the numbers were measured', () => {
    // Niezmiennik 24, ta sama racja co przy `cli`: T1 §10 ryzyko 2 — flagi bywają
    // nieudokumentowane i zmieniają się między wydaniami, więc odpowiedź bez daty
    // jest bezużyteczna za trzy tygodnie.
    const date = fields.get('date');
    expect(
      DATE.test(date ?? ''),
      'date has to be the day the probes ran, as YYYY-MM-DD; the answer block says: ' + shown(date),
    ).toBe(true);
  });

  it('describes the directory it built when the answer is a generated directory', () => {
    const verdict = fields.get('verdict');
    if (verdict !== 'generated-dir') {
      // `layout` obowiązuje tylko ten jeden werdykt — ale żeby to rozstrzygnąć, werdykt
      // musi w ogóle być zapisany. Bez tej asercji warunek byłby pusty i test
      // przechodziłby na dokumencie, w którym nie ma nic.
      expect(
        VERDICTS,
        'the verdict decides whether a layout is required, so it has to be recorded first; the answer block says: ' +
          shown(verdict),
      ).toContain(verdict);
      return;
    }
    const layout = fields.get('layout') ?? '';
    expect(
      layout.trim().length,
      'a generated-dir answer has to record the directory structure it built — that structure is what T-18 generates',
    ).toBeGreaterThan(0);
    expect(
      layout,
      'the layout has to show where SKILL.md sits, because the folder name is the only thing that differs between the six vendors (docs/ARCHITECTURE.md §9)',
    ).toContain('SKILL.md');
  });
});
