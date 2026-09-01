/* Wypowiedź w strumieniu jest PODPISANA: twarzą agenta, jego nazwą i godziną.
 *
 * # Skarga, z której to wyrosło
 *
 * Właściciel odrzucił dwie przebudowy tego ekranu słowami „nudne" i „UX totalnie nieoczywisty".
 * Zmierzona przyczyna po tej stronie: czterech agentów mówiących naraz było czterema wierszami
 * jednakowego szarego tekstu. Jedyną drogą do pytania „kto to powiedział i kiedy" było czytanie
 * liter — a strumień biegu, w którym pracuje czterech agentów, czyta się wtedy jak jeden monolog.
 *
 * Makieta `polecenie.html` (kolumna `.stream`) odpowiada na to trzema rzeczami przy KAŻDEJ
 * wypowiedzi: kwadratem z inicjałami w barwie tożsamości agenta (`.msg .sig`), jego nazwą
 * pogrubioną (`.msg .mh b`) i godziną w mono (`.msg .mh time`). Tego kryterium pilnuje.
 *
 * # Dlaczego to kryterium sądzi MARKUP CAŁEGO STRUMIENIA, a nie zwróconą wartość
 *
 * Niezmiennik 29. `initialsOf('Scout') === 'Sc'` dowodzi, że funkcja istnieje; nie dowodzi, że
 * ktokolwiek ją woła. Dokładnie tej klasy wady to repo pilnuje: kryterium zielone, funkcja
 * martwa. Dlatego scena jedzie przez `createFeed` → `Feed` — czyli przez ten sam model i ten sam
 * komponent, które w działającej aplikacji dostają linie z kanału Tauri.
 *
 * # Kontrola przeciw stałej
 *
 * Dwaj agenci, nie jeden. Implementacja rysująca ten sam kwadrat wszystkim (albo ten sam kolor)
 * przechodzi każdą asercję o obecności i przewraca się dopiero na PORÓWNANIU dwóch podpisów.
 * Godzina jest z tego samego powodu czytana z DWÓCH różnych stempli: zegar wpisany na stałe
 * i zegar odczytany w chwili renderu wyglądają na ekranie identycznie, a mówią o czym innym.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { identityToken } from '../rail/colour';
import { Feed } from './feed';
import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import type { FeedView } from './model';
import { createFeed } from './model';
import { clockOf, initialsOf } from './who';

const SCOUT = 'Scout';
const BUILDER = 'Builder';

const SCOUT_SAID = 'Opened the workspace and reproduced the failure on the first try.';
const BUILDER_SAID = 'Reused the open connection instead of opening a second one.';

/** Dwa różne stemple, żeby „godzina jest" nie przeszło na jednej wpisanej liczbie. */
const SCOUT_AT = Date.UTC(2026, 7, 31, 12, 0, 44);
const BUILDER_AT = Date.UTC(2026, 7, 31, 12, 4, 38);

function markupOf(view: FeedView): string {
  return renderToStaticMarkup(
    <Feed
      view={view}
      portRef={() => {
        /* Przewijanie ma swój własny plik. */
      }}
      onToggle={() => {
        /* Rozwijanie wiersza też. */
      }}
      onAnswer={() => {
        /* To kryterium pyta o podpis wypowiedzi, nie o odpowiadanie. */
      }}
      onJumpToNewest={() => {
        /* Skok do najnowszego wiersza to inna kontrolka. */
      }}
    />,
  );
}

/** Strumień, w którym odezwało się dwoje agentów — po jednym zdaniu każde. */
function twoVoices(): FeedView {
  const feed = createFeed(sealedScroller());
  feed.appendLines([line.note(1, SCOUT_AT, SCOUT, SCOUT_SAID)]);
  feed.appendLines([line.note(2, BUILDER_AT, BUILDER, BUILDER_SAID)]);
  return feed.view;
}

/** Znacznik otwierający elementu niosącego ten atrybut, razem z całym jego stylem. */
function openingTag(markup: string, attribute: string, value: string): string {
  return (
    new RegExp('<[a-z]+[^>]*\\b' + attribute + '="' + value + '"[^>]*>').exec(markup)?.[0] ?? ''
  );
}

