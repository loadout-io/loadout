/* Umiejętności i notatki są JEDNĄ sekcją, a dwie półki mówią prawdę o różnicy między nimi.
 *
 * Decyzja właściciela z 2026-08-31. Powód jest mierzalny, nie estetyczny: do tego dnia menu
 * miało siedem pozycji, a dwie z nich odpowiadały na to samo pytanie człowieka — „co ten model
 * wie o mojej pracy". Różnica między nimi jest przy tym jedyną rzeczą, którą człowiek MUSI
 * tu zrozumieć, i była powiedziana raz, mimochodem, w zdaniu strefy: notatka w użyciu wchodzi
 * do KAŻDEGO promptu, a po umiejętność model sięga sam, kiedy pasuje. Dwie sekcje obok siebie
 * nie mówiły tego wcale — mówiły, że to dwie różne rzeczy, i na tym kończyły.
 *
 * DLACZEGO TO KRYTERIUM PYTA MARKUP CAŁEJ POWŁOKI, a nie funkcji.
 * `renderToStaticMarkup(<App section="knowledge" />)` przechodzi przez odkrywanie ekranów
 * (`src/ui/screens.ts`), więc odpowiada na pytanie „czy człowiek to widzi", a nie „czy
 * komponent istnieje" (niezmiennik 29). Ekran zamontowany propsem przeszedłby także wtedy,
 * gdyby rejestr nie miał tej sekcji wcale.
 *
 * TRZY PYTANIA, BO SAMO POŁĄCZENIE NICZEGO NIE DOWODZI:
 *   1. rejestr ma jedną pozycję zamiast dwóch — inaczej człowiek dalej wybiera dwa razy;
 *   2. obie półki stoją w JEDNYM dokumencie i w kolejności, w której kolejka decyzji jest
 *      pierwsza — półka pod półką jest całym zyskiem, bo dopiero sąsiedztwo czyta się
 *      jako różnica;
 *   3. zasięg ma jedno brzmienie — „Everywhere" umiejętności i „Every project" notatki znaczyły
 *      TO SAMO i brzmiały inaczej, więc człowiek nie miał jak poznać, że to jedna oś.
 *
 * Kontrola przeciw pustej asercji stoi przy punkcie trzecim: negatywne „nigdzie nie ma słowa
 * Everywhere" przechodzi na ekranie, który nie rysuje ani jednego wyboru miejsca, więc obok
 * niej stoi asercja, że wybór miejsca JEST w dokumencie.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';
import { App } from '../../App';
import type { Note } from '../../state/memory';
import { useMemory } from '../../state/memory';
import type { Import, InstalledSkill } from '../../state/skills';
import { useSkills } from '../../state/skills';
import { SECTIONS } from '../../ui/sections';

/** Notatka, którą agent zaproponował i która czeka na człowieka. */
const WAITING: Note = {
  place: 'project',
  id: 'n-1',
  title: 'Run the formatter',
  rule: 'Run the formatter before you hand work over.',
  because: 'Two fix rounds went on a comma.',
  status: 'suggested',
  scope: 'this-project',
  length: 137,
  occurrences: 3,
  modified: '2026-08-31T09:00:00Z',
};

/** Notatka w użyciu i sięgająca poza jeden projekt — to ona niesie słowo zasięgu. */
const IN_USE: Note = {
  place: 'library',
  id: 'n-2',
  title: 'Say what changed',
  rule: 'Say what changed, not what you tried.',
  because: 'Reports without it needed a second read every time.',
  status: 'in-use',
  scope: 'everywhere',
  length: 96,
  occurrences: 11,
  modified: '2026-08-30T17:40:00Z',
};

/** Umiejętność, która już leży w katalogach narzędzi agentowych. */
const PLACED: InstalledSkill = {
  name: 'pdf',
  fromTheInternet: false,
  summary: 'Reads a PDF and pulls out its text',
};

/** Wciągnięta z linku i czekająca — bez niej wybór miejsca nie stoi na ekranie. */
const PENDING: Import = {
  name: 'design-review',
  summary: 'Review a screen against the design document.',
  reviewed: {
    body: 'Read the screen and say which rules it breaks.',
    findings: [],
    verdict: 'clean',
  },
  scripts: 0,
  fromTheInternet: true,
};

/** Nagłówek kolejki decyzji: jedyna rzecz na tym ekranie, która czegoś od człowieka chce. */
const DECIDE = 'Waiting for you';
/** Półka notatek. Zdanie różnicy: to wchodzi do każdego promptu. */
const ALWAYS_ON = 'Always on';
/** Półka umiejętności. Zdanie różnicy: po to model sięga sam, kiedy pasuje. */
const WHEN_IT_FITS = 'Used when it fits';

