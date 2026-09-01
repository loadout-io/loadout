/* Wiersz propozycji niesie przycisk, który NAZYWA workflow — trzecie kryterium T-61.
 *
 * PO CO. Wartość tej zmiany to jedno kliknięcie zamiast przepisywania: lider patrzy na projekt
 * i umie powiedzieć „to jest robota dla tego workflow, z takim zadaniem", a człowiek nie musi
 * pamiętać nazw plików. Zdanie bez przycisku zostawia go tam, gdzie był — z komendą do
 * przepisania ręcznie.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(markup).toContain('<button')`. Przechodzi dla przycisku
 * ROZWIJANIA, który w wierszu z wyjściem i tak stoi, i przechodzi dla implementacji, która
 * dokłada przycisk każdemu wierszowi. Rozróżniają to dwie rzeczy: DOSTĘPNA NAZWA musi zawierać
 * nazwę workflow z komendy („Run" bez nazwy nie mówi, co się stanie), a wiersz `note` z DOKŁADNIE
 * tą samą treścią i tą samą komendą przycisku mieć nie może — bo o tym, czy to jest propozycja,
 * rozstrzygnął Rust, a nie okno (niezmiennik 15).
 *
 * WIERSZE BUDUJE MODEL, nie ten plik. Rodzaj spoza rejestru model PORZUCA, więc wiersz, którego
 * nie ma jak dostać z drutu, nie da się tu narysować przez pomyłkę — i dlatego pierwszy przypadek
 * pyta najpierw o to, co model naprawdę zbudował. To ta sama obrona, co w
 * `line-says-who-and-how-much.test.tsx`.
 *
 * CZEGO TU NIE MA, i to jest zgłoszone (AGENTS.md §7): drogi, którą komenda naprawdę przyjeżdża
 * do komponentu. `HistoryRow` powstaje w `./model.ts`, którego T-61 nie ma w bloku
 * `<!-- OWNS -->`, więc pole `command` z drutu kończy bieg w modelu. Komenda jedzie tu propsem
 * — szwem, który istnieje po to, żeby ten wiersz dał się w ogóle narysować i osądzić.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { FeedLine } from '../../../state/run';
import { Feed } from './feed';
import { sealedScroller } from './fixtures/scroller';
import type { Kind } from './kinds';
import { Line } from './line';
import type { HistoryRow } from './model';
import { createFeed } from './model';

/** Kto proponuje. To samo słowo, którym lider podpisuje każdy swój wiersz. */
const LEAD = 'Lead';

/** Rodzaj wiersza propozycji na drucie. */
const SUGGESTED = 'suggested';

/**
 * Nazwa rodzaju jako `Kind`.
 *
 * Rzutowanie jest treścią, nie obejściem: `Kind` pochodzi z lustra drutu, więc dopóki lustro
 * tego rodzaju nie zna, nazwa do niego nie należy i ten plik nie skompilowałby się wcale —
 * a kryterium, które się nie kompiluje, nie uruchomiło niczego (AGENTS.md §2a p. 5).
 */
function asKind(kind: string): Kind {
  return kind as unknown as Kind;
}

/** Nazwa workflow z komendy. Rozpoznawalna, żeby żadna asercja nie trafiła jej przypadkiem. */
const WORKFLOW = 'nightly-cleanup';

/** Komenda, znak w znak taka, jaką napisał lider. */
const COMMAND = '/run ' + WORKFLOW + ' Delete the run folders older than a week';

/** Powód, dla którego lider to proponuje. Bez apostrofów: markup je ucieka, asercja nie. */
const BECAUSE = 'They are eating the disk and nobody reads them.';

/** Cała proza lidera, sklejona do jednej linii — tak, jak przyjeżdża z drutu (reguła 1). */
const PROSE = COMMAND + ' ' + BECAUSE;

/**
 * Wiersz z drutu tego rodzaju.
 *
 * Rzutowanie, bo dopóki lustro drutu tego rodzaju nie zna, `FeedLine` go nie obejmuje i ten
 * plik nie skompilowałby się wcale — a kryterium, które się nie kompiluje, nie uruchomiło
 * niczego (AGENTS.md §2a p. 5). Kształt jest kształtem ze złotego pliku.
 */
function suggested(id: number, at: number): FeedLine {
  return {
    kind: SUGGESTED,
    agent: LEAD,
    text: PROSE,
    command: COMMAND,
    id,
    at,
  } as unknown as FeedLine;
}

/** Ta sama proza, rodzaj `note`. Kontrola: różni je RODZAJ i nic poza nim. */
function note(id: number, at: number): FeedLine {
  return { kind: 'note', agent: LEAD, text: PROSE, id, at, body: [] };
}

/** Wiersze historii policzone przez MODEL — dokładnie te, które dostaje ekran. */
function rows(): readonly HistoryRow[] {
  const feed = createFeed(sealedScroller());
  feed.appendLines([suggested(1, 0), note(2, 5_000)]);
  return feed.view.history;
}

/* PO RODZAJU, NIE PO POZYCJI. Rodzaj spoza rejestru model porzuca, więc wiersz z pozycji zero
 * bywa TYM DRUGIM — a wtedy „wiersz `note` nie ma przycisku" przechodzi na wierszu, którego
 * w historii nie ma wcale. To jest dokładnie ta odmiana zieleni, którą kontrola ma łapać. */
const history = rows();
const proposed = history.find((row) => row.kind === asKind(SUGGESTED));
const plain = history.find((row) => row.kind === asKind('note'));

