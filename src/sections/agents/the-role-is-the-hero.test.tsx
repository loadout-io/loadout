/* Ekran Agents jest o ROLACH, nie o modelach — i rola jest bohaterem PANELU, nie kafelka.
 *
 * PO CO TEN PLIK ISTNIEJE, WERSJA PIERWSZA (2026-08-31, rano). Właściciel obejrzał ten ekran
 * i powiedział, że nie widzi przebudowy. Zrzut przyznał mu rację i dał się zmierzyć co do
 * piksela, przy oknie 1512×950: sześć kafelków zajmowało 38% ciała ekranu, panel z formularzem
 * miał 332 px i był wyższy niż okno, a w rzędzie z nazwą stały dwie pigułki z modelem, którego
 * nikt nie wybierał. Odpowiedzią była ściana kafelków na całą szerokość — jeden kafelek na
 * agenta, z nazwą, wierszem metadanej, zdaniem o roli i pierwszymi 150 znakami instrukcji.
 *
 * PO CO ISTNIEJE W TEJ WERSJI (2026-08-31, wieczorem, drugie zgłoszenie właściciela, dwa
 * zrzuty i jedno zdanie: „a i to powinno byc domyslnie, wyjeb ten widok tu"). Ściana kafelków
 * przegrała z arytmetyką swojej własnej biblioteki:
 *
 *   - w bibliotece właściciela leży DWADZIEŚCIA DZIEWIĘĆ ról. Kafelek ma cztery wiersze,
 *     więc ekran ma kilometry przewijania i mieści sześć pozycji naraz;
 *   - żeby przeczytać rolę, dalej trzeba ją było OTWORZYĆ: kafelek niósł 150 znaków promptu,
 *     czyli mniej więcej jedno zdanie z dwudziestu;
 *   - a układ, w którym rola JEST czytelna — spis nazw po lewej i cała rola po prawej —
 *     istniał od rana i stał za kliknięciem. Człowiek dostawał go dopiero, kiedy trafił
 *     w kafelek.
 *
 * TO NIE JEST ODWOŁANIE POPRZEDNIEJ PRAWDY, tylko przeniesienie jej o jedną powierzchnię
 * dalej. Rola dalej jest największą treścią ekranu — tylko jest nią w PRAWEJ KOLUMNIE, a nie
 * w kafelku: nazwa, zdanie o roli i jej WŁASNE instrukcje, w całości, bez wielokropka.
 * Metadana („czym myśli", „ile workflow jej używa") jest od nich mniejsza i to ona ustępuje.
 *
 * KAŻDE KRYTERIUM NIŻEJ PADA NA KODZIE SPRZED TEJ ZMIANY — poza ostatnim, które jest
 * kryterium ZACHOWANYM z poprzedniej wersji i mówi to samo, co mówiło. Reszta to lista zdań,
 * które przed tą zmianą były nieprawdziwe.
 *
 * DLACZEGO `renderToStaticMarkup`, A NIE PRZEGLĄDARKA. Wszystkie te pytania są pytaniami
 * o dokument: co stoi na ekranie, zanim ktokolwiek kliknie, która treść jest największa, czy
 * metadana ustępuje nazwie. Repo nie ma jsdom, a `e2e/harness.ts` odpowiada na pytanie
 * o KLIKNIĘCIE, którego tu nie ma (niezmiennik 29 pozwala wybrać jedno z trzech).
 */
import { readFileSync } from 'node:fs';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent, AgentsIo } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import AgentsScreen from './index';

/* Drabinka stopni jest CZYTANA w tym samym biegu. Liczba przepisana z palca przechodzi także
 * wtedy, gdy arkusz mówi co innego — i to jest najczęstszy sposób, w jaki kryterium o wyglądzie
 * staje się pieczątką (ten sam powód i ta sama technika, co w `library-is-reachable.test.tsx`). */
const THEME = readFileSync(new URL('../../styles/theme.css', import.meta.url), 'utf8');

/** Instrukcje DŁUŻSZE niż 150 znaków, i to jest treść kryterium, nie ozdoba fikstury: tyle
 *  dokładnie mieściło się na kafelku, reszta szła pod wielokropek. */
