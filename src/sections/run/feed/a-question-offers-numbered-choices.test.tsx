/* Pytanie, na którym stanął bieg, mówi KTO czeka, i daje ponumerowane odpowiedzi.
 *
 * # Skarga
 *
 * Właściciel, o poprzedniej wersji tego ekranu: „UX totalnie nieoczywisty". Karta pytania była
 * etykietą `Needs your answer` i rzędem szarych pastylek z surowym tekstem opcji. Nie mówiła
 * ani kto stanął, ani co się stanie po wybraniu — a bieg stoi na niej i kosztuje pieniądze,
 * dopóki człowiek nie odpowie.
 *
 * Makieta `polecenie.html` (`.ask`) odpowiada na to czterema rzeczami: nadoczkiem
 * `NEEDLE IS WAITING FOR YOU`, pytaniem w stopniu podtytułu, opcjami jako SZEROKIMI przyciskami
 * z NUMEREM w kwadracie i jednym zdaniem konsekwencji pod tytułem, oraz wierszem skrótów.
 *
 * # Skąd bierze się zdanie konsekwencji — i dlaczego nie jest zmyślone
 *
 * Z opcji, którą napisał agent. Rust wysyła `options: Vec<String>` i nic więcej
 * (`src/ipc/types.ts`, `asked`), więc drugie zdanie MUSI przyjechać w tym samym napisie albo
 * nie istnieć. Widok rozcina je na myślniku — tam, gdzie agent sam je rozdzielił — i **nie
 * dopisuje ani jednego słowa**: opcja bez myślnika ma sam tytuł i żadnego drugiego wiersza.
 * Zdanie konsekwencji wymyślone przez okno byłoby obietnicą, której nikt nie złożył
 * (niezmiennik 17), na kontrolce, która uruchamia pracę za pieniądze.
 *
 * # Dlaczego numery, i dlaczego mają naprawdę działać
 *
 * Bo bieg stoi. Klawisz jest najkrótszą drogą od przeczytania do odpowiedzi, a numer w kwadracie
 * jest jedyną rzeczą, która mówi, KTÓRY klawisz. Numer narysowany nad martwym nasłuchem jest
 * gorszy niż jego brak: obiecuje skrót, po którym nic się nie dzieje (niezmiennik 16). Dlatego
 * niżej stoi polityka klawisza, a `e2e/tests/a-key-answers-and-a-chip-narrows.spec.ts` naciska
 * go prawdziwą klawiaturą w prawdziwym chromium.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { answerForKey, choiceOf, keyMayAnswer } from './choice';
import { Feed } from './feed';
import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import type { FeedView } from './model';
import { createFeed } from './model';

const NEEDLE = 'Needle';
const QUESTION = 'Two checks still fail on the migration path. Skip them, or fix them first?';

const SKIP = 'Skip them and continue — Second reader gets the change as it stands';
const FIX = 'Fix them first — Builder gets another try before the checks re-run';

/** Opcja bez myślnika: agent nie napisał konsekwencji, więc jej nie ma. */
const BARE = 'Leave it alone';

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
        /* Bez jsdom kliknięcia nie ma jak wywołać; to kryterium pyta, czy jest CO nacisnąć. */
      }}
      onJumpToNewest={() => {
        /* Skok do najnowszego wiersza to inna kontrolka. */
      }}
    />,
  );
}

/** Bieg, który stanął na pytaniu z dwiema opcjami. */
function standingOnAQuestion(options: readonly string[] = [SKIP, FIX]): FeedView {
  const feed = createFeed(sealedScroller());
  feed.appendLines([line.note(1, 0, NEEDLE, 'Ran the checks on the changed workspace.')]);
  feed.appendLines([line.asked(2, 500, NEEDLE, QUESTION, options)]);
  return feed.view;
}