/** Markup jednego wiersza. `command` jedzie propsem — powód w nagłówku. */
function markupOf(row: HistoryRow | undefined): string {
  if (row === undefined) return '';
  return renderToStaticMarkup(
    <Line
      row={row}
      command={COMMAND}
      onToggle={() => {
        /* Kryterium pyta o markup; co robi kliknięcie, ma swój własny plik. */
      }}
    />,
  );
}

/**
 * Dostępne nazwy przycisków w tym markupie, w kolejności wystąpienia.
 *
 * Nazwa to `aria-label`, a kiedy go nie ma — widoczna treść przycisku. Tak samo czyta ją
 * czytnik ekranu, więc pytanie „czy przycisk mówi, co uruchomi" ma tu jedną odpowiedź.
 */
function buttonNames(markup: string): readonly string[] {
  return [...markup.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)].map((hit) => {
    const attributes = hit[1] ?? '';
    const inside = hit[2] ?? '';
    const labelled = /aria-label="([^"]*)"/.exec(attributes);
    return (labelled === null ? inside.replace(/<[^>]*>/g, ' ') : (labelled[1] ?? '')).trim();
  });
}

describe('a suggested run carries a button that names the workflow', () => {
  it('runs on the row the model really built out of the line', () => {
    expect(
      [proposed?.kind, plain?.kind],
      'the model gave back something other than the two rows this file asks about. A kind with ' +
        'no entry in the registry is DROPPED, so an empty history here means the view has no ' +
        'place for this line at all and every assertion below would be about a row nobody built.',
    ).toEqual(['suggested', 'note']);
    expect(
      proposed?.label,
      'and the row keeps the whole prose, because the model, not this file, wrote the label',
    ).toBe(PROSE);
  });

  it('carries exactly one control, and its name says which workflow will run', () => {
    const names = buttonNames(markupOf(proposed));

    expect(
      names.length,
      'one proposal is one control. Zero means the sentence stands there with the command to be ' +
        'retyped by hand, which is the state this task exists to end; two means something else ' +
        'on this row also looks clickable, and neither says which of them starts work.',
    ).toBe(1);
    expect(
      names[0] ?? '',
      'the name of the button has to carry the name of the workflow from the command. `Run` on ' +
        'its own does not say what will happen — and what happens is that agents start and money ' +
        'starts being spent, so it is the one thing the control must say before it is pressed.',
    ).toContain(WORKFLOW);
  });

  it('shows the reason the lead gave without anyone having to open the row', () => {
    const collapsed = markupOf(
      proposed === undefined ? undefined : { ...proposed, expanded: false },
    );

    expect(
      collapsed,
      'the prose has to stand in the same row as the button. A person is meant to read WHY the ' +
        'lead suggests this before pressing it; a row that hides the reason behind a click is a ' +
        'button with a hidden reason next to it.',
    ).toContain(BECAUSE);
    expect(
      buttonNames(collapsed).length,
      'and the button is there while the row is shut, or it is a control nobody can find',
    ).toBe(1);
  });

  it('tells the two apart by kind alone, on the very same text and command', () => {
    expect(
      buttonNames(markupOf(plain)),
      'the row of kind `note` grew a control too, so the button belongs to every row instead of ' +
        'to a proposal. What makes a proposal is the KIND — decided in Rust, where the mapping ' +
        'from event to line lives (invariant 15) — and this row carries the same text and the ' +
        'same command with a different kind. A window that draws the button anyway is back to ' +
        'reading `/run` out of prose, which is curation a stylesheet can break.',
    ).toEqual([]);
    expect(
      buttonNames(markupOf(proposed)).length,
      'and the other half, without which the line above passes for a view that draws no buttons ' +
        'at all: the row Rust DID mint as a proposal has one',
    ).toBe(1);
  });

  it('draws the same button through the whole history, with nothing handed over by this file', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([suggested(1, 0), note(2, 5_000)]);

    /* CAŁA STREFA HISTORII, NIE POJEDYNCZY WIERSZ, i to jest jedyna różnica wobec przypadków
       wyżej — te podają komendę propsem, więc dowodzą, że wiersz UMIE się narysować. Tutaj nie
       podaje jej nikt stąd: jeśli przycisk jest, to znaczy, że komenda przeszła całą drogę,
       którą przechodzi w działającej aplikacji — z linii przez model do wiersza, a z wiersza
       przez `./feed.tsx` do komponentu. Kontrolka, której jedynym wołającym jest kryterium,
       jest kontrolką, której nikt nigdy nie naciśnie (niezmiennik 16). */
    const markup = renderToStaticMarkup(
      <Feed
        view={feed.view}
        portRef={() => {
          /* Przewijanie ma swój własny plik; ten przypadek pyta o markup. */
        }}
        onToggle={() => {
          /* Rozwijanie wiersza też. */
        }}
        onAnswer={() => {
          /* Pytania do człowieka w tej historii nie ma. */
        }}
        onJumpToNewest={() => {
          /* Skok do najnowszego wiersza to inna kontrolka. */
        }}
      />,
    );

    expect(
      buttonNames(markup),
      'the history the screen really draws has no button naming the workflow, so the command ' +
        'stops somewhere between the line and the row: the button renders only when the row is ' +
        'given one, and in the running app the screen is the only thing that gives it. Passing ' +
        'the command in by hand, as the cases above do, proves the row can be drawn — it cannot ' +
        'prove anybody draws it.',
    ).toContain('Run ' + WORKFLOW);
  });
});
