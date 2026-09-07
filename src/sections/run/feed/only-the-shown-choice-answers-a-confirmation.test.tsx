/* KARTA POTWIERDZENIA NIE PRZYJMUJE ZDANIA, KTÓRE NIE MOŻE NICZEGO POTWIERDZIĆ.
 *
 * CO SIĘ DZIAŁO. Lider pyta „Stop this run?" i pokazuje dwa przyciski. Pod nimi stało pole
 * „Your answer" z przyciskiem „Send" — a zgodę na tę operację wystawia host WYŁĄCZNIE na
 * dosłowny napis przycisku (`src-tauri/src/bridge/library.rs`, `answer_exact`, porównanie
 * `affirmative` co do bajta; napis pochodzi z `bridge/library/control.rs`, `replay.rs`,
 * `result_restore.rs`, `services.rs`). Człowiek, który wpisał „yes, stop it" i nacisnął Send,
 * widział więc, jak pytanie znika z ekranu — okno zdejmuje przypięcie, bo most przyjął treść —
 * a pod jego własnym zdaniem stawał wiersz „The person did not approve that operation. Nothing
 * changed.". Aplikacja zaprzeczała człowiekowi, który właśnie się zgodził, i zabierała mu przy
 * tym jedyną kontrolkę, którą mógł zgodzić się skutecznie: przyciski schodziły razem z kartą.
 * Żeby spróbować jeszcze raz, musiał poprosić lidera o to samo pytanie od nowa, a bieg przez
 * ten czas szedł dalej i kosztował pieniądze.
 *
 * DLACZEGO KRYTERIUM SĄDZI MARKUP CAŁEJ STREFY. Bo pytanie brzmi „co człowiek ma przed sobą",
 * a między modelem a ekranem stoi warunek, którego nikt nie sprawdzał. Ta sama droga, co
 * w `./answer-card-dies-with-the-run.test.tsx`: widok jedzie propsem, tak jak dostaje go ekran.
 *
 * SŁABE WERSJE, KTÓRE TE PRZYPADKI ODRZUCAJĄ — bo każda z nich jest gorsza od wady:
 *   1. „pola nie ma nigdy" — punkt kontrolny biegu przestaje być dokończalny (wada naprawiona
 *      2026-08-18: `commands::run::ask` wysyła `options: Vec::new()`, więc bez pola karta nie
 *      miałaby ANI JEDNEJ kontrolki);
 *   2. „pola nie ma, kiedy są przyciski" — zabiera własne słowa pytaniu agenta, gdzie są one
 *      całą odpowiedzią, i punktowi kontrolnemu, który kiedyś zaproponuje warianty;
 *   3. „pola nie ma przy każdym pytaniu hosta" — punkt kontrolny biegu też jedzie z adresem
 *      (`operation: 'continue_run'`), a tam host zachowuje oryginał człowieka co do bajta
 *      (`affirmative: None`).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { FeedLine } from '../../../state/run';
import { ANSWER_PROMPT, Feed } from './feed';
import { sealedScroller } from './fixtures/scroller';
import type { FeedView } from './model';
import { createFeed } from './model';

const LEAD = 'Lead';
const WORKER = 'Forge';

const RUN = '01980000-0000-7000-8000-000000000001';
const ADDRESS = '01980000-0000-7000-8000-000000000010';

/** Pytanie, na którym stoi Stop biegu. Bez apostrofów: markup je ucieka, asercja nie. */
const STOP_ASKED = 'Stop Review the result? Its unfinished work will end.';

/** Oba przyciski karty Stop — pierwszy z nich jest zarazem napisem, na który host mintuje zgodę. */
const STOP_OPTIONS = ['Stop run', 'Keep running'] as const;

/** Punkt kontrolny biegu: pytanie, na które odpowiada się własnymi słowami. */
const CHECKPOINT_ASKED = 'Which header row is the real one?';

/** Pytanie agenta — bez adresu hosta, więc każde zdanie jest dla niego odpowiedzią. */
const WORKER_ASKED = 'Should the old splitter stay behind a switch?';

/** Opcje, które proponuje pytanie przyjmujące zarazem własne słowa. */
const SUGGESTED = ['Keep it', 'Drop it'] as const;

/** Wiersz `asked` w kształcie, w jakim przyjeżdża z drutu — z adresem hosta albo bez niego. */
function asked(
  id: number,
  agent: string,
  text: string,
  options: readonly string[],
  operation?: string,
): FeedLine {
  return {
    kind: 'asked',
    agent,
    text,
    options: [...options],
    id,
    at: id * 100,
    ...(operation === undefined
      ? {}
      : {
          question: {
            questionId: ADDRESS,
            runId: RUN,
            checkpointId: operation === 'continue_run' ? ADDRESS : null,
            operation,
          },
        }),
  };
}