/** Cała treść przycisku odpowiedzi o tym numerze, bez znaczników. */
function choiceButton(markup: string, number: number): string {
  const found = new RegExp(
    '<button[^>]*\\bdata-choice="' + String(number) + '"[^>]*>([\\s\\S]*?)</button>',
  ).exec(markup);
  return (found?.[1] ?? '')
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('the question the run stands on names who is waiting and numbers the ways out', () => {
  it('says which agent is waiting, in its own words, above the question', () => {
    const markup = markupOf(standingOnAQuestion());

    expect(
      markup,
      'the card says a run needs an answer and never says WHOSE run stopped. With four agents ' +
        'going at once that sentence sends a person looking. Markup: ' +
        markup.slice(0, 600),
    ).toContain(NEEDLE + ' is waiting for you');
    expect(markup, 'and the question itself is still on the card').toContain(QUESTION);
  });

  it('draws each option as its own numbered control', () => {
    const markup = markupOf(standingOnAQuestion());

    expect(
      choiceButton(markup, 1),
      'the first way out has to be a control carrying the number 1, so the square on it and the ' +
        'key that presses it are the same fact. Markup: ' +
        markup.slice(0, 800),
    ).not.toBe('');
    expect(choiceButton(markup, 2), 'and the second one carries the number 2').not.toBe('');
    expect(
      choiceButton(markup, 3),
      'two options are two controls. A third numbered button over a two-option question offers ' +
        'an answer the agent never asked for',
    ).toBe('');
  });

  it('splits what the agent wrote into the action and what will follow from it', () => {
    expect(choiceOf(SKIP), 'the agent wrote both halves; the view only cuts where he cut').toEqual({
      title: 'Skip them and continue',
      consequence: 'Second reader gets the change as it stands',
    });
    expect(choiceOf(FIX)).toEqual({
      title: 'Fix them first',
      consequence: 'Builder gets another try before the checks re-run',
    });
    expect(
      choiceOf(BARE),
      'an option the agent wrote as one clause has no second sentence, and the view does not ' +
        'invent one: a consequence nobody promised is a lie on a control that spends money',
    ).toEqual({ title: BARE, consequence: '' });

    const markup = markupOf(standingOnAQuestion());
    expect(
      choiceButton(markup, 1),
      'both halves have to reach the screen: the action to press and the one sentence saying ' +
        'what it will do',
    ).toContain('Second reader gets the change as it stands');
    expect(
      choiceButton(markup, 1),
      'and the raw option text with its dash still in it is not what a person should read',
    ).not.toContain('—');
  });

  it('gives a key to every option, and no key to an option that is not there', () => {
    const options = [SKIP, FIX];

    expect(
      answerForKey('1', options),
      'pressing 1 has to answer with the first option, character for character — the run is ' +
        'standing still and the key is the shortest way from reading to answering',
    ).toBe(SKIP);
    expect(answerForKey('2', options)).toBe(FIX);
    expect(
      answerForKey('3', options),
      'a key past the last option answers nothing. Wrapping round would send the agent an ' +
        'answer the person did not choose',
    ).toBeNull();
    expect(answerForKey('a', options), 'a letter is not a way out of the question').toBeNull();
    expect(
      answerForKey('1', []),
      'a question with no options has no numbered ways out, so the key answers nothing rather ' +
        'than sending an empty sentence to the agent',
    ).toBeNull();
  });

  it('lets the key answer while the caret is idle, and gets out of the way once you type', () => {
    expect(
      keyMayAnswer({ modified: false, typing: false }),
      'with nothing typed anywhere, 1 has to answer — that is the whole reason the number is ' +
        'drawn on the button',
    ).toBe(true);
    expect(
      keyMayAnswer({ modified: false, typing: true }),
      'once a person has started writing, 1 is a character in their sentence. A shortcut that ' +
        'swallowed it would answer the agent with an option nobody chose, in the middle of a word',
    ).toBe(false);
    expect(
      keyMayAnswer({ modified: true, typing: false }),
      'a modifier belongs to somebody else: Cmd+1 switches section. Two meanings for one press ' +
        'is two things happening at once',
    ).toBe(false);
  });

  it('names under the card only the keys that really do something', () => {
    const markup = markupOf(standingOnAQuestion());
    /* Zamknięcie szukane po NAZWIE elementu, nie po pierwszym `</…>`: wiersz skrótów niesie
       zagnieżdżone `<span>`, więc leniwe dopasowanie urywało go na pierwszym z nich i sądziło
       jedną trzecią treści. */
    const hints = /<p[^>]*\bdata-answer-keys="[^"]*"[^>]*>([\s\S]*?)<\/p>/.exec(markup);
    const row = (hints?.[1] ?? '')
      .replace(/<[^>]*>/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();

    expect(
      row,
      'the mockup puts a row of shortcuts under the answer field, and it is the only thing on ' +
        'the screen that says the numbers do anything at all. Markup: ' +
        markup.slice(0, 800),
    ).not.toBe('');
    expect(row, 'the numbers answer').toContain('1 or 2 answer');
    expect(row, 'the slash opens the list of commands in the entry row').toContain('/ commands');
    expect(row, 'and the palette is reachable from anywhere').toContain('⌘K anywhere');
  });

  it('leaves the answer a person gave standing under the question it answered', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([line.asked(1, 0, NEEDLE, QUESTION, [SKIP, FIX])]);

    expect(
      markupOf(feed.view),
      'the option is on screen only as a button while the question is open; nothing below ' +
        'would mean anything if the sentence were already standing as a line',
    ).not.toContain('You → ' + NEEDLE);

    feed.answer(1, SKIP);
    const after = markupOf(feed.view);

    expect(
      after,
      'answering took the card away and left nothing behind. Pressing 1 then looks exactly ' +
        'like pressing a key that did nothing (DESIGN §8) — and the option chosen is the only ' +
        'record of which way the run was sent, so without it the transcript cannot answer why. ' +
        'Markup: ' +
        after.slice(0, 600),
    ).toContain('You → ' + NEEDLE);
    expect(
      after,
      'and it has to say WHICH way, word for word, not merely that an answer happened',
    ).toContain(SKIP);
  });

  it('takes the whole card away when the run goes down, keys and all', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([line.asked(1, 0, NEEDLE, QUESTION, [SKIP, FIX])]);

    expect(
      choiceButton(markupOf(feed.view), 1),
      'the card has to be there while the run really is waiting, or the emptiness below proves ' +
        'nothing',
    ).not.toBe('');

    feed.runEnded();
    const after = markupOf(feed.view);

    expect(
      choiceButton(after, 1),
      'the run is gone and the numbered buttons are still on screen. Every one of them answers ' +
        'an agent who stopped working, and the key row under them still says 1 and 2 answer — ' +
        'a person who steps away, comes back and presses 1 gets silence',
    ).toBe('');
    expect(
      after,
      'and the sentence naming who was waiting goes with it: nobody is waiting any more',
    ).not.toContain(NEEDLE + ' is waiting for you');
    expect(
      after,
      'the question itself stays in the transcript, because that part did happen',
    ).toContain(QUESTION);
  });
});
