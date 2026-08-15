// AC-1 dla S-2: dla KAŻDEJ z dwóch flag osobno — czy parser ją przyjmuje i czy bieg naprawdę
// się o nią zatrzymał. To są dwa różne pytania i cały spike istnieje dlatego, że w logu
// wyglądają identycznie: flaga przyjęta i po cichu zignorowana kończy bieg tak samo jak flaga,
// która zadziałała — `result` mówi `success` i nikt nie patrzy na `num_turns` (TASK.md, nagłówek).
//
// Ten plik leży obok odpowiedzi, a nie w src/, bo odpowiedź jest jedynym artefaktem tego spike'u
// (TASK.md §„Co to zadanie posiada", niezmiennik 21). Czyta blok ```answer z sąsiedniego
// S2-turn-and-budget-flags.md.
//
// Czego ten test świadomie NIE robi: nie sprawdza, że w dokumencie „jest napisane enforced"
// (niezmiennik 20). Słaba asercja `expect(md).toMatch(/enforced:\s*(yes|no)/)` przechodzi na
// dokumencie, w którym kontrola wzięła jedną turę — czyli na najczęstszym błędzie tego pomiaru:
// odpalasz `--max-turns 1` na promptcie, który i tak potrzebował jednej tury, widzisz
// `num_turns: 1` i zapisujesz „egzekwowane". Stąd dwa wymagania, których słaba asercja nie ma:
// kontrola sama w sobie musi być ≥ 2 tury i > $0,001, a każde `enforced: yes` musi cytować
// NAZWANE pole ze zdarzenia `result`, nie komunikat parsera.
//
// Kształt bloku, który ten plik i S2-decision.test.ts czytają (klucze płaskie, kropka jest
// częścią nazwy, nie zagnieżdżeniem):
//
//   cli: claude 2.1.233
//   date: 2026-08-15
//   control-turns: 2
//   control-cost: 0.0124
//   max-turns.accepted: yes
//   max-turns.enforced: yes
//   max-turns.evidence: num_turns=1 subtype=error_max_turns terminal_reason=…
//   max-budget-usd.accepted: yes
//   max-budget-usd.enforced: no
//   max-budget-usd.evidence:
//   decision: use-max-turns
//   agent-field: turns
//
// Dokument czytamy przez existsSync(...) ? readFileSync(...) : '' — celowo, i to nie jest
// ostrożność, tylko wymóg. Test, który przewraca się na odczycie pliku, przewraca się PRZED
// asercją, a bramka zna taki upadek jako czerwień, która nic nie uruchomiła (AGENTS.md §2a p. 5,
// TASK.md §„Jak zaczerwienić before uczciwie"). Ten ma paść na treści.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const HERE = dirname(fileURLToPath(import.meta.url));
const ANSWER = resolve(HERE, 'S2-turn-and-budget-flags.md');
const REL = 'docs/research/topics/S2-turn-and-budget-flags.md';

// Prefiksy z AC-1. Obie flagi dostają ten sam komplet trzech pól, bo obie mają ten sam sposób
// skłamania: parser je zna, a bieg i tak leci do końca.
const FLAGS = ['max-turns', 'max-budget-usd'];

const ACCEPTED = ['yes', 'no'];
// `not-tested` jest wartością pełnoprawną: sonda, której nie dało się przeprowadzić, jest
// uczciwym wynikiem, a `no` znaczy „uruchomiliśmy i bieg się NIE zatrzymał". Zlanie tych dwóch
// w jedno `no` jest tym, przed czym ostrzega ARCHITECTURE §11 — niepewność przebrana za pomiar.
const ENFORCED = ['yes', 'no', 'not-tested'];

// Pola zdarzenia `result` [T1 §4.4]. Dowód ma cytować nazwę pola RAZEM z wartością (`num_turns=1`),
// bo tylko wtedy widać, czy liczba pochodzi z biegu, czy z komunikatu parsera. `is_error` celowo
// nie jest na liście: sam w sobie nie odróżnia zatrzymania na limicie od błędu sieci, a T1 §4.4
// pokazuje bieg z `subtype:"success"` i `is_error:true` naraz. Ma prawo stać w dowodzie obok
// któregoś z tych czterech, tylko nie zamiast niego.
const RESULT_FIELD = /(num_turns|subtype|terminal_reason|total_cost_usd)=/;

