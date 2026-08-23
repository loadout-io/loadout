/* Przycisk Start niesie ZADANIE, a nie tylko wybór pliku.
 *
 * # Co było zepsute
 *
 * `launchRun` przyjmuje zadanie trzecim argumentem od dawna, a `/run <workflow> <zadanie>` je
 * podaje. Przycisk Start wołał tę samą funkcję DWOMA argumentami, więc do `Setup.task` po stronie
 * Rusta szedł pusty napis — a `with_the_task` przy pustym zadaniu oddaje prompt kroku co do
 * bajtu, bez nagłówka „What the person asked for". Bieg z przycisku ruszał na pustce.
 *
 * Zmierzone na biegu `20260823-010248` właściciela: manifest pierwszego kroku bez pozycji
 * `run/task`, odpowiedź agenta „Please send the research prompt or topic you want analyzed"
 * (207 tokenów wyjścia), status `succeeded`, i cały graf 22 kroków puszczony za tym.
 *
 * # Dlaczego to kryterium wygląda tak
 *
 * SŁABĄ WERSJĄ jest `expect(markup).toContain('What should this run build?')`. Przechodzi ją
 * ekran, na którym pole stoi, wygląda dobrze i NIE JEST PODŁĄCZONE — czyli dokładnie ta rodzina
 * wady, z której wzięło się siedemnaście kłamiących kontrolek opisanych w `./launch`. Dlatego
 * pierwsza połowa pyta o kontrolkę, a druga woła TO, CO WOŁA PRZYCISK, i sprawdza, co z tego
 * wyszło na krawędź.
 *
 * To repo nie ma jsdom, więc kliknięcia nie da się odpalić. Polityka startu jest z tego powodu
 * funkcją (`startWhatIsChosen`), a nie ciałem `onClick` — i to jest ten sam wybór, który
 * `./launch` opisuje w swoim nagłówku.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Choice } from './choices';

const { launched } = vi.hoisted(() => ({
  launched: vi.fn((..._sent: unknown[]) => Promise.resolve<string | null>(null)),
}));

vi.mock('./launch', () => ({ launchRun: launched }));

/* Ten sam powód, co w `lead-replaces-the-picker`: moduły po drodze zakładają kanał przy
 * wczytaniu, a prawdziwy woła okno, którego tu nie ma. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => new Promise(() => undefined)),
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { Start, TASK_LABEL, startWhatIsChosen } = await import('./start');

/** Nazwa pliku, który ma pojechać do Rusta. */
const OPEN = 'deep-research.json';

/** „Ile naraz". Nie trójka — domyślną łatwo wpisać na sztywno i nie zauważyć. */
const AT_ONCE = 5;

const CHOICES: readonly Choice[] = [
  {
    path: OPEN,
    name: 'Deep research',
    steps: [{ id: 's1', name: 'Plan steps', state: 'pending' as const }],
  },
];

/** Zadanie z odstępami po bokach — przycinanie jest częścią umowy, nie kosmetyką. */
const TASK = 'which districts of Gdansk are best to live in';

function markup(): string {
  return renderToStaticMarkup(
    <Start
      onSaid={() => {
        /* Kanał raportowania musi istnieć, bo wymaga go typ; to kryterium nie pyta o zdania. */
      }}
    />,
  );
}

/** Ile kontrolek w tym markupie nosi tę nazwę. */
function named(html: string, name: string): number {
  return html.split('aria-label="' + name + '"').length - 1;
}

describe('the Run button carries what the person asked for', () => {
  beforeEach(() => {
    launched.mockClear();
  });

  it('puts exactly one field for the task in the run controls', () => {
    expect(
      named(markup(), TASK_LABEL),
      'without a field there is no way to say what a run should build except by typing ' +
        '/run in the line below, and the button quietly starts every run with nothing asked ' +
        'of it. Two fields would be two answers to one question',
    ).toBe(1);
  });

  it('sends what was typed as the third thing the start policy is told', () => {
    void startWhatIsChosen(CHOICES, OPEN, AT_ONCE, '  ' + TASK + '  ');

    expect(launched, 'the button has to go through the one start policy').toHaveBeenCalledTimes(1);
    const sent = launched.mock.calls[0] ?? [];
    expect(
      sent[1],
      'the limit still has to travel; a fix that carries the task and drops the limit trades ' +
        'one silent default for another',
    ).toBe(AT_ONCE);
    expect(
      sent[2],
      'this is the whole defect: the task reaches the start policy, trimmed. Sent with two ' +
        'arguments it is undefined here, Rust sees an empty task, and every step gets its ' +
        'prompt with nothing asked of it',
    ).toBe(TASK);
  });

  it('turns a task of nothing but spaces into no task at all', () => {
    void startWhatIsChosen(CHOICES, OPEN, AT_ONCE, '   ');

    expect(
      launched.mock.calls[0]?.[2],
      'no task and a task of spaces are one fact — nothing was asked — and two different ' +
        'prompts for one fact are two different answers to what this run builds',
    ).toBeNull();
  });
});