describe('every sentence in the stream says who said it and when', () => {
  it('gives each speaker a square with their own initials', () => {
    const markup = markupOf(twoVoices());

    expect(
      initialsOf(SCOUT),
      'the square is two letters of the name, the way the mockup signs every message',
    ).toBe('Sc');
    expect(initialsOf(BUILDER)).toBe('Bu');
    expect(
      initialsOf('Second reader'),
      'a name of two words is signed with the first letter of each, not the first two letters ' +
        'of the first word — otherwise Second reader and Scout share a square',
    ).toBe('Sr');

    expect(
      openingTag(markup, 'data-sig', initialsOf(SCOUT)),
      'no square carries ' +
        SCOUT +
        "'s initials, so four agents talking at once are four identical grey paragraphs and " +
        'the only way to the question "who said this" is reading the letters. Markup: ' +
        markup.slice(0, 400),
    ).not.toBe('');
    expect(
      openingTag(markup, 'data-sig', initialsOf(BUILDER)),
      'only one of the two speakers got a square. A version that signs the first message and ' +
        'nothing else looks right on a screenshot of one line',
    ).not.toBe('');
  });

  it('paints that square in the identity colour of that agent, and not one colour for all', () => {
    const markup = markupOf(twoVoices());
    const scout = openingTag(markup, 'data-sig', initialsOf(SCOUT));
    const builder = openingTag(markup, 'data-sig', initialsOf(BUILDER));

    expect(
      scout,
      'the square has to carry ' +
        SCOUT +
        "'s identity colour (" +
        identityToken(SCOUT) +
        '), the SAME map the tile in the agents list lives from — a second map here would ' +
        'paint the same agent two colours on one screen (invariant 13). It rendered: ' +
        scout,
    ).toContain(identityToken(SCOUT));
    expect(
      builder,
      'and ' + BUILDER + "'s square has to carry " + BUILDER + "'s colour, not " + SCOUT + "'s",
    ).toContain(identityToken(BUILDER));
    expect(
      identityToken(SCOUT) === identityToken(BUILDER),
      'this scene only means something while the two agents have different identity colours; ' +
        'if the map ever gives these two names the same colour, pick two other names',
    ).toBe(false);
  });

  it('signs the sentence with the clock of the line, not of the render', () => {
    const markup = markupOf(twoVoices());

    expect(
      clockOf(SCOUT_AT),
      'the clock is hours, minutes and seconds of the moment the line arrived',
    ).toMatch(/^\d{2}:\d{2}:\d{2}$/);
    expect(
      clockOf(SCOUT_AT) === clockOf(BUILDER_AT),
      'two lines four minutes apart cannot read as the same time',
    ).toBe(false);

    expect(
      markup,
      'the mockup signs every message with a time and this stream carries none, so a person ' +
        'reading it back cannot tell whether two sentences are seconds or an hour apart. It ' +
        'has to be the stamp the boundary put on the line: a clock read at render time changes ' +
        'on every repaint and says nothing about the run (invariant 17). Markup: ' +
        markup.slice(0, 400),
    ).toContain('>' + clockOf(SCOUT_AT) + '<');
    expect(markup, 'the second line carries its own time, not the first one repeated').toContain(
      '>' + clockOf(BUILDER_AT) + '<',
    );
  });

  it('opens the stream with a rule saying when the run started, not with somebody speaking', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([line.run(1, SCOUT_AT, 'Loadout', 'Run started — Ship a feature · 4 steps')]);
    feed.appendLines([line.note(2, BUILDER_AT, SCOUT, SCOUT_SAID)]);
    const markup = markupOf(feed.view);

    expect(
      /* `data-start-line` bez wartości renderuje się jako `="true"` — atrybut logiczny Reacta. */
      openingTag(markup, 'data-start-line', 'true'),
      'the mockup opens the stream with a hairline rule carrying the run and its clock — a ' +
        'boundary, not a sentence. Drawn as an ordinary message it would be signed with a ' +
        "square, and the start of the run is nobody's utterance. Markup: " +
        markup.slice(0, 400),
    ).not.toBe('');
    expect(
      markup,
      'and that rule carries the clock the run started at, so a transcript read back says when',
    ).toContain('>' + clockOf(SCOUT_AT) + '<');
    expect(
      openingTag(markup, 'data-sig', initialsOf('Loadout')),
      'the opening rule got a signature square, so the start of the run reads as a sentence ' +
        'somebody said',
    ).toBe('');
  });

  it('writes the name of the speaker beside that square, in their colour', () => {
    const markup = markupOf(twoVoices());

    expect(
      openingTag(markup, 'data-who', SCOUT),
      'the square is a help for the eye, never an identifier: two agents past the fifth share a ' +
        'colour on purpose. The name has to stand beside it or the stream stops saying who is ' +
        'talking the moment a sixth agent joins. Markup: ' +
        markup.slice(0, 400),
    ).not.toBe('');
    expect(openingTag(markup, 'data-who', BUILDER)).not.toBe('');
  });
});
