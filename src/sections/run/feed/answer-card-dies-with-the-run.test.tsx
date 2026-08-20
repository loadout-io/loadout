/* Karta „Needs your answer" nie przeżywa biegu, który to pytanie zadał.
 *
 * ZMIERZONE. `./feed.tsx` rysuje tę kartę wyłącznie po `view.pinned !== null` i nie ma tam żadnej
 * bramki na to, czy bieg jeszcze żyje — a `runEnded()` nie tyka kolejki pytań. Więc pytanie
 * zadane przez agenta, na które człowiek nie odpowiedział przed Stopem albo przed błędem
 * (`../io.ts` woła `runEnded()` w `finally`, więc także przy odmowie), zostaje na ekranie razem
 * z kompletem kontrolek: przyciskiem opcji, polem na zdanie i przyciskiem wysyłki. Każda z nich
 * woła `answer()`, które zapisze odpowiedź i ustawi zdanie do przewiezienia agentowi, który nie
 * pracuje — kontrolka bez roboty (niezmiennik 16) przypięta do relacji, której w danych już nie
 * ma (niezmiennik 17).
 *
 * DLACZEGO NA MARKUPIE, A NIE NA SAMYM POLU MODELU. Bo to jest pytanie o to, co człowiek widzi,
 * a między polem i ekranem stoi warunek, którego nikt nie sprawdzał. Pole ma swoje kryterium
 * obok (`./nothing-live-survives-the-run.test.ts`) i ono odpowiada na inne pytanie: czy model
 * gasi CAŁĄ listę. To odpowiada na jedno — czy karta schodzi z ekranu.
 *
 * NAPRAWA NALEŻY DO MODELU, nie do tego komponentu. Drugi warunek w `./feed.tsx` („rysuj kartę,
 * jeśli przypięta ORAZ bieg żyje") byłby drugim miejscem, w którym mieszka odpowiedź na pytanie
 * „czy cokolwiek żyje" (niezmiennik 13), i zostawiłby `pinned` pełne widma dla następnego
 * czytającego. Kuracja mieszka w modelu (niezmiennik 15).
 *
 * SŁABA WERSJA: samo „po `runEnded()` karty nie ma". Przechodzi dla komponentu, który nie umie
 * jej narysować nigdy, i dla naprawy przez trwałe wyłączenie karty — a wtedy każdy punkt
 * kontrolny jest nieukończalny, czyli defekt gorszy od tego, który to zadanie zamyka.
 * Rozróżniają to dwa przypadki: karta JEST przed zejściem biegu i WRACA przy następnym pytaniu.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { ANSWER_PROMPT, Feed } from './feed';
import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import type { FeedView } from './model';
import { createFeed } from './model';

const FORGE = 'Forge';

const FORGE_SAID = 'Rewriting the splitter.';

/** Pytanie agenta. Bez apostrofów: markup je ucieka, asercja nie. */
const QUESTION = 'Should the old splitter stay behind a switch?';

/** Opcja, którą to pytanie podało — nazwa przycisku, który wołałby `answer()`. */
const OPTION = 'Yes, keep the old one behind a switch.';

/** Drugie pytanie, z następnego biegu. Inna treść, żeby żadna asercja nie trafiła jej mimochodem. */
const LATER_QUESTION = 'Which header row is the real one?';

/** Nazwa karty na ekranie [DESIGN §3: kolor `--attend` odpowiada na „co czeka na MOJĄ decyzję"]. */
const HEADING = 'Needs your answer';

/** Przycisk wysyłki karty. */
const SEND = 'Send';

/**
 * Markup CAŁEJ strefy pracy, z widoku, który dostaje ekran.
 *
 * Widok jedzie propsem, bo tak go dostaje ekran w działającej aplikacji, a model jest żywy
 * dłużej niż ekran. Port przewijania i uchwyty nie mają tu nic do roboty: to kryterium pyta
 * o markup, a co robi kliknięcie, ma swoje własne pliki — to repo nie ma jsdom.
 */
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
        /* Kliknięcia nie ma jak wywołać bez jsdom; to kryterium pyta, czy jest CO kliknąć. */
      }}
      onJumpToNewest={() => {
        /* Skok do najnowszego wiersza to inna kontrolka. */
      }}
    />,
  );
}

/**
 * Dostępne nazwy przycisków w tym markupie, w kolejności wystąpienia.
 *
 * Nazwa to `aria-label`, a kiedy go nie ma — widoczna treść przycisku. Tak samo czyta ją czytnik
 * ekranu. Ten sam pomocnik stoi w `./suggestion-has-a-button.test.tsx`: wspólne miejsce na niego
 * byłoby nowym plikiem w `./fixtures/`, a ten plik nie należy do tego zadania (AGENTS.md §7).
 */