// Wzorzec z AC-1, nieukotwiony — celowo, tak samo jak w S-1: `which claude` zwraca ścieżkę,
// więc `/Users/…/bin/claude 2.1.233` jest poprawnym zapisem tego, co autor widział.
const CLI = /claude \d+\.\d+\.\d+/;
const DATE = /^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$/;

// Próg z TASK.md §„Jak to zmierzyć”: to jest wartość podana do `--max-budget-usd` w sondzie.
// Kontrola tańsza od capa nie testuje capa — bieg zmieściłby się w budżecie i bez flagi.
const CAP_USD = 0.001;
// Bieg haiku z lean flagami kosztował $0,0124 [T1 §3.3], więc cap 0,001 jest realny — ale to
// jest cytat, nie pomiar, i dlatego ta stała nie występuje w żadnej asercji.

// Minimum z TASK.md: prompt MUSI wziąć co najmniej dwie tury (model woła narzędzie, potem musi
// się odezwać z jego wynikiem). Kontrola z jedną turą unieważnia całą próbę `--max-turns`.
const CONTROL_TURNS_MIN = 2;

// Niezmiennik 9: w dokumencie ląduje pełna linia poleceń sondy. Łamie się to cicho, przez
// skopiowanie jej z historii powłoki razem z przedrostkiem `ANTHROPIC_API_KEY=`.
const SECRET = /sk-ant-|_API_KEY|--api-key|Authorization:/i;

// Niezmiennik 21: blok ```answer ma DOKŁADNIE te klucze, których szuka któryś z testów.
// To jest unia obu specyfikacji — `decision` i `agent-field` czyta S2-decision.test.ts, więc
// stoją tutaj, choć ten plik ich nie używa. Cokolwiek poza tą listą jest kluczem, którego nigdy
// nikt nie odczyta; opis metody, kosztów i wariantów należy do prozy nad blokiem. Tam też,
// nie tutaj, ląduje pełna linia poleceń każdej z trzech sond.
const KNOWN_KEYS = [
  'cli',
  'date',
  'control-turns',
  'control-cost',
  'max-turns.accepted',
  'max-turns.enforced',
  'max-turns.evidence',
  'max-budget-usd.accepted',
  'max-budget-usd.enforced',
  'max-budget-usd.evidence',
  'decision',
  'agent-field',
];

// ── Format bloku ```answer: `klucz: wartość` w jednej linii albo `klucz: |` i wcięte ciało. ──
// Parser jest ten sam co w S1-*.test.ts, poszerzony o kropkę w nazwie klucza (`max-turns.accepted`).
// Powielony w obu plikach świadomie: każde kryterium wskazuje dokładnie jeden plik testu
// (AGENTS.md §2a p. 1), więc plik ma stać sam.

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

// Bez znaku i bez wykładnika: to jest `num_turns` ze zdarzenia `result`. Odrzucamy też
// `2 (kontrola)` — komentarz przy liczbie należy do prozy, nie do pola, które czyta maszyna.
function integer(value: string | undefined): number | null {
  if (value === undefined || !/^\d+$/.test(value.trim())) return null;
  return Number.parseInt(value.trim(), 10);
}

// `total_cost_usd` wprost ze zdarzenia, czyli goła liczba dziesiętna. `$0.0124` i `0.0124 (haiku)`
// są odrzucane celowo: to pole jest PORÓWNYWANE z progiem, a nie czytane okiem.
function decimal(value: string | undefined): number | null {
  if (value === undefined || !/^\d+(\.\d+)?$/.test(value.trim())) return null;
  return Number.parseFloat(value.trim());
}

function shown(value: string | undefined): string {
  return value === undefined ? '<the key is absent from the answer block>' : JSON.stringify(value);
}