/** Widok strefy pracy z jednym nieodpowiedzianym pytaniem — stan, w którym stoi bieg. */
function standingOn(line: FeedLine): FeedView {
  const feed = createFeed(sealedScroller());
  feed.appendLines([line]);
  return feed.view;
}

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
        /* To repo nie ma jsdom; to kryterium pyta, czy jest CO nacisnąć. */
      }}
      onJumpToNewest={() => {
        /* Skok do najnowszego wiersza to inna kontrolka. */
      }}
    />,
  );
}

/** Nazwy przycisków w markupie — tak samo, jak czyta je czytnik ekranu. */
function buttonNames(markup: string): readonly string[] {
  return [...markup.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)].map((hit) => {
    const attributes = hit[1] ?? '';
    const inside = hit[2] ?? '';
    const labelled = /aria-label="([^"]*)"/.exec(attributes);
    return (labelled === null ? inside.replace(/<[^>]*>/g, ' ') : (labelled[1] ?? '')).trim();
  });
}

/**
 * Czym CZŁOWIEK może odpowiedzieć na to pytanie — wypisane, nie policzone.
 *
 * Kotwicą pola jest jego zachęta (`ANSWER_PROMPT`), a nie sama obecność `<input>`: pole wejścia
 * całego ekranu tutaj nie stoi, ale gdyby kiedyś stanęło, licznik `<input>` mówiłby o nim.
 */
function waysToAnswer(view: FeedView): readonly string[] {
  const markup = markupOf(view);
  const names = buttonNames(markup);
  const ways: string[] = [];
  for (const option of [...STOP_OPTIONS, ...SUGGESTED]) {
    if (names.some((name) => name.includes(option))) ways.push('the button ' + option);
  }
  if (markup.includes(ANSWER_PROMPT)) ways.push('a field for your own words');
  if (names.includes('Send')) ways.push('the send button');
  return ways;
}

describe('only the shown choice answers a confirmation', () => {
  it('offers the two buttons and no field when consent is one exact label', () => {
    expect(
      waysToAnswer(standingOn(asked(1, LEAD, STOP_ASKED, STOP_OPTIONS, 'stop_run'))),
      'the card invites a person to write their own sentence about stopping the run, and no ' +
        'sentence they can write will stop it: this operation is authorized by the exact label ' +
        'on the button and by nothing else. What they get for answering in their own words is ' +
        'their question taken off the screen and a line under their own sentence saying they ' +
        'did not approve — the app contradicting the person who just agreed, and taking away ' +
        'the only control that would have worked.',
    ).toEqual(['the button Stop run', 'the button Keep running']);
  });

  it('keeps the field where a run stands on a checkpoint, because own words are the answer', () => {
    expect(
      waysToAnswer(standingOn(asked(2, WORKER, CHECKPOINT_ASKED, [], 'continue_run'))),
      'the run is standing on a question and there is nothing on the card to answer it with. ' +
        'Every workflow that pauses to ask a person is unfinishable in this state — which is a ' +
        'worse defect than the one this file closes, and the way to arrive at it is to take the ' +
        'field away from everybody instead of from the card that cannot use it.',
    ).toEqual(['a field for your own words', 'the send button']);
  });

  it('keeps the field at a checkpoint that also suggests answers', () => {
    expect(
      waysToAnswer(standingOn(asked(3, WORKER, CHECKPOINT_ASKED, SUGGESTED, 'continue_run'))),
      'the suggestions arrived and the way to say something else disappeared. A checkpoint ' +
        'keeps the original words of the person byte for byte, so a shortcut that reads "there ' +
        'are buttons, therefore the buttons are the only answer" is wrong exactly here.',
    ).toEqual([
      'the button Keep it',
      'the button Drop it',
      'a field for your own words',
      'the send button',
    ]);
  });

  it('keeps the field when the question comes from an agent, not from the host', () => {
    expect(
      waysToAnswer(standingOn(asked(4, WORKER, WORKER_ASKED, SUGGESTED))),
      'an agent asked and the person can only pick one of its two suggestions. Nothing here is ' +
        'authorizing anything: the agent reads whatever sentence it gets, and answering in your ' +
        'own words is most of what this card is for.',
    ).toEqual([
      'the button Keep it',
      'the button Drop it',
      'a field for your own words',
      'the send button',
    ]);
  });

  it('never leaves a confirmation with nothing to press', () => {
    expect(
      waysToAnswer(standingOn(asked(5, LEAD, STOP_ASKED, [], 'stop_run'))),
      'a question is waiting and the card carries no control at all. Whatever sent this one ' +
        'without buttons, the person is left looking at a sentence that stops the run and has ' +
        'no way to answer it — the field goes away only when something else stays.',
    ).toEqual(['a field for your own words', 'the send button']);
  });
});
