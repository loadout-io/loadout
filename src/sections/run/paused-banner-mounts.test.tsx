/* AC-5 dla T-39: pasek pauzy pokazuje się wtedy i TYLKO wtedy, gdy bieg czeka na dostawcę.
 *
 * SŁABA WERSJA: wyrenderować `PausedBanner` wprost. To samo pytanie, co przy liście agentów —
 * „istnieje" to nie „zamontowany", a `limits/paused-banner.tsx` miał od T-21 komplet testów
 * i ani jednego miejsca montowania. Dlatego renderujemy CAŁY ekran Run, a atrapa modułu jest
 * przelotką: prawdziwy pasek dalej się rysuje, a test widzi, z czym ekran go zawołał.
 *
 * „TYLKO WTEDY" JEST TU POŁOWĄ KRYTERIUM. Pasek, który raz się pojawi i zostaje do końca biegu,
 * przechodzi każdą asercję o obecności i kłamie przez resztę biegu: mówi „czekamy na limit"
 * nad agentem, który od dziesięciu minut pisze kod. Stąd trzeci stan w tym pliku — bieg, który
 * po pauzie znowu coś powiedział.
 *
 * SKĄD BIERZE SIĘ PAUZA. Z linii `problem` i jej pola `resetsAt` (`src/ipc/types.ts`, lustro
 * `engine/line.rs`) — magazyn nie ma pola „wstrzymany", więc zasiew idzie tą samą drogą, którą
 * jadą linie z Rusta: `useRun.appendLines`. Test, który wstrzyknąłby ekranowi gotową flagę,
 * mierzyłby własną atrapę.
 *
 * MOMENT WZNOWIENIA CZYTAMY Z KOMPONENTU, nie z literału: zdanie składa `paused-banner.tsx`
 * z `Intl.DateTimeFormat` w strefie czytelnika, a godzina przepisana do testu byłaby poprawna
 * wyłącznie na tej maszynie i tylko do zmiany czasu.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { FeedLine } from '../../state/run';
import { useRun } from '../../state/run';
import type { PausedBannerProps } from './limits/paused-banner';

const { seen } = vi.hoisted(() => ({ seen: [] as unknown[] }));

vi.mock('./limits/paused-banner', async (importOriginal) => {
  const real = await importOriginal<typeof import('./limits/paused-banner')>();
  return {
    ...real,
    PausedBanner: (props: PausedBannerProps) => {
      seen.push(props);
      return real.PausedBanner(props);
    },
  };
});

const Run = (await import('./index')).default;
const actual =
  await vi.importActual<typeof import('./limits/paused-banner')>('./limits/paused-banner');

/** Kiedy limit wraca — sekundy uniksowe, tak jak jadą z drutu. */
const RESETS_AT = 1_786_800_600;

/** Linia, którą Rust wysyła, kiedy dostawca każe czekać. */
const PAUSE: FeedLine = {
  kind: 'problem',
  agent: 'Build',
  text: 'Claude is out of usage for now',
  resetsAt: RESETS_AT,
  id: 1,
  at: 1_000,
};

/** Cokolwiek, co wydarzyło się PO pauzie — czyli dowód, że dostawca znowu odpowiada. */
const AFTER: FeedLine = {
  kind: 'note',
  agent: 'Build',
  text: 'Back to the parser.',
  id: 2,
  at: 2_000,
};

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function propsAfterRender(): PausedBannerProps | undefined {
  return seen.at(-1) as PausedBannerProps | undefined;
}

const sendingMarkup = renderToStaticMarkup(<Run />);
const sendingProps = propsAfterRender();

useRun.getState().appendLines([PAUSE]);
const pausedMarkup = renderToStaticMarkup(<Run />);
const pausedProps = propsAfterRender();

useRun.getState().appendLines([AFTER]);
const resumedMarkup = renderToStaticMarkup(<Run />);

/** Zdanie, które ten pasek napisałby o tej chwili — policzone przez komponent, nie przez test. */
const sentence = renderToStaticMarkup(
  <actual.PausedBanner run={{ waitingUntil: RESETS_AT, steps: [] }} />,
).replace(/<[^>]*>/g, '');

describe('the pause banner is on screen when the run waits, and only then', () => {
  it('shows no banner while the run is sending', () => {
    expect(
      sendingProps,
      'the run screen never rendered the banner at all, so its absence says nothing about ' +
        'the state of the run — an unmounted component is invisible for the same reason a ' +
        'quiet one is.',
    ).toBeDefined();
    expect(
      sendingProps === undefined ? 'not handed over' : sendingProps.run.waitingUntil,
      'a run that is sending has no moment to wait for, and the screen has to hand the banner ' +
        'exactly that',
    ).toBeNull();
    expect(
      occurrences(sendingMarkup, 'data-paused-banner'),
      'a run that is sending must not carry the waiting sentence. An empty banner holding its ' +
        'place teaches people to stop reading that part of the window.',
    ).toBe(0);
  });

  it('shows it when Claude said wait, and names the moment it comes back', () => {
    expect(
      sentence.trim(),
      'the banner component wrote no sentence for this moment, so "the screen carries it" ' +
        'would be a statement about an empty string.',
    ).not.toBe('');
    expect(
      occurrences(pausedMarkup, 'data-paused-banner'),
      'one paused run, one banner (invariant 13). Zero means nobody mounted it; two means the ' +
        'same sentence lives in two places and the first disagreement between them is silent.',
    ).toBe(1);
    expect(
      pausedMarkup,
      'the banner has to carry the moment the limit comes back, in the reader own clock. The ' +
        'sentence the component writes for this run is ' +
        JSON.stringify(sentence) +
        '; neither the number from the wire nor its machine spelling may reach the screen.',
    ).toContain(sentence);
  });

  it('takes it down again as soon as the run says anything else', () => {
    expect(
      occurrences(resumedMarkup, 'data-paused-banner'),
      'after the run said something new the banner has to go. A banner that stays for the rest ' +
        'of the run passes every assertion about being shown and lies for the rest of the run: ' +
        'it says "waiting for the limit" above an agent who has been writing code for ten ' +
        'minutes.',
    ).toBe(0);
  });

  it('shows the banner from limits/paused-banner.tsx, not a second copy of that sentence', () => {
    expect(
      pausedProps,
      'the screen has to render the component from src/sections/run/limits/paused-banner.tsx. ' +
        'Nothing recorded means it drew the sentence itself — a second place where "the run is ' +
        'waiting" is said, and the one that will drift (invariant 13).',
    ).toBeDefined();
    expect(
      pausedProps === undefined ? 'not handed over' : pausedProps.run.waitingUntil,
      'the screen has to hand the banner the moment that came from the stream, not one of its ' +
        'own making',
    ).toBe(RESETS_AT);
  });
});