const doc = existsSync(ANSWER) ? readFileSync(ANSWER, 'utf8') : '';
const blocks = answerBlocks(doc);
const fields = parseFields(blocks[0] ?? '');

describe('S-2 — parser acceptance and actual stopping are recorded separately, per flag', () => {
  it('carries exactly one machine-readable answer block', () => {
    expect(
      blocks.length,
      REL + ' has to carry exactly one ```answer block — that block is what T-11 and T-21 quote',
    ).toBe(1);
  });

  it('names the CLI build the probes ran against', () => {
    // Niezmiennik 24 i T1 ryzyko 2: te flagi są nieudokumentowane w `--help` i zmieniają się
    // między wydaniami, więc odpowiedź bez wersji jest bezużyteczna przy następnej aktualizacji.
    const cli = fields.get('cli');
    expect(
      CLI.test(cli ?? ''),
      'cli has to be the output of `claude --version`, e.g. "claude 2.1.233"; the answer block says: ' +
        shown(cli),
    ).toBe(true);
  });

  it('records the day the probes ran', () => {
    const date = fields.get('date');
    expect(
      DATE.test(date ?? ''),
      'date has to be the day the probes ran, as YYYY-MM-DD; the answer block says: ' + shown(date),
    ).toBe(true);
  });

  it('records a control run that really needed more than one turn', () => {
    // To jest cała różnica między tym testem a słabą asercją z AC-1. Bez tła `num_turns: 1`
    // pod flagą nie dowodzi niczego: prompt, który i tak potrzebował jednej tury, daje tę samą
    // liczbę z flagą i bez niej. TASK.md: jeśli kontrola pokaże 1, próba `--max-turns` jest
    // nieważna i prompt trzeba zmienić, a nie dopisać do dokumentu.
    const control = fields.get('control-turns');
    const turns = integer(control);
    expect(
      turns,
      'control-turns has to be the plain `num_turns` of the run WITHOUT any flag; the answer block says: ' +
        shown(control),
    ).not.toBeNull();
    expect(
      turns as number,
      'the control run took fewer than ' +
        CONTROL_TURNS_MIN +
        ' turns, so it does not tell a capped run apart from an uncapped one — the prompt has to be one that forces a tool call and then a reply about its output',
    ).toBeGreaterThanOrEqual(CONTROL_TURNS_MIN);
  });

  it('records a control run that cost more than the cap it is compared against', () => {
    // Ta sama pułapka po stronie budżetu: cap 0,001 nałożony na bieg, który i tak kosztował
    // mniej, zatrzymuje się „sam" i wygląda jak egzekwowanie.
    const control = fields.get('control-cost');
    const cost = decimal(control);
    expect(
      cost,
      'control-cost has to be the plain `total_cost_usd` of the run WITHOUT any flag, as a bare decimal; the answer block says: ' +
        shown(control),
    ).not.toBeNull();
    expect(
      cost as number,
      'the control run cost no more than the ' +
        CAP_USD +
        ' cap the budget probe passes, so stopping under that cap would prove nothing about the flag',
    ).toBeGreaterThan(CAP_USD);
  });

  for (const flag of FLAGS) {
    it('records whether the parser accepts --' + flag, () => {
      const accepted = fields.get(flag + '.accepted');
      expect(
        ACCEPTED,
        flag +
          '.accepted has to be yes or no — that is the T1-versus-T4 dispute and it is settled by the probe, not by --help; the answer block says: ' +
          shown(accepted),
      ).toContain(accepted);
    });

    it('records whether a run actually stopped on --' + flag, () => {
      const enforced = fields.get(flag + '.enforced');
      expect(
        ENFORCED,
        flag +
          '.enforced has to be yes, no or not-tested; no means the run was made and did NOT stop, not-tested means it was not made; the answer block says: ' +
          shown(enforced),
      ).toContain(enforced);
    });

    it('backs an enforced --' + flag + ' with a named field out of the result event', () => {
      const enforced = fields.get(flag + '.enforced');
      // Ten sam strażnik co w teście wyżej, powtórzony świadomie. Bez niego implikacja
      // „jeśli yes, to dowód" jest prawdziwa dla NIEOBECNEGO pola, a ten test — jedyny, który
      // patrzy na dowód — przechodzi na dokumencie, w którym nie ma nic. Zmierzone na S-1
      // (2026-08-15): dokładnie tak zachowywał się test równoważności werdyktu z konsekwencją
      // dla UI, dopóki nie dostał dwóch takich linii.
      expect(
        ENFORCED,
        'the evidence requirement needs a recorded ' +
          flag +
          '.enforced to apply to; the answer block says: ' +
          shown(enforced),
      ).toContain(enforced);
      if (enforced !== 'yes') return;

      const evidence = fields.get(flag + '.evidence');
      expect(
        (evidence ?? '').trim().length,
        flag +
          '.enforced is yes, so ' +
          flag +
          '.evidence has to say what the run reported; the answer block says: ' +
          shown(evidence),
      ).toBeGreaterThan(0);
      expect(
        RESULT_FIELD.test(evidence ?? ''),
        flag +
          '.evidence has to quote at least one named field of the `result` event with its value — num_turns=, subtype=, terminal_reason= or total_cost_usd= — because a parser reply proves only that the token was accepted, which is the other question entirely; the answer block says: ' +
          shown(evidence),
      ).toBe(true);
    });

    it('does not claim a run stopped on --' + flag + ' while the parser rejects it', () => {
      const accepted = fields.get(flag + '.accepted');
      const enforced = fields.get(flag + '.enforced');
      // Obie wartości muszą najpierw BYĆ i należeć do swoich zbiorów, inaczej implikacja jest
      // prawdziwa dla dwóch pustych pól i przechodzi na pustym dokumencie.
      expect(
        ACCEPTED,
        'the coherence check needs a recorded ' +
          flag +
          '.accepted; the answer block says: ' +
          shown(accepted),
      ).toContain(accepted);
      expect(
        ENFORCED,
        'the coherence check needs a recorded ' +
          flag +
          '.enforced; the answer block says: ' +
          shown(enforced),
      ).toContain(enforced);
      // Flaga, której parser nie zna, przewraca wywołanie zanim model powie cokolwiek, więc
      // żaden bieg nie mógł się o nią zatrzymać. `accepted: no` obok `enforced: yes` to dwa
      // zdania, które nie mogą być prawdziwe naraz — i jest to dokładnie ten fałszywy „tak",
      // dla którego ten spike istnieje.
      if (accepted === 'no') {
        expect(
          enforced,
          flag +
            '.accepted is no, so the CLI rejected the flag before the model ran and nothing could have stopped on it, yet ' +
            flag +
            '.enforced says yes',
        ).not.toBe('yes');
      }
    });
  }

  it('keeps credentials out of the document', () => {
    // Niezmiennik 9. Dokument z założenia niesie pełne linie poleceń trzech sond, a te
    // najłatwiej wkleić prosto z historii powłoki — razem z przedrostkiem eksportującym klucz.
    expect(
      doc.trim().length,
      REL + ' has to carry the probe command lines before they can be read for credentials',
    ).toBeGreaterThan(0);
    expect(
      SECRET.test(doc),
      REL + ' carries something shaped like a credential and this document is committed',
    ).toBe(false);
  });

  it('carries no key that no spec reads', () => {
    // Niezmiennik 21. Klucz dopisany „na wszelki wypadek" to klucz, którego nikt nigdy
    // nie przeczyta. Nowy subtyp `result`, którego nikt wcześniej nie widział (niezmiennik 5,
    // T1 pytanie otwarte 3), zapisuje się DOSŁOWNIE w polu `*.evidence`, a nie w nowym kluczu.
    const extra = [...fields.keys()].filter((k) => !KNOWN_KEYS.includes(k));
    expect(
      extra,
      'the answer block carries keys no spec reads: ' +
        extra.join(', ') +
        ' — prose above the block is where that belongs',
    ).toEqual([]);
  });
});
