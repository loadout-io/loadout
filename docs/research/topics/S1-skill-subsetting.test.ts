// AC-1 dla S-1: „podzbiór" jest zmierzoną liczbą mniejszą od kontroli, a nie zdaniem w prozie.
//
// Ten plik leży obok odpowiedzi, a nie w src/, bo odpowiedź jest jedynym artefaktem tego
// spike'u (TASK.md §„Co to zadanie posiada", niezmiennik 21). Czyta blok ```answer
// z sąsiedniego S1-skill-subsetting.md i porównuje LICZBY. Asercja `toContain("verdict:")`
// przeszłaby na dokumencie, w którym kontrola i próba mają tę samą wartość — czyli na
// dokumencie, który nie zmierzył niczego, tylko wszedł do katalogu, w którym i tak były
// dwie umiejętności (niezmiennik 20).
//
// Dokument czytamy przez existsSync(...) ? readFileSync(...) : '' — celowo, i to nie jest
// ostrożność, tylko wymóg. Test, który przewraca się na odczycie pliku, przewraca się PRZED
// asercją, a bramka zna taki upadek jako czerwień, która nic nie uruchomiła (AGENTS.md §2a
// p. 5). Ten ma paść na treści.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const HERE = dirname(fileURLToPath(import.meta.url));
const ANSWER = resolve(HERE, 'S1-skill-subsetting.md');
const REL = 'docs/research/topics/S1-skill-subsetting.md';

const VERDICTS = ['flag', 'generated-dir', 'not-possible'];
const UI_CONSEQUENCES = ['only-these', 'all-or-none'];

// Niezmiennik 21: blok ```answer ma DOKŁADNIE te klucze, których szuka któryś z testów.
// To jest unia obu specyfikacji — S1-init-shape.test.ts czyta `date`, `init-skills-raw`
// i `layout`, więc stoją tutaj, choć ten plik ich nie używa. Cokolwiek poza tą listą jest
// kluczem, którego nigdy nikt nie odczyta; jego miejsce jest w prozie nad blokiem.
const KNOWN_KEYS = [
  'verdict',
  'cli',
  'date',
  'control-command',
  'treatment-command',
  'control-skills',
  'treatment-skills',
  'ui-consequence',
  'init-skills-raw',
  'layout',
];

// Sekrety w linii poleceń zapisanej potem w dokumencie (niezmiennik 9). Łamie się to cicho,
// przez skopiowanie całej linii z historii powłoki razem z przedrostkiem `ANTHROPIC_API_KEY=`.
const SECRET = /sk-ant-|_API_KEY|--api-key|Authorization:/i;

// Wzorzec z AC-1, nieukotwiony — celowo. `which claude` w T1 §3 zwraca ścieżkę, więc autor
// ma prawo zapisać `/Users/…/bin/claude 2.1.233`, a odrzucenie tego byłoby czerwienią
// z powodu, którego kryterium nie stawia.
const CLI = /claude \d+\.\d+\.\d+/;

// ── Format bloku ```answer: `klucz: wartość` w jednej linii albo `klucz: |` i wcięte ciało. ──

