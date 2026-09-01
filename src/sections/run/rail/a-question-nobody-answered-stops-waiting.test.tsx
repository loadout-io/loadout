/* Kafelek przestaje czekać na odpowiedź, kiedy bieg zszedł i już jej nie będzie.
 *
 * ZMIERZONA WADA. Wiersz `asked` zostaje w historii na zawsze — „że agent zapytał" naprawdę
 * się wydarzyło. `./say.ts` odwzorowywał ten wiersz na jedno zdanie Loadouta i robił to
 * bezwarunkowo, więc kafelek kroku mówił „Waiting for your answer" długo po tym, jak bieg
 * zszedł z odmową, ze Stopem albo po prostu się skończył. Zdanie opisywało stan sprzed
 * zakończenia i nie było na ekranie niczego, co by to prostowało: kolejka pytań gaśnie
 * (`../feed/model.ts`, `runEnded`), więc karta z przyciskami znika, a zdanie zostaje.
 *
 * DLACZEGO ZDANIEM PO ZEJŚCIU BIEGU JEST SAMO PYTANIE. Kiedy pytanie stoi, ma na ekranie JEDNO
 * żywe miejsce — kartę z przyciskami przy kroku — i powtórzone na kafelku byłoby drugim domem
 * jednego faktu (niezmiennik 13); dlatego kafelek mówi wtedy, na co się czeka, a nie o co.
 * Kiedy pytanie przestaje stać, tamtej karty nie ma wcale, więc nie ma czego powtarzać —
 * a ostatnią rzeczą, jaką ten agent naprawdę powiedział, jest właśnie to pytanie. Zdanie
 * wymyślone na tę chwilę byłoby trzecim wariantem tekstu tam, gdzie wystarczy zdjąć wyjątek.
 *
 * DLACZEGO CAŁY EKRAN, A NIE SAMA FUNKCJA. Wartość zwrócona przez funkcję dowodzi, że mechanizm
 * istnieje; zdanie w markupie dowodzi, że dochodzi do człowieka (niezmiennik 29). Ta rodzina
 * plików — `roster.ts`, `card.ts`, `say.ts` — przez trzydzieści zadań miała komplet kryteriów
 * i ani jednej drogi na ekran, więc mierzymy tutaj markup `<Run />`.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Step } from '../../../state/run';
import { useRun } from '../../../state/run';
import { line } from '../feed/fixtures/lines';
import { runFeed } from '../feed/live';
import Run from '../index';

const BUILD = 'Build';

/* Podpis agenta w strumieniu JEST nazwą kroku — na tym jednym polu spotykają się plan
 * i strumień. Bez pozycji, bo to repo nie ma jsdom, a bez niej obraz oddaje LISTĘ kroków tym
 * samym kafelkiem: to jest droga, na której kafelek widać w tym środowisku. */
const RUNNING: readonly Step[] = [{ id: 's_build', name: BUILD, state: 'running' }];
const STOPPED: readonly Step[] = [{ id: 's_build', name: BUILD, state: 'cancelled' }];

const QUESTION = 'The row has more columns than the header. What should it do?';
const WAITING = 'Waiting for your answer';

useRun.setState({ workflow: 'Fix the CSV parser', steps: RUNNING, links: null });
runFeed.appendLines([
  line.asked(1, 0, BUILD, QUESTION, ['Drop the extra columns', 'Fail the whole file']),
]);
const standing = renderToStaticMarkup(<Run />);

/* Bieg schodzi bez odpowiedzi: kolejka pytań gaśnie, krok jest odwołany. */
runFeed.runEnded();
useRun.setState({ steps: STOPPED });
const over = renderToStaticMarkup(<Run />);

/** Markup jednego kafelka: od jego znacznika do znacznika następnego. */
function tileOf(markup: string, id: string): string {
  const opens = markup.indexOf('data-step="' + id + '"');
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const next = rest.indexOf('data-step="', 1);
  return next < 0 ? rest : rest.slice(0, next);
}

/** Tekst, który człowiek na tym kafelku przeczyta — bez znaczników. */
function textOf(piece: string): string {
  return piece
    .slice(piece.indexOf('>') + 1)
    .replace(/<[^>]*>/g, ' ')
    .replace(/<[^>]*$/, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('zdanie o pytaniu po zejściu biegu', () => {
  it('puts the step on the screen in both moments, so both readings have a card', () => {
    expect(tileOf(standing, 's_build'), 'no card for the step while the run is live').not.toBe('');
    expect(tileOf(over, 's_build'), 'no card for the step once the run is over').not.toBe('');
  });

  it('says it is waiting on you while the question is still standing', () => {
    expect(
      textOf(tileOf(standing, 's_build')),
      'while the run is live and nobody has answered, the card has to say what the run is ' +
        'waiting for. Without this reading the point below would pass over a card that never ' +
        'said it in the first place',
    ).toContain(WAITING);
  });

  it('stops saying it is waiting on you once the run is over', () => {
    expect(
      textOf(tileOf(over, 's_build')),
      'the run is over and this question will never be answered, and the card still reads ' +
        JSON.stringify(textOf(tileOf(over, 's_build'))) +
        '. That sentence describes the moment before the ending, not this one: nothing is ' +
        'waiting, the buttons are gone, and a person reading it looks for something to press',
    ).not.toContain(WAITING);
  });

  it('says instead what this agent actually asked, so the line is not empty either', () => {
    expect(
      textOf(tileOf(over, 's_build')),
      'with the waiting sentence gone the card has to carry the last thing this agent really ' +
        'said, which is the question itself. An empty line reads as a broken step, and an ' +
        'invented sentence is a third wording of one fact',
    ).toContain(QUESTION);
  });
});
