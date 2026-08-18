/* AC-3 dla T-42: drugie wejście ISTNIEJE, mieszka w tym samym panelu i OPUSZCZA OKNO.
 *
 * DWA ZDANIA NA EKRANIE OBIECUJĄ KONTROLKĘ, KTÓREJ NIE MA. Pusty ekran sekcji mówi „Paste
 * a link, or write one yourself." (`src/sections/skills/index.tsx`), makieta powtarza to samo
 * (`docs/mockup/index.html:712`), a `src-tauri/commands.golden.txt` nie ma ANI JEDNEJ komendy,
 * która przyjmuje treść umiejętności. Obietnica bez kontrolki jest tym samym defektem, co
 * kontrolka bez skutku, tylko odwróconym (niezmiennik 16) — i jest droższa, bo człowiek szuka
 * przycisku, którego nie ma, zamiast zgłosić, że go brakuje.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: asercja, że w markupie są trzy `<input>`. Przechodzi na
 * formularzu, który nie woła NICZEGO — a to jest dokładnie dzisiejszy stan `answer()` z T-41,
 * tylko w innej sekcji. Rozstrzyga policzenie wywołań na atrapie granicy i porównanie tego, co
 * pojechało, z tym, co człowiek wpisał.
 *
 * DLACZEGO NAZWA KOMENDY NIE JEST TU WPISANA, i to jest druga połowa. Nazwa jest czytana
 * z `src-tauri/commands.golden.txt`, a zbiór nazw ARGUMENTÓW z `src-tauri/src/ipc.rs` — w tym
 * samym biegu testu. Tauri dopasowuje argumenty PO NAZWIE i deserializuje je ZANIM wejdzie
 * w ciało komendy, więc klucz, który się nie zgadza, nie daje mniejszego wywołania: daje
 * odrzucone, przy każdym kliknięciu, z odmową w postaci surowego napisu, którego nikt nie widzi.
 * Tak był zepsuty Start 2026-08-17 za zielonym kryterium, które przepisało z `ipc.rs` dwie nazwy
 * z trzech.
 *
 * RENDER JEST STATYCZNY, bo w repo nie ma `jsdom` — stąd dwie konsekwencje, obie widoczne niżej:
 * treść panelu i to, czy jest otwarty, muszą dać się ZASIAĆ w magazynie, a „oddanie trzech
 * odpowiedzi" jest wywołaniem akcji magazynu, nie kliknięciem. Pierwsza z nich nie jest
 * ustępstwem na rzecz testu: odmowa z Rusta musi zostawić wpisany akapit na ekranie, więc pola
 * i tak muszą leżeć tam, gdzie ląduje odmowa (niezmiennik 13).
 *
 * KONTRAKT NA MARKUP, żeby następny czytelnik nie musiał go zgadywać:
 *   data-add-panel            panel, który otwiera `data-create`. Jeden na ekran.
 *   data-question="<klucz>"   kontrolka jednego z trzech pytań; klucz jest kluczem magazynu.
 *   id / <label for=…>        każde pytanie ma etykietę — pole bez etykiety to pole, o którego
 *                             znaczenie człowiek musi zapytać.
 *   data-write-it-yourself    kontrolka, która oddaje trzy odpowiedzi. Jedna.
 * Adres zostaje pod swoim dzisiejszym `id="skill-link"`: to jest ta droga wejścia, która już była.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { AddPanel } from '../../state/skills';
import { useSkills } from '../../state/skills';
import { ipcSource, windowSideArguments } from '../ipc-signature';
import SkillsScreen from './index';

/* Atrapa granicy: rozwiązuje się albo odmawia, zależnie od tego, o co pyta dany test. Żadnego
 * żywego Tauri — kryterium, które go wymaga, nie umie być czerwone z właściwego powodu, bo
 * `Failed to launch` stoi na liście `NOT_A_REAL_RED` w `harness/gate.py`. */