function buttonNames(markup: string): readonly string[] {
  return [...markup.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)].map((hit) => {
    const attributes = hit[1] ?? '';
    const inside = hit[2] ?? '';
    const labelled = /aria-label="([^"]*)"/.exec(attributes);
    return (labelled === null ? inside.replace(/<[^>]*>/g, ' ') : (labelled[1] ?? '')).trim();
  });
}

/**
 * Z czego ta karta jest zrobiona na ekranie — WYPISANE, nie policzone.
 *
 * Cztery części, bo karta jest nagłówkiem i trzema kontrolkami, a każda z tych trzech woła
 * `answer()`. Lista części mówi, KTÓRA z nich została; licznik powiedziałby „coś zostało".
 *
 * TREŚĆ PYTANIA NIE JEST NA TEJ LIŚCIE i nie jest to przeoczenie: wiersz `asked` zostaje
 * w historii jako zapis tego, co się stało, więc zdanie agenta jest w markupie także po zejściu
 * biegu — i ma być. Karta to nie tekst pytania, to komplet kontrolek pod nim.
 */
function answerCard(view: FeedView): readonly string[] {
  const markup = markupOf(view);
  const names = buttonNames(markup);
  const parts: string[] = [];
  if (markup.includes(HEADING)) parts.push('the heading');
  if (names.includes(OPTION)) parts.push('the option button');
  if (markup.includes(ANSWER_PROMPT)) parts.push('the answer field');
  if (names.includes(SEND)) parts.push('the send button');
  return parts;
}

/** Wszystkie cztery części — tak wygląda karta, kiedy bieg naprawdę stoi na pytaniu. */
const WHOLE_CARD: readonly string[] = [
  'the heading',
  'the option button',
  'the answer field',
  'the send button',
];

/** Bieg, który zadał pytanie i nie dostał odpowiedzi — dokładnie stan, w którym ludzie klikają Stop. */
function askedAndWaiting() {
  const feed = createFeed(sealedScroller());
  feed.appendLines([line.note(1, 0, FORGE, FORGE_SAID)]);
  feed.appendLines([line.asked(2, 500, FORGE, QUESTION, [OPTION])]);
  return feed;
}

describe('the answer card does not outlive the run that asked', () => {
  it('draws the whole card while the run is standing on the question', () => {
    const feed = askedAndWaiting();

    expect(
      answerCard(feed.view),
      'the card is not on the screen while the run really is waiting for an answer, so every ' +
        'assertion below would be about a card this view never draws — and the emptiness after ' +
        'the run goes down would be the emptiness of a view that cannot draw it at all.',
    ).toEqual(WHOLE_CARD);
  });

  it('takes the card off the screen when the run goes down', () => {
    const feed = askedAndWaiting();

    feed.runEnded();

    expect(
      answerCard(feed.view),
      'the run is gone and the card is still there. Every part named here reaches an agent who ' +
        'stopped working: the buttons write an answer nobody will read and queue a sentence for ' +
        'delivery to nowhere. A person who steps away, comes back, and answers gets silence, ' +
        'which reads exactly like an app that is broken.',
    ).toEqual([]);
  });

  it('keeps the question itself in the transcript, because that part did happen', () => {
    const feed = askedAndWaiting();

    feed.runEnded();

    expect(
      markupOf(feed.view),
      'the sentence the agent asked has to stay readable after the run is over: it is part of ' +
        'the account of what happened. Clearing the whole stream to get rid of the card takes ' +
        'the transcript with it, and the transcript is what a person comes back to read.',
    ).toContain(QUESTION);
  });

  it('stops saying it is your turn once nobody is working and nothing is waiting', () => {
    const feed = askedAndWaiting();
    expect(feed.view.attention, 'it is your turn while the run stands on your answer').toBe('you');

    feed.runEnded();

    expect(
      feed.view.attention,
      'nothing is waiting on you, so the colour that answers "what needs MY decision" has ' +
        'nothing to point at. Left on, it sends a person looking for a decision that is not ' +
        'there — and it is the same one fact as the card above, so it cannot answer differently.',
    ).toBe('agents');
  });

  it('pins a card again when the next run asks, so switching it off is not a repair', () => {
    const feed = askedAndWaiting();
    feed.runEnded();

    feed.appendLines([line.note(3, 10_000, FORGE, FORGE_SAID)]);
    feed.appendLines([line.asked(4, 10_500, FORGE, LATER_QUESTION, [OPTION])]);

    expect(
      answerCard(feed.view),
      'the next run asked and the card did not come back. Turning the card off for good passes ' +
        'the case above and leaves every run that stops to ask a person unfinishable — the run ' +
        'stands there with nothing to answer it with, which is the defect this one replaces ' +
        'rather than fixes.',
    ).toEqual(WHOLE_CARD);
    expect(
      feed.view.attention,
      'and it is your turn again, because this time somebody really is waiting on you',
    ).toBe('you');
  });
});