const WHOLE_ROLE =
  'Write the smallest change that makes the checks pass. Do not touch the public API unless ' +
  'the step says to. When the wording of a step can be read two ways, say which reading you ' +
  'took and why, then take it.';

function agent(over: Partial<Agent> = {}): Agent {
  return {
    schema: 1,
    id: 'a-1',
    name: 'Forge',
    summary: 'Writes code. Small changes, and keeps the public API unless told otherwise.',
    color: 'clay',
    instructions: WHOLE_ROLE,
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'work-freely',
    giveUpAfterMinutes: 20,
    tools: 'everything',
    reachesTheWeb: true,
    skills: [],
    connections: [],
    writeResultsTo: '',
    ...over,
  };
}

const NEEDLE = agent({
  id: 'a-2',
  name: 'Needle',
  color: 'slate',
  summary: 'Runs the checks and reports what passed.',
  instructions: 'Never change a file. Report what passed and what did not.',
  runsWith: 'codex',
  model: 'sol',
  thinking: 'quick',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 8,
});

function ioWith(agents: readonly Agent[]): AgentsIo {
  return {
    list: () => Promise.resolve([...agents]),
    newId: () => Promise.resolve('a-new'),
    save: () => Promise.resolve('after-the-save'),
    remove: () => Promise.resolve(),
  };
}

async function screenOf(
  agents: readonly Agent[],
  over: { usage?: Record<string, number> | null; opened?: Agent } = {},
): Promise<string> {
  const store = createAgentsStore(ioWith(agents));
  await store.getState().load();
  return renderToStaticMarkup(
    <AgentsScreen
      store={store}
      usage={over.usage ?? null}
      {...(over.opened === undefined ? {} : { opened: over.opened })}
    />,
  );
}

/** Arkusz otwartej roli: od otwierającego `<aside` do końca dokumentu. Arkusz jest ostatnią
 *  powierzchnią tego ekranu, więc to wystarcza i nie wymaga liczenia zagnieżdżeń — ta sama
 *  technika, co `panelOf()` w `the-refusal-stands-by-its-button.test.tsx`. */
function sheetOf(markup: string): string {
  const at = markup.indexOf('<aside');
  return at < 0 ? '' : markup.slice(at);
}

/** Spis biblioteki: od jego znacznika do arkusza, albo do końca, gdy arkusza nie ma. */
function libraryOf(markup: string): string {
  const at = markup.indexOf('data-agent-index');
  if (at < 0) return '';
  const end = markup.indexOf('<aside', at);
  return end < 0 ? markup.slice(at) : markup.slice(at, end);
}

/** Wiersz TEGO agenta w spisie — od ZNACZNIKA OTWIERAJĄCEGO jego kontrolkę do znacznika
 *  następnego wiersza. Cofnięcie się do `<` jest treścią, nie wygodą: bez niego kawałek
 *  zaczynałby się w środku atrybutu i pytanie „czy to jest przycisk" nie dałoby się zadać. */
