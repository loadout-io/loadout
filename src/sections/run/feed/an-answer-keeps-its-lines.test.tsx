/* To, co agent napisał w kilku wierszach, ma się czytać w kilku wierszach.
 *
 * # Skarga
 *
 * Właściciel, 2026-08-23, o strumieniu: „ten tekst niech też będzie jakoś fajnie i ładnie
 * formatowany aby było to przyjemniejsze".
 *
 * # Co było zepsute
 *
 * Model widoku przepuszcza tekst agenta NIETKNIĘTY (`feed/model.ts`, `sentence`), więc jego
 * przełamania dojeżdżały aż do DOM-u — i ginęły tam, bo domyślne `white-space` zamienia każdy
 * przełam w spację. Agent, który odpowiadał listą albo akapitami, dostawał na ekranie jeden
 * zbity blok. To nie był brak renderera markdown: to była utrata rzeczy, którą model naprawdę
 * napisał, już po tym, jak dojechała na miejsce.
 *
 * # Czego to kryterium NIE mówi
 *
 * # 2026-08-30 — TO KRYTERIUM SĄDZIŁO NIEWŁAŚCIWY RODZAJ WIERSZA, WIĘC BYŁO ZIELONE NAD WADĄ
 *
 * Fikstura wołała `line.step(...)`. Wiersz rodzaju `step` pisze PLANISTA (`engine::line`,
 * nagłówek `Line`) i nie przechodzi przez kuratora — więc to kryterium sprawdzało arkusz stylów
 * na ścieżce, którą proza agenta nie chodzi nigdy. Prawdziwa proza to rodzaj `note`, a ta była
 * spłaszczana WARSTWĘ WCZEŚNIEJ, w Ruście: `Curator::observe` wołało `one_line`, które skleja
 * każdy biały znak w spację (`src-tauri/src/engine/line.rs`). CSS był poprawny, kryterium
 * zielone, a skarga właściciela z 2026-08-23 niezałatwiona — dokładnie ta klasa, dla której
 * w AGENTS.md stoi niezmiennik 29, tylko po stronie okna.
 *
 * Rust ma od 2026-08-30 dwa tryby (`Curator::talking` dla rozmowy, `Curator::new` dla biegu),
 * a ta fikstura pyta o rodzaj, który agent naprawdę produkuje.
 *
 * Nie mówi „renderuj markdown". Renderer to nowa zależność, a `src/ui/shell/permissions.test.ts`
 * zapisuje wprost, czym to grozi w oknie z dostępem do powłoki — i taka decyzja należy do
 * człowieka (AGENTS.md §7). To kryterium pilnuje wyłącznie tego, żeby NIE GUBIĆ tego, co już
 * przyszło.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { createFeed } from './model';
import { Line } from './line';

const FORGE = 'Forge';

/** Odpowiedź w trzech wierszach — dokładnie to, co agenci piszą naprawdę. */
const ANSWER = 'Three districts came out ahead:\n- Wrzeszcz Gorny\n- Oliwa';

function markup(): string {
  const feed = createFeed(sealedScroller());
  feed.appendLines([line.note(1, 0, FORGE, ANSWER)]);
  const row = feed.view.history[0];
  if (row === undefined) return '';
  return renderToStaticMarkup(
    <Line
      row={row}
      onToggle={() => {
        /* To kryterium pyta o markup, nie o skutek kliknięcia. */
      }}
    />,
  );
}

/** Element otaczający zdanie agenta, razem z jego klasami.
 *
 * 2026-08-31 — SZUKA KLASY, NIE TEKSTU TUŻ ZA NIĄ. Poprzednia wersja dopasowywała
 * `<span class="…">Three districts` i przestała cokolwiek znajdować, kiedy między klasą a tekstem
 * stanął renderer nagłówka. Zwracała wtedy pusty napis, a wszystkie asercje na niej stały się
 * zdaniami o pustym napisie — czyli czerwone nad poprawnym kodem. */