function answerBlocks(markdown: string): string[] {
  const lines = markdown.split(/\r?\n/);
  const blocks: string[] = [];
  for (let i = 0; i < lines.length; i++) {
    if ((lines[i] ?? '').trim() !== '```answer') continue;
    for (let j = i + 1; j < lines.length; j++) {
      if ((lines[j] ?? '').trim() === '```') {
        blocks.push(lines.slice(i + 1, j).join('\n'));
        i = j;
        break;
      }
    }
  }
  return blocks;
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

// Bez znaku: to jest długość tablicy `skills`, więc ujemna wartość nie jest „liczbą, której
// nie lubimy", tylko wpisem, którego żaden bieg nie mógł wyprodukować. Odrzucamy też `2 (alpha,
// beta)` — komentarz przy liczbie należy do prozy, nie do pola, które czyta maszyna.
function integer(value: string | undefined): number | null {
  if (value === undefined || !/^\d+$/.test(value.trim())) return null;
  return Number.parseInt(value.trim(), 10);
}

function shown(value: string | undefined): string {
  return value === undefined ? '<the key is absent from the answer block>' : JSON.stringify(value);
}

const doc = existsSync(ANSWER) ? readFileSync(ANSWER, 'utf8') : '';
const blocks = answerBlocks(doc);
const fields = parseFields(blocks[0] ?? '');

describe('S-1 — the subset claim is a measured number, not a sentence', () => {
  it('carries exactly one machine-readable answer block', () => {
    expect(
      blocks.length,
      REL + ' has to carry exactly one ```answer block — that block is what T-13 and T-18 quote',
    ).toBe(1);
  });

  it('records a verdict from the closed set the UI branches on', () => {
    const verdict = fields.get('verdict');
    expect(
      VERDICTS,
      'verdict has to be one of flag / generated-dir / not-possible; the answer block says: ' +
        shown(verdict),
    ).toContain(verdict);
  });

  it('names the CLI build the numbers came from', () => {
    const cli = fields.get('cli');
    // Niezmiennik 24: T1 §10 ryzyko 2 mówi wprost, że flagi bywają nieudokumentowane i zmieniają
    // się między wydaniami. Bez wersji ta odpowiedź jest bezużyteczna za trzy tygodnie.
    expect(
      CLI.test(cli ?? ''),
      'cli has to be the output of `claude --version`, e.g. "claude 2.1.233"; the answer block says: ' +
        shown(cli),
    ).toBe(true);
  });

  it('records two different claude commands — the control and the treatment', () => {
    const control = fields.get('control-command');
    const treatment = fields.get('treatment-command');
    expect(
      (control ?? '').startsWith('claude '),
      'control-command has to be the probe as it was actually typed, starting with "claude "; got: ' +
        shown(control),
    ).toBe(true);
    expect(
      (treatment ?? '').startsWith('claude '),
      'treatment-command has to be the probe as it was actually typed, starting with "claude "; got: ' +
        shown(treatment),
    ).toBe(true);
    expect(
      treatment,
      'control-command and treatment-command are the same line, so both numbers describe one run and one of them was transcribed rather than measured',
    ).not.toBe(control);
  });

  it('records commands that carry no secret', () => {
    // Niezmiennik 9. Wymóg „zaczyna się od `claude `" już odcina przedrostek z eksportem
    // zmiennej; ta asercja łapie klucz podany jako wartość flagi.
    for (const key of ['control-command', 'treatment-command']) {
      const value = fields.get(key) ?? '';
      // Puste pole nie jest „czyste" — nie ma w nim czego szukać. Bez tej linii test
      // przechodzi na dokumencie, w którym żadnej komendy nie zapisano.
      expect(
        value.length,
        key + ' has to be recorded before it can be read for credentials',
      ).toBeGreaterThan(0);
      expect(SECRET.test(value), key + ' carries a credential and this document is committed').toBe(
        false,
      );
    }
  });

  it('records control and treatment as plain integers', () => {
    const control = fields.get('control-skills');
    const treatment = fields.get('treatment-skills');
    expect(
      integer(control),
      'control-skills has to be the length of the `skills` array the control run reported; got: ' +
        shown(control),
    ).not.toBeNull();
    expect(
      integer(treatment),
      'treatment-skills has to be the length of the `skills` array the treatment run reported; got: ' +
        shown(treatment),
    ).not.toBeNull();
  });

  it('measures a proper subset: 0 < treatment-skills < control-skills', () => {
    const verdict = fields.get('verdict');
    const control = integer(fields.get('control-skills'));
    const treatment = integer(fields.get('treatment-skills'));

    if (verdict === 'not-possible') {
      // Bez porównania, świadomie. TASK.md §„Dwie rzeczy, które trzeba rozstrzygnąć": mechanizm
      // wymagający przepisania ~/.claude/skills jest globalną zmianą maszyny użytkownika,
      // więc odpowiedź brzmi `not-possible`, CHOĆBY liczba wyszła. Odwrotna asercja
      // (treatment >= control) też byłaby fałszem — `--plugin-dir` może DOŁOŻYĆ dwie
      // umiejętności do kompletu użytkownika i dać treatment > control, co jest prawdziwym
      // wynikiem „to nie zawęża". Zostaje wymóg, żeby obie liczby były zmierzone.
      expect(control, 'a not-possible answer still has to record what the control run saw').not.toBeNull();
      expect(
        treatment,
        'a not-possible answer still has to record what the treatment run saw',
      ).not.toBeNull();
      return;
    }

    expect(control, 'the subset claim needs a control number to be smaller than').not.toBeNull();
    expect(treatment, 'the subset claim needs a treatment number').not.toBeNull();
    expect(
      treatment as number,
      'treatment-skills is zero, which is "nothing", not a subset — that is the M1/M2 outcome and its verdict is not-possible',
    ).toBeGreaterThan(0);
    expect(
      treatment as number,
      'treatment-skills is not below control-skills, so the probe proves only that the directory held that many skills all along — the false yes this spike exists to catch',
    ).toBeLessThan(control as number);
  });

  it('records a ui-consequence from the closed set', () => {
    const ui = fields.get('ui-consequence');
    expect(
      UI_CONSEQUENCES,
      'ui-consequence has to be only-these or all-or-none; the answer block says: ' + shown(ui),
    ).toContain(ui);
  });

  it('ties the UI consequence to the verdict: all-or-none exactly when not-possible', () => {
    const verdict = fields.get('verdict');
    const ui = fields.get('ui-consequence');
    // Obie wartości muszą najpierw BYĆ i należeć do swoich zbiorów. Zmierzone na pierwszym
    // biegu warstwy before (2026-08-15): bez tych dwóch linii równoważność jest prawdziwa dla
    // dwóch nieobecnych pól (false === false), więc jedyny test łapiący sprzeczność werdyktu
    // z konsekwencją dla UI przechodził na dokumencie, w którym nie ma nic.
    expect(
      VERDICTS,
      'the equivalence needs a recorded verdict to compare against; got: ' + shown(verdict),
    ).toContain(verdict);
    expect(
      UI_CONSEQUENCES,
      'the equivalence needs a recorded ui-consequence to compare against; got: ' + shown(ui),
    ).toContain(ui);
    // `verdict: flag` obok `ui-consequence: all-or-none` to dwa zdania, które nie mogą być
    // prawdziwe naraz, a asercja o obecności stringa przepuszcza je oba. „only-these" to lista
    // checkboxów, którą buduje T-13; „all-or-none" to wiersz, który T-13 ma wtedy ukryć,
    // zamiast pokazywać kontrolkę bez handlera (niezmiennik 16).
    expect(
      ui === 'all-or-none',
      'verdict ' +
        shown(verdict) +
        ' and ui-consequence ' +
        shown(ui) +
        ' contradict each other: all-or-none belongs to a not-possible answer and to no other',
    ).toBe(verdict === 'not-possible');
  });

  it('carries no key that no spec reads', () => {
    // Niezmiennik 21. Klucz dopisany „na wszelki wypadek" to klucz, którego nikt nigdy
    // nie przeczyta; opis mechanizmu, kosztów i wariantów należy do prozy nad blokiem.
    const extra = [...fields.keys()].filter((k) => !KNOWN_KEYS.includes(k));
    expect(
      extra,
      'the answer block carries keys no spec reads: ' +
        extra.join(', ') +
        ' — prose above the block is where that belongs',
    ).toEqual([]);
  });
});