const { invoked, refuseWith } = vi.hoisted(() => {
  const answer = { refusal: null as string | null };
  return {
    invoked: vi.fn((..._sent: unknown[]) =>
      answer.refusal === null
        ? Promise.resolve(undefined)
        : Promise.reject(new Error(answer.refusal)),
    ),
    refuseWith: (said: string | null): void => {
      answer.refusal = said;
    },
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');

/** Pliki czytamy tak, żeby test padał na asercji o treści, nigdy na otwarciu pliku. */
function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Ta sama lista, którą po drugiej stronie granicy czyta `ipc_commands_registered.rs`. */
const known = new Set(
  fileText(resolve(ROOT, 'src-tauri/commands.golden.txt'))
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#')),
);

/** `ipc.rs` w całości — jedyne miejsce, w którym stoją nazwy argumentów komend. */
const rust = ipcSource();

/**
 * Trzy odpowiedzi z formularza.
 *
 * ANI JEDNEGO APOSTROFU ANI CUDZYSŁOWU w tych zdaniach, i to nie jest przypadek: React ucieka
 * `'` na `&#x27;` we wszystkim, co renderuje, więc `toContain` na tekście z apostrofem byłby
 * czerwony także wtedy, gdy ekran pokazuje dokładnie to, co trzeba. Klasa pułapki jest ta sama,
 * co „test sprawdza obecność stringa": mierzy kodowanie, nie zachowanie.
 */
const TYPED: AddPanel = {
  link: '',
  name: 'Review pull requests',
  whenToUse: 'Use this when somebody asks for a second look at a pull request.',
  whatToDo: 'Read the change first, then say in one paragraph what to fix.',
};

/** Trzy pytania, kluczami magazynu — tymi samymi, którymi markup je nazywa. */
const QUESTIONS = ['name', 'whenToUse', 'whatToDo'] as const;

/** Zdanie odmowy z Rusta. Walidatorowe w kształcie i bez apostrofu, z powodu wyżej. */
const REFUSED = 'Missing required field in frontmatter: name';

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/**
 * Otwierający znacznik elementu niosącego ten atrybut, albo `''`.
 *
 * Pusty napis, a nie wyjątek: wołający ma o niego zapytać JAWNIE i powiedzieć, czego zabrakło.
 * `expect(undefined).not.toContain(…)` przechodzi dla elementu, którego nie ma.
 */
function tagWith(markup: string, attribute: string): string {
  const at = markup.indexOf(attribute);
  if (at < 0) return '';
  const opens = markup.lastIndexOf('<', at);
  const closes = markup.indexOf('>', at);
  return opens < 0 || closes < 0 ? '' : markup.slice(opens, closes + 1);
}

function attributeOf(tag: string, name: string): string {
  return new RegExp(name + '="([^"]*)"').exec(tag)?.[1] ?? '';
}

/** Tekst etykiety wskazującej ten `id`, bez odstępów po brzegach. */
function labelFor(markup: string, id: string): string {
  const found = new RegExp('<label[^>]*for="' + id + '"[^>]*>([^<]*)<', 'i').exec(markup);
  return (found?.[1] ?? '').trim();
}

/** Każda wartość prosta w środku, na dowolnym poziomie zagnieżdżenia. */
function insides(value: unknown, into: unknown[]): unknown[] {
  if (Array.isArray(value)) {
    for (const item of value as unknown[]) insides(item, into);
  } else if (typeof value === 'object' && value !== null) {
    for (const item of Object.values(value as Record<string, unknown>)) insides(item, into);
  } else if (value !== undefined && value !== null) {
    into.push(value);
  }
  return into;
}

function screen(): string {
  return renderToStaticMarkup(<SkillsScreen store={useSkills} />);
}

beforeEach(() => {
  /* Magazyn umiejętności jest singletonem, więc zasianie go w jednym teście dojechałoby do
   * następnego. Stan pusty przed każdym: kolejność testów przestaje mieć znaczenie. */
  useSkills.setState({
    pending: null,
    acknowledged: [],
    message: null,
    installed: [],
    adding: null,
  });
  refuseWith(null);
  invoked.mockClear();
});

describe('a person can write a skill here, and what they write leaves the window', () => {
  it('really read both oracles, so nothing below compares against nothing', () => {
    expect(
      known.size,
      'src-tauri/commands.golden.txt could not be read, so "the window asked for a name off the ' +
        'list" would pass for every name there is.',
    ).toBeGreaterThan(0);
    expect(
      rust,
      'src-tauri/src/ipc.rs could not be read, so the expected set of argument names would come ' +
        'from nowhere and the comparison would pass on two empty lists.',
    ).not.toBe('');
  });

  it('offers exactly one way in at zero, and the second entry is not sitting beside it', () => {
    const closed = screen();
    expect(
      occurrences(closed, 'data-create'),
      'an empty screen is an invitation (DESIGN §6), so exactly one way to add a skill is on ' +
        'screen at zero. src/sections/skills/mounted.test.tsx freezes this same number and that ' +
        'is its whole content: one invitation, not two',
    ).toBe(1);
    expect(
      occurrences(closed, 'data-question'),
      'the three questions are in the document before anybody asked to add anything. The second ' +
        'entry belongs INSIDE the panel that button opens — a form standing open on the empty ' +
        'screen is a second invitation, and then the screen has two answers to one question',
    ).toBe(0);

    useSkills.setState({ adding: TYPED });
    expect(
      occurrences(screen(), 'data-create'),
      'opening the panel added a second way in. Both entries live under the SAME button: a link ' +
        'and a skill written here are one decision with two answers, not two decisions',
    ).toBe(1);
  });

  it('the open panel carries both ways in, and every question says what it is asking', () => {
    useSkills.setState({ adding: TYPED });
    const markup = screen();

    expect(
      occurrences(markup, 'data-add-panel'),
      'the panel a person types into is not in the document, or it is there twice. One panel, ' +
        'because it holds one decision (invariant 13)',
    ).toBe(1);

    const link = tagWith(markup, 'id="skill-link"');
    expect(
      link,
      'the panel dropped the field for an address. This entry existed first and is the one the ' +
        'mockup draws; adding the second one must not cost the first',
    ).not.toBe('');
    expect(
      labelFor(markup, 'skill-link'),
      'the address field lost its label. A field whose meaning a person has to guess is a field ' +
        'they fill in wrong once and never again',
    ).not.toBe('');

    for (const key of QUESTIONS) {
      const tag = tagWith(markup, 'data-question="' + key + '"');
      expect(
        tag,
        'the panel carries no control for the question "' +
          key +
          '". The form is three questions and exactly three [T5 §8.3]: what it is called, when ' +
          'to use it, what to do. Two of them are not a form, they are a field with a title',
      ).not.toBe('');

      const id = attributeOf(tag, 'id');
      expect(id, 'the control for "' + key + '" has no id, so no label can point at it').not.toBe(
        '',
      );
      expect(
        labelFor(markup, id),
        'the question "' +
          key +
          '" has no label with words in it. T5 §8.3 asks three QUESTIONS: a person answers them, ' +
          'so they have to be able to read them',
      ).not.toBe('');
    }

    expect(
      occurrences(markup, 'data-write-it-yourself'),
      'the panel takes three answers and offers no way to hand them over, or offers two. ' +
        'A control that is missing and a control that does nothing are the same thing to the ' +
        'person in front of it (invariant 16)',
    ).toBe(1);
  });

  it('handing over the three answers reaches Rust once, under a name off the golden list', async () => {
    useSkills.setState({ adding: TYPED });

    await useSkills.getState().writeItHere();

    expect(
      invoked.mock.calls.length,
      'the three answers never left the window. This is the state the section is in today: the ' +
        'empty screen promises "write one yourself" and there is no command on the Rust side ' +
        'that takes the CONTENT of a skill at all. Exactly one call, because more than one means ' +
        'the same answers are written twice',
    ).toBe(1);

    const sent = invoked.mock.calls.at(0);
    if (sent === undefined) {
      throw new Error('the three answers never reached Rust at all');
    }

    const asked = sent.at(0);
    expect(
      typeof asked === 'string' && known.has(asked),
      'the window asked Rust for ' +
        String(asked) +
        ', which is not on src-tauri/commands.golden.txt — so nothing on the Rust side keeps ' +
        'that name alive, and the day it is renamed this call goes quiet. The name is read out ' +
        'of that file here, never typed into this test',
    ).toBe(true);

    const payload = sent.at(1);
    const carried =
      typeof payload === 'object' && payload !== null ? (payload as Record<string, unknown>) : {};
    const wanted = [...windowSideArguments(rust, String(asked))].sort();
    expect(
      wanted.length,
      'no signature for ' +
        String(asked) +
        ' could be parsed out of src-tauri/src/ipc.rs, so the key comparison below would be ' +
        'nothing against nothing — the exact shape of green this criterion exists to end',
    ).toBeGreaterThan(0);
    expect(
      Object.keys(carried).sort(),
      'the window sends ' +
        JSON.stringify(Object.keys(carried).sort()) +
        ' and the command takes ' +
        JSON.stringify(wanted) +
        ' (read out of src-tauri/src/ipc.rs in this run). Tauri matches invoke arguments BY NAME ' +
        'and deserializes them before the command body runs, so a key that does not line up is ' +
        'not a smaller call — it is a rejected one, and the refusal arrives as a raw string ' +
        'nobody sees',
    ).toEqual(wanted);

    const leaves = insides(carried, []);
    const lost = QUESTIONS.filter((key) => !leaves.includes(TYPED[key]));
    expect(
      lost,
      'the call reached Rust and left some of what the person typed behind: ' +
        JSON.stringify(lost) +
        '. A form that calls the right command with an empty body is the same silence as no call ' +
        'at all — and the weak version of this criterion (three inputs are in the markup) passes ' +
        'on exactly that',
    ).toEqual([]);
  });

  it('a refusal from the other side keeps what was typed and puts the reason on screen', async () => {
    useSkills.setState({ adding: TYPED });
    refuseWith(REFUSED);

    await useSkills.getState().writeItHere();
    const markup = screen();

    expect(
      markup,
      'Rust refused with a sentence that says what to change and the screen does not show it. ' +
        'Silence after a control looks exactly like a broken control: the person presses it a ' +
        'second time and then reports a bug',
    ).toContain(REFUSED);

    for (const key of QUESTIONS) {
      expect(
        markup,
        'the answer to "' +
          key +
          '" is gone from the panel after a refusal. Text lost on a refusal is the same defect ' +
          'as silence, only more expensive: the person writes a paragraph, reads one sentence ' +
          'about the name, and has to write the paragraph again',
      ).toContain(TYPED[key]);
    }
  });
});