function wrapper(html: string): string {
  return /<span class="([^"]*whitespace-pre-line[^"]*)"/.exec(html)?.[1] ?? '';
}

describe('an answer keeps the shape the agent gave it', () => {
  const html = markup();

  it('put the answer on the screen at all, or the rest is about nothing', () => {
    expect(
      html.includes('Three districts came out ahead:'),
      'the answer never reached the markup, so every point below would be true of an empty ' +
        'string. It rendered: ' +
        JSON.stringify(html.slice(0, 200)),
    ).toBe(true);
  });

  it('keeps the line breaks the agent typed', () => {
    expect(
      /whitespace-pre-line/.test(wrapper(html)),
      'the answer is drawn with the default whitespace rule, which turns every line break into ' +
        'a space. An agent that answered with a list gets one solid block of prose, and the ' +
        'shape it chose to explain itself is gone. It carried: ' +
        JSON.stringify(wrapper(html)),
    ).toBe(true);
    /* PO CAŁYM TOKENIE, nie regexem z `\b`: granica słowa wypada także między `pre` a `-line`,
     * więc `/whitespace-pre\b/` trafiałaby w tę samą klasę, której ten punkt broni. Pierwsza
     * wersja tej asercji miała dokładnie ten błąd i była czerwona nad poprawnym kodem. */
    expect(
      wrapper(html).split(/\s+/).includes('whitespace-pre'),
      'but not the rule that also stops wrapping: a long answer would then run off the side ' +
        'of the column instead of folding, and the agents list gets pushed out of the window',
    ).toBe(false);
  });

  it('lets a long unbroken word fold instead of widening the column', () => {
    expect(
      /break-words|break-all/.test(wrapper(html)),
      'one long path or address with no spaces in it widens the stream column, and the column ' +
        'of agents beside it is what gets pushed off the screen',
    ).toBe(true);
  });
});

/* ── CIAŁO ZA WIERSZEM ─────────────────────────────────────────────────────────────────────
 *
 * 2026-08-31. Odpowiedź, która nie mieści się w wierszu, oddaje nagłówek, a całość idzie za
 * wiersz — reguła 1 [T2 §7.3], domknięta dopiero tego dnia, bo `Line::Note` do wtedy nie miało
 * pola na tę treść. Pomiar, który to wymusił: zrzut właściciela z biegu `20260830-191440`,
 * odpowiedź na 78 wierszy zasłaniająca komplet dziewięciu kroków.
 *
 * TE PRZYPADKI SĄDZĄ RZECZ, KTÓREJ RUST SĄDZIĆ NIE MOŻE: czym ta treść jest NARYSOWANA. Podział
 * na nagłówek i ciało ma swoje kryterium po tamtej stronie (`lead_answer_keeps_its_lines`), a tu
 * pytamy o jedno — czy zdanie agenta wygląda jak zdanie, czy jak wyjście maszyny, która padła.
 */
function withBody(): string {
  const feed = createFeed(sealedScroller());
  feed.appendLines([line.note(1, 0, FORGE, 'All three came out ahead.', BODY)]);
  const row = feed.view.history[0];
  if (row === undefined) return '';
  return renderToStaticMarkup(
    <Line
      row={{ ...row, expanded: true }}
      onToggle={() => {
        /* To kryterium pyta o markup, nie o skutek kliknięcia. */
      }}
    />,
  );
}

const BODY = ['All three came out ahead.', '', '## Evidence', '- src/styles.css:456'];