beforeEach(() => {
  useMemory.setState({
    notes: [WAITING, IN_USE],
    passed: [],
    message: null,
    passedProblem: null,
    choice: null,
    pendingDiscard: null,
    read: true,
  });
  useSkills.setState({
    installed: [PLACED],
    pending: PENDING,
    acknowledged: [],
    message: null,
    folders: 'read',
    landing: 'everywhere',
  });
});

describe('knowledge is one section with two shelves that say the difference', () => {
  it('has one place in the side menu instead of two', () => {
    const ids = SECTIONS.map((entry) => entry.id);

    expect(
      ids,
      'skills and notes answer one question a person has — what does this model know about my ' +
        'work — so they get one row in the side menu',
    ).toContain('knowledge');
    expect(
      ids,
      'the old Skills row has to be gone, or a person still picks between two places for one ' +
        'question and the merge bought nothing',
    ).not.toContain('skills');
    expect(ids, 'and so does the old Memory row').not.toContain('memory');
    /* JEDEN WIERSZ, NIE „SZEŚĆ WIERSZY". 2026-08-31, poprawione przy scaleniu z trunkiem.
     *
     * Stała tu liczba bezwzględna (`toBe(6)`) i przewróciła się w dniu, w którym z trunku
     * przyszła ósma sekcja `lab` — mimo że scalenie, o które to kryterium pyta, było nadal
     * zrobione i nadal poprawne. Liczba wierszy menu jest pochodną rejestru i zmienia się
     * z powodów, które nie mają nic wspólnego z tym pytaniem.
     *
     * Pytanie brzmi: czy Skills i Memory są dziś JEDNYM wierszem, a nie jednym przemianowanym
     * obok drugiego. Odpowiadają na nie trzy warunki wyżej (jest `knowledge`, nie ma `skills`,
     * nie ma `memory`) plus ten: `knowledge` występuje DOKŁADNIE RAZ. Rename dałby jeden z tych
     * czterech na czerwono, a dojście nowej sekcji nie rusza żadnego. */
    expect(
      ids.filter((id) => id === 'knowledge').length,
      'Knowledge has to be exactly one row. Two rows carrying it would be the same two-places-' +
        'for-one-question the merge removed, only under one name',
    ).toBe(1);
  });

  it('puts both shelves in one document, with what wants a decision on top', () => {
    const markup = renderToStaticMarkup(<App section="knowledge" />);

    const decide = markup.indexOf(DECIDE);
    const alwaysOn = markup.indexOf(ALWAYS_ON);
    const whenItFits = markup.indexOf(WHEN_IT_FITS);

    expect(
      decide,
      'the notes an agent suggested are the only thing here that wants something from a ' +
        'person, so they are on this screen and they are first',
    ).toBeGreaterThan(-1);
    expect(
      alwaysOn,
      'the shelf of notes in use has to name what makes it different: these go into every ' +
        'prompt. That difference was said once, in passing, and it is the whole point',
    ).toBeGreaterThan(-1);
    expect(
      whenItFits,
      'and the shelf of skills has to name its own half: the model reaches for these itself',
    ).toBeGreaterThan(-1);

    expect(
      decide,
      'what wants a decision stands above both shelves. Under them a person reads two lists ' +
        'first and finds the one thing asked of them last',
    ).toBeLessThan(alwaysOn);
    expect(
      alwaysOn,
      'the two shelves stand next to each other in this order. Split apart they are two lists ' +
        'again, and nothing on screen says how they differ',
    ).toBeLessThan(whenItFits);

    expect(
      markup,
      'the note in use is on this screen under its own words, not just its shelf heading',
    ).toContain('data-note-address="library:n-2"');
    expect(
      markup,
      'and so is the skill on disk — one document carries both, or this is two screens with ' +
        'one heading over them',
    ).toContain('data-skill="pdf"');
  });

  it('says the reach of a note and the reach of a skill with the same words', () => {
    const markup = renderToStaticMarkup(<App section="knowledge" />);

    expect(
      markup,
      'control against an empty assertion below: the choice of where a skill goes has to be ' +
        'on this screen at all, or "the old wording is gone" also passes on a screen that ' +
        'draws no choice',
    ).toContain('data-pick-where');

    expect(
      markup,
      'a note that reaches past one project and a skill that reaches past one project are the ' +
        'same fact on one axis. Two wordings for it read as two different things, and a person ' +
        'has no way to find out they are not',
    ).not.toContain('Everywhere');
    expect(
      (markup.match(/Every project/g) ?? []).length,
      'both halves say it, so the shared wording is on screen twice: once by the note in use, ' +
        'once as the choice of where a skill goes',
    ).toBeGreaterThanOrEqual(2);
  });
});