function rowOf(markup: string, id: string): string {
  const library = libraryOf(markup);
  const at = library.indexOf('data-agent="' + id + '"');
  if (at < 0) return '';
  const start = library.lastIndexOf('<', at);
  const next = library.slice(at + 1).search(/data-agent="/);
  return next < 0 ? library.slice(start) : library.slice(start, at + 1 + next);
}

/** Znacznik otwierający element, który niesie ten atrybut. */
function tag(markup: string, attribute: string): string {
  const at = markup.indexOf(attribute);
  if (at < 0) return '';
  const open = markup.lastIndexOf('<', at);
  const close = markup.indexOf('>', at);
  return close < 0 ? '' : markup.slice(open, close + 1);
}

/** Rozmiar stopnia drabinki, przeczytany z arkusza. `null`, kiedy arkusz go nie deklaruje. */
function step(name: string): number | null {
  const found = new RegExp(`--text-${name}:\\s*(\\d+(?:\\.\\d+)?)px`).exec(THEME);
  return found === null ? null : Number(found[1]);
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe('the agents screen opens on a role, not on a wall of cards', () => {
  it('stands a role in the body before anybody clicks anything', async () => {
    const markup = await screenOf([agent(), NEEDLE]);
    const sheet = sheetOf(markup);

    expect(
      sheet,
      'the library opened as a column of cards and the arrangement that actually reads — the ' +
        'names down one side, the whole role down the other — sat behind a click. A person ' +
        'with twenty-nine roles saw six of them and had to open each one to learn what it is.',
    ).not.toBe('');
    expect(
      sheet,
      'and the role standing there is the first one the folder answered with, named by its own ' +
        'name. A surface that opens with nothing in it is the same empty rectangle the card ' +
        'wall was, one column narrower.',
    ).toContain('Forge');
    expect(
      /<button[^>]*>\s*Cancel\s*<\/button>/.test(markup),
      'nothing has been edited yet, so there is nothing to cancel. A control whose handler ' +
        'cannot have an effect is worse than no control (invariant 16) — and Cancel used to ' +
        'mean go back to the cards, which is the view that just went away.',
    ).toBe(false);
  });

  it('lets a role say what it is in its OWN words, whole, without being opened first', async () => {
    const markup = await screenOf([agent(), NEEDLE]);

    expect(
      markup,
      'the instructions are the whole content of an agent, and the card carried the first 150 ' +
        'characters of them — about one sentence in twenty. To read a role you still had to ' +
        'open it, one after another, twenty-nine times.',
    ).toContain(WHOLE_ROLE);
    expect(markup, 'and the words are not cut short with an ellipsis on the way in').not.toContain(
      '…',
    );

    const other = await screenOf([agent(), NEEDLE], { opened: NEEDLE });
    expect(
      sheetOf(other),
      'and picking the other role shows the words of THAT role. Without this line the ' +
        'assertion above also passes on a screen that prints the first role forever.',
    ).toContain('Never change a file. Report what passed and what did not.');
    expect(
      sheetOf(other),
      'the first role is not standing in the same body at the same time',
    ).not.toContain(WHOLE_ROLE);
  });

  it('keeps the whole library as an index of names beside it, and marks the one standing', async () => {
    const markup = await screenOf([agent(), NEEDLE]);

    for (const id of ['a-1', 'a-2']) {
      const row = rowOf(markup, id);
      expect(
        row,
        'every saved role stays reachable by name, so switching between them costs one click ' +
          'and the way back is on screen rather than living only in a corner of the panel',
      ).not.toBe('');
      expect(
        row.startsWith('<button'),
        'and the name is the control that opens it. A row with no handler is the defect that ' +
          'kept saved agents unopenable until 2026-08-18, drawn one column narrower.',
      ).toBe(true);
      expect(
        row,
        'the index is a list of names, not the card wall moved sideways: a card carried four ' +
          'rows each and six of them filled the window',
      ).not.toContain('class="card');
    }

    expect(
      tag(libraryOf(markup), 'aria-current="true"'),
      'and the index says WHICH role is standing. An index that does not is a list a person ' +
        'has to search again after every click.',
    ).toContain('data-agent="a-1"');
    expect(
      occurrences(libraryOf(markup), 'aria-current="true"'),
      'exactly one of them is standing. Two marked rows say the screen holds two roles at once',
    ).toBe(1);
  });

  it('leaves exactly one loudest action standing, and it is the one that writes the file', async () => {
    const markup = await screenOf([agent(), NEEDLE]);

    expect(
      tag(markup, 'data-save'),
      'a screen has one main action, and with a role in the body that action is Save. It used ' +
        'to be ＋ Create in the header, on a screen whose whole subject is the role in front of ' +
        'you — and the accent on both at once says nobody decided what a person came here to do.',
    ).toContain('btn-primary');
    expect(
      occurrences(markup, 'btn-primary'),
      'and it is the ONLY thing in the accent colour. Two controls of the same weight are two ' +
        'answers to the same question.',
    ).toBe(1);
    expect(
      occurrences(markup, 'data-create'),
      'creating a role is still one control and still on screen — it moved to the top of the ' +
        'index, where the old way back to the cards used to be, because the cards it led to ' +
        'are gone (invariant 16)',
    ).toBe(1);

    const empty = await screenOf([]);
    expect(
      occurrences(empty, 'btn-primary'),
      'and with an empty library the one loud action is the invitation. Without this line the ' +
        'assertion above also passes on a screen that lost its accent entirely.',
    ).toBe(1);
  });

  it('gives the name of the role a bigger step than its metadata, and the metadata yields', async () => {
    const markup = await screenOf([agent(), NEEDLE], { usage: { 'a-1': 3 } });
    const sheet = sheetOf(markup);

    const heading = step('heading');
    const meta = step('meta');
    expect(
      heading,
      'theme.css has to still declare the heading step; the maths leans on it',
    ).not.toBeNull();
    expect(meta, 'and the meta step, or there is nothing to compare against').not.toBeNull();
    expect(
      Number(heading ?? 0) > Number(meta ?? 0),
      'the ladder itself has to put the heading above the meta step, or naming the classes ' +
        'below proves nothing about which text is bigger',
    ).toBe(true);

    const name = tag(sheet, 'text-heading');
    expect(name.startsWith('<h2'), 'the name of the role heads the surface that holds it').toBe(
      true,
    );

    const facts = tag(sheet, 'data-facts');
    expect(
      facts,
      'the line of plain facts has to be marked, or nothing below is measurable',
    ).not.toBe('');
    expect(
      facts,
      'and it sits on the smaller step. The card put the model in a pill — a full border, a ' +
        'fill and a pill radius, the shape this app uses for something worth reading — beside ' +
        'a name a person wrote by hand.',
    ).toContain('text-meta');
    expect(
      facts,
      'the metadata is the one that gives way when the row runs out of width: it shortens, ' +
        'human words do not',
    ).toContain('truncate');
    expect(facts, 'so it must not be pinned to its own width the way the pills were').not.toContain(
      'shrink-0',
    );
    expect(
      name,
      'and the name never yields. It used to be the other way round: the pills refused to ' +
        'shrink, so a long name was cut short to make room for the model nobody chose.',
    ).toContain('shrink-0');
    expect(
      facts,
      'the number is the one that was actually counted, and it is the same one the question ' +
        'before Delete asks with',
    ).not.toBe('');
    expect(sheet).toContain('used in 3 workflows');
  });

  it('never says the same fact in the index and in the sheet at the same time', async () => {
    const markup = await screenOf([agent(), NEEDLE], { usage: { 'a-1': 3, 'a-2': 0 } });

    expect(
      libraryOf(markup),
      'the row carried five facts about the model in one line — vendor, model, how deeply it ' +
        'thinks, what it may touch, when it gives up — and every one of them is a field of the ' +
        'form standing right beside it. Said twice, one of the two goes stale (invariant 13).',
    ).not.toContain('Claude Code');
    expect(
      libraryOf(markup),
      'and the same goes for the count of workflows: it belongs beside the role it is about, ' +
        'directly above the Delete that needs it',
    ).not.toContain('used in 3 workflows');
    expect(sheetOf(markup), 'which is where it now stands, exactly once').toContain(
      'used in 3 workflows',
    );
  });

  it('gives no room to a column that would have nothing in it', async () => {
    /* KRYTERIUM ZACHOWANE z pierwszej wersji tego pliku i mówiące dokładnie to samo: pierwsza
     * rola powstaje na PUSTEJ bibliotece, więc spis obok niej nie miałby czego spisać. */
    const fresh = agent({ id: '', name: '', summary: '', instructions: '' });
    const markup = await screenOf([], { opened: fresh });

    expect(
      markup,
      'the first role is written on an empty library, so the index beside it would list ' +
        'nothing. A container with nothing in it is room taken away from the thing a person ' +
        'came for.',
    ).not.toContain('data-agent-index');
    expect(sheetOf(markup), 'and the sheet is still the surface that holds it').not.toBe('');
  });
});