describe('an answer too long for its row is reachable, and reads as prose', () => {
  it('offers a way to open it, or the rest of the answer is simply gone', () => {
    const html = withBody();
    expect(
      html.includes('Show less') || html.includes('Show more'),
      'the row shows a headline and the rest lives behind a control. With no control the other ' +
        'seventy-seven lines are not collapsed — they are unreachable, which is worse than the ' +
        'wall this replaced. It rendered: ' +
        html.slice(0, 300),
    ).toBe(true);
  });

  it('draws it as prose, not as the output of something that failed', () => {
    const html = withBody();
    /* ZNACZNIK DOWOLNY, `data-line-body` obowiązkowy. Pierwsza wersja szukała `<p …>` i zapaliła
       się przy wpięciu renderera markdownu, który potrzebuje `<div>` — bo blok kodu i lista nie
       mieszczą się w akapicie. To kryterium pyta, CZYM ta treść jest narysowana, a nie jakim
       znacznikiem, więc kotwiczy na atrybucie, który jest jej adresem. */
    const opened = /<[a-z]+[^>]*data-line-body[^>]*>/.exec(html)?.[0] ?? '';

    expect(opened, 'the opened answer has no element of its own: ' + html.slice(0, 300)).not.toBe(
      '',
    );
    expect(
      /font-mono|border-l-fail/.test(opened),
      'monospace and a red left edge mean "a command failed" everywhere else in this view. An ' +
        "agent's answer drawn that way tells the person something went wrong when nothing did. " +
        'It carried: ' +
        opened,
    ).toBe(false);
    expect(
      /max-w-\[/.test(opened),
      'and it keeps the readable measure the mockup sets. Prose does not get wider just because ' +
        'it was opened: ' +
        opened,
    ).toBe(true);
    /* PRZEŁAMANIA SĄ TERAZ ROBOTĄ RENDERERA, nie CSS-u: akapit, lista i blok kodu przyjeżdżają
       jako osobne elementy, więc `whitespace-pre-line` na pojemniku nie ma czego zachowywać.
       Kryterium tamtej reguły stoi w `answer-is-elements-never-html.test.tsx`. */
    expect(
      html.includes('## Evidence'),
      'a heading shown literally is the noise this renderer exists to remove: ' + html,
    ).toBe(false);
  });

  it('gives a short answer no control at all', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([line.note(2, 0, FORGE, 'All green.')]);
    const row = feed.view.history[0];
    const html =
      row === undefined
        ? ''
        : renderToStaticMarkup(
            <Line
              row={row}
              onToggle={() => {
                /* Jak wyżej. */
              }}
            />,
          );

    expect(
      html.includes('Show more'),
      'an expand control on a two-word note is a step to take for nothing, and it makes the ' +
        'person wonder what is hidden when nothing is',
    ).toBe(false);
  });
});

/* ── I ŻE WIERSZ NAPRAWDĘ GO UŻYWA ──────────────────────────────────────────────────────────
 *
 * 2026-08-31 — TO KRYTERIUM POWSTAŁO PO PUSTEJ MUTACJI. Zdjęcie `<AnswerLine>` z `line.tsx` nie
 * zapaliło ani jednego z trzech kryteriów, które właśnie napisałem obok: wszystkie sądziły sam
 * komponent, więc były zielone także dla wersji, w której wiersz go nie woła. Komponent wpięty
 * w nic wygląda dokładnie tak samo, jak wpięty naprawdę — do chwili, gdy ktoś spojrzy na ekran.
 */
describe('the row itself draws its headline as markdown', () => {
  it('shows a heading opener as words, not as two hash characters', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([line.note(3, 0, FORGE, '## Backend: nothing to implement')]);
    const row = feed.view.history[0];
    const html =
      row === undefined
        ? ''
        : renderToStaticMarkup(
            <Line
              row={row}
              onToggle={() => {
                /* To kryterium pyta o markup, nie o skutek kliknięcia. */
              }}
            />,
          );

    expect(
      html.includes('Backend: nothing to implement'),
      'the headline carries the summary of the whole answer: ' + html,
    ).toBe(true);
    expect(
      html.includes('##'),
      "measured on the owner's own runs: four first lines out of six carry markdown. Raw, this " +
        'row shows those characters — and it is the row a person sees ALWAYS, not after a click. ' +
        'It rendered: ' +
        html,
    ).toBe(false);
  });
});
