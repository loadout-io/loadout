/* Ekran Knowledge ma jednego bohatera, a jest nim kolejka decyzji.
 *
 * ZMIERZONA WADA (2026-08-31, zrzuty z chromium przy 1512×950). Sekcja miała dobrą STRUKTURĘ —
 * kolejka na górze, dwie półki, przekazania na dole — i nie miała KOMPOZYCJI. Wszystko na niej
 * było tego samego rozmiaru i tej samej głośności:
 *
 *   - ani jednej czynności głównej na całym ekranie pełnym. „Use this" przy notatce, którą
 *     agent właśnie zaproponował, było tym samym cichym przyciskiem, co „Stop using" przy
 *     notatce, która od tygodnia jedzie do promptu — a najgłośniejszą rzeczą na dolnej połowie
 *     był rząd pięciu czerwonych „Remove" pod umiejętnościami, czyli czynność, której człowiek
 *     używa najrzadziej ze wszystkich;
 *   - pusty ekran to był znak, jedno zdanie i jeden przycisk pośrodku czerni. Zdanie mówiło
 *     o DWÓCH rzeczach, a wyjście prowadziło do jednej — i nic nie mówiło, że drugiej człowiek
 *     nie dodaje wcale, bo pisze ją agent po biegu.
 *
 * # Jak brzmiałaby słaba wersja tego kryterium i co ją odróżnia
 *
 * **Słaba pierwsza: „w markupie jest klasa `btn-primary`".** Przechodzi na ekranie z pięcioma
 * takimi przyciskami, czyli na dokładnie tym braku rozstrzygnięcia, który to kryterium usuwa.
 * Odróżnia je LICZBA: dokładnie jeden, na obu stanach ekranu.
 *
 * **Słaba druga: policzyć te przyciski i nie pytać, KTÓRY to jest.** Przechodzi na ekranie,
 * na którym akcentem świeci „Add a skill", a decyzja czekająca na człowieka jest cicha — czyli
 * na odwróconej kompozycji z inną liczbą. Dlatego przypadek pierwszy pyta, czy ten jeden
 * przycisk stoi WEWNĄTRZ strefy kolejki, i czy niesie jej czasownik.
 *
 * **Słaba trzecia: pytać komponent półki wprost.** Zwrócona wartość dowodzi, że mechanizm
 * istnieje; markup całej powłoki dowodzi, że produkt działa (niezmiennik 29). Każdy przypadek
 * niżej renderuje `<App section="knowledge" />`, czyli przechodzi przez odkrywanie ekranów
 * (`src/ui/screens.ts`) — tak samo jak `one-section-two-shelves.test.tsx` obok.
 *
 * Render jest statyczny (`renderToStaticMarkup`), bo w repo nie ma `jsdom`. To, że kolumny
 * naprawdę stoją OBOK SIEBIE, a nie jedna pod drugą, mierzy przeglądarka:
 * `e2e/tests/knowledge-two-shelves-touch.spec.ts`.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import { App } from '../../App';
import type { Note } from '../../state/memory';
import { useMemory } from '../../state/memory';
import type { InstalledSkill } from '../../state/skills';
import { useSkills } from '../../state/skills';

/** Notatka, którą agent zaproponował i która czeka na człowieka. */
function suggested(id: string, rule: string): Note {
  return {
    place: 'project',
    id,
    title: id,
    rule,
    because: 'The fixture keeps a reason on screen, because every note carries one.',
    status: 'suggested',
    scope: 'this-project',
    length: rule.length,
    occurrences: 2,
    modified: '2026-08-31T09:00:00Z',
  };
}

/** Notatka, która już jedzie do każdego promptu i niczego od nikogo nie chce. */
const IN_USE: Note = {
  place: 'library',
  id: 'in-use',
  title: 'Say what changed',
  rule: 'Say what changed, not what you tried.',
  because: 'Reports without it needed a second read every time.',
  status: 'in-use',
  scope: 'everywhere',
  length: 96,
  occurrences: 11,
  modified: '2026-08-30T17:40:00Z',
};

const FIRST = suggested('first', 'Run the formatter before you hand work over.');
const SECOND = suggested('second', 'Name the next move when a vendor fails.');

const PLACED: InstalledSkill = {
  name: 'pdf',
  fromTheInternet: false,
  summary: 'Reads a PDF and pulls out its text',
};

/** Czasownik czoła kolejki — to samo słowo czyta kryterium i człowiek. */
const ANSWER_THE_QUEUE = 'Use this';
/** Czasownik drugiej połowy sekcji. */
const ADD_A_SKILL = 'Add a skill';

/** Ile razy ta klasa stoi w atrybucie `class` — z granicą, więc `btn-primary` ≠ `btn`. */
function loudest(markup: string): number {
  return (markup.match(/class="[^"]*\bbtn-primary\b[^"]*"/g) ?? []).length;
}

/** Kawałek markupu od znacznika tej strefy do znacznika następnej. */
function zone(markup: string, id: string): string {
  const start = markup.indexOf('data-zone="' + id + '"');
  if (start < 0) return '';
  const next = markup.slice(start + 1).search(/data-zone="/);
  return next < 0 ? markup.slice(start) : markup.slice(start, start + 1 + next);
}

/** Otwierający znacznik przycisku niosącego tę etykietę — napis w `<span>` wygląda tak samo. */
function buttonFor(html: string, label: string): string {
  const at = html.indexOf(label);
  if (at < 0) throw new Error('nothing on screen is labelled: ' + label);
  const opens = html.lastIndexOf('<button', at);
  if (opens < 0) throw new Error('this label is not inside a button: ' + label);
  return html.slice(opens, html.indexOf('>', opens) + 1);
}

/** Sam tekst kawałka markupu, bez znaczników i bez nadmiarowych odstępów. */
function words(part: string): string {
  return part
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function full(): string {
  useMemory.setState({
    notes: [FIRST, SECOND, IN_USE],
    passed: [],
    message: null,
    passedProblem: null,
    choice: null,
    pendingDiscard: null,
    read: true,
  });
  useSkills.setState({
    installed: [PLACED],
    pending: null,
    adding: null,
    acknowledged: [],
    message: null,
    folders: 'read',
    removing: null,
  });
  return renderToStaticMarkup(<App section="knowledge" />);
}

function empty(): string {
  useMemory.setState({
    notes: [],
    passed: [],
    message: null,
    passedProblem: null,
    choice: null,
    pendingDiscard: null,
    read: true,
  });
  useSkills.setState({
    installed: [],
    pending: null,
    adding: null,
    acknowledged: [],
    message: null,
    folders: 'read',
    removing: null,
  });
  return renderToStaticMarkup(<App section="knowledge" />);
}

beforeEach(() => {
  useMemory.setState({ notes: [], passed: [], read: true });
  useSkills.setState({ installed: [], pending: null, adding: null, folders: 'read' });
});

describe('the knowledge screen has one hero, and it is the queue of decisions', () => {
  it('gives the loudest control to the decision waiting, and gives it to nothing else', () => {
    const markup = full();

    expect(
      loudest(markup),
      'a screen where everything is equally loud has decided nothing. With two notes an agent ' +
        'suggested, one skill saved and a way to add another, exactly one control on this ' +
        'screen may carry the accent — and a row of same-weight controls means nobody said ' +
        'what matters',
    ).toBe(1);

    expect(
      buttonFor(zone(markup, 'suggested'), ANSWER_THE_QUEUE),
      'and the one that carries it is the answer to what is waiting, because that is the only ' +
        'thing on this screen that wants something from a person. Everything else here is a ' +
        'statement of how things stand',
    ).toContain('btn-primary');

    expect(
      buttonFor(markup, ADD_A_SKILL),
      'so adding a skill steps down while a decision is waiting. Two accents are two answers ' +
        'to "what did I come here for"',
    ).not.toContain('btn-primary');
  });

  /* KONTROLA PRZECIW PUSTEJ ASERCJI, i jedyny przypadek w tym pliku, który był ZIELONY przed
     zmianą. Pusty ekran miał swoją jedną czynność główną od pierwszego dnia — a bez tego
     przypadku „dokładnie jeden akcent" wyżej dałoby się spełnić także tak, że akcent znika
     z ekranu, na którym nic nie czeka, i człowiek zostaje bez ani jednej drogi dalej. */
  it('leaves the accent to adding a skill when nothing is waiting for an answer', () => {
    const markup = empty();

    expect(
      loudest(markup),
      'an empty screen is an invitation, and an invitation with no way forward is a dead end. ' +
        'Exactly one control carries the accent here too',
    ).toBe(1);
    expect(
      buttonFor(markup, ADD_A_SKILL),
      'with an empty queue nothing on this screen wants anything, so the one thing a person ' +
        'can do here is the loud one',
    ).toContain('btn-primary');
  });

  it('says both roads in on the empty screen, and offers the one a person can walk', () => {
    const said = words(empty());

    expect(
      said,
      'a person landing here for the first time reads about two kinds of knowledge. The screen ' +
        'has to name the one that goes into every prompt',
    ).toContain('go into every prompt');
    expect(
      said,
      'and it has to say who writes those: an agent does, after a run teaches it something. ' +
        'One button labelled "Add a skill" answers for the other half and says nothing at all ' +
        'about this one, so a person is left believing notes are theirs to type',
    ).toContain('An agent writes one');
    expect(
      said,
      'and it has to name the other kind by what makes it different: the model reaches for ' +
        'these itself',
    ).toContain('reaches for these on its own');
    expect(
      said,
      'and say how those get here, because unlike a note this one is in a person’s hands',
    ).toContain('Paste a link');

    expect(
      (empty().match(/data-create\b/g) ?? []).length,
      'exactly one way forward, and it belongs to the half a person can actually add to. A ' +
        'button beside the notes road would be a control with nothing behind it (invariant 16)',
    ).toBe(1);
  });

  it('puts what a person wrote above the facts about it', () => {
    const markup = full();
    const first = markup.indexOf(FIRST.rule);
    const facts = markup.indexOf('Length ' + String(FIRST.length));

    expect(first, 'the note an agent suggested is not on this screen at all').toBeGreaterThan(-1);
    expect(facts, 'and neither is its length').toBeGreaterThan(-1);
    expect(
      first,
      'the sentence that actually goes to the model reads first. Under the old order the eye ' +
        'landed on "Suggested Length 44 This project Suggested after run run-2026-08-30-1412" ' +
        '— machine-written, longer than the note itself, and standing over it',
    ).toBeLessThan(facts);
  });
});
