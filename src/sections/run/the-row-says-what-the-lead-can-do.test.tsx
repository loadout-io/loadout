/* Z-50: zdanie pod polem rozmowy mówi to, co lider NAPRAWDĘ może.
 *
 * PO CO TO ISTNIEJE. Do 2026-09-04 stało tam jedno zdanie na wszystkie loadouty: „Enter sends
 * this to the lead agent — it can talk things through and prepare, but only /run starts work."
 * Lider z dialem „work freely" jechał tymczasem z ośmioma narzędziami, `Bash`, `Edit` i `Write`
 * włącznie. Zmierzone w rozmowie z 2026-09-01: ten sam lider zrobił sobie kopię repozytorium,
 * zmienił kod, uruchomił testy i zatrzymał dwa kroki. Ekran obiecywał rozmowę i przygotowanie,
 * a dostawał człowiek agenta, który pisze po jego plikach (niezmiennik 4).
 *
 * DLACZEGO PYTAMY EKRAN, A NIE FUNKCJI. Wartość zwrócona przez `whereItGoes` dowodzi, że
 * mechanizm istnieje; zdanie w markupie dowodzi, że doszło tam, gdzie czyta je człowiek
 * (niezmiennik 29). Ten plik montuje więc CAŁY ekran pracy i czyta `data-entry-hint` — tym samym
 * wyrażeniem, którym czyta je `./entry-row.test.tsx`.
 *
 * SŁABA WERSJA: `expect(hint).toContain('commands')` przy jednym ustawieniu. Przechodzi dla
 * wiersza, który mówi to samo zdanie zawsze — czyli dla dokładnie tej wady, którą to zadanie
 * zdejmuje. Rozstrzygają trzy rzeczy: zdanie dla lidera BEZ tych narzędzi jest dzisiejszym
 * zdaniem znak w znak, zdanie zmienia się po samej zmianie odpowiedzi granicy, a granica folderu
 * znika dokładnie wtedy, kiedy lider nie jest do folderu przywiązany.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';

/* Transport. Ekran pracy i tak go nie dotknie w renderze statycznym — `useEffect` się nie
 * uruchamia — ale moduły po drodze importują go przy wczytaniu, a prawdziwy woła okno, którego
 * tu nie ma. `Channel` musi umieć powstać, bo krawędź startu zakłada go w konstruktorze. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => new Promise(() => undefined)),
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const Run = (await import('./index')).default;
const { rememberWhatTheLeadCanDo } = await import('./lead');
const { whereItGoes } = await import('./entry/entry');

/** Zdanie pod polem, wyjęte z markupu tym samym wyrażeniem, co w `./entry-row.test.tsx`. */
function hint(): string {
  const markup = renderToStaticMarkup(<Run />);
  return /data-entry-hint[^>]*>([\s\S]*?)<\/p>/.exec(markup)?.[1] ?? '';
}

/** Lider z pełną listą: `Bash`, `Edit` i `Write` w folderze, w którym człowiek pracuje. */
const FULL = { changesFiles: true, runsCommands: true, heldToTheFolder: true };

/** Ten sam lider z listą zawężoną do czytania (`Read`, `Grep`, `Glob`). */
const READING = { changesFiles: false, runsCommands: false, heldToTheFolder: true };

afterEach(() => {
  /* Odpowiedź granicy jest stanem MODUŁU i przeżywa przypadek — dokładnie dlatego, że przeżywa
   * odmontowanie ekranu w aplikacji. Zostawiona tu opisywałaby lidera z poprzedniego przypadku. */
  rememberWhatTheLeadCanDo(null);
});

describe('the row under the field says what the lead can really do', () => {
  it('says the lead can change files and run commands when its tool list carries Bash', () => {
    rememberWhatTheLeadCanDo(FULL);
    const said = hint();

    expect(
      said,
      'the work screen renders no sentence under the field at all, so everything below would be ' +
        'a statement about an empty string.',
    ).not.toBe('');
    expect(
      said.toLowerCase(),
      'this lead has Bash in the list that becomes its argv, and the row does not say it can run ' +
        'commands. That is the whole of L-5: the screen promised a conversation and the person ' +
        'got an agent that ran the test suite. It said: ' +
        JSON.stringify(said),
    ).toContain('run commands');
    expect(
      said.toLowerCase(),
      'and it has Edit and Write, so the row has to say the files can change. Naming one of the ' +
        'two powers and hiding the other is the same wrong promise, one half smaller.',
    ).toContain('change files');
    expect(
      said,
      'the old sentence says only a command starts work, which is exactly what stopped being ' +
        'true for this lead. Leaving it standing next to the new one would leave the person ' +
        'reading two answers to one question (invariant 13). It said: ' +
        JSON.stringify(said),
    ).not.toContain('only /run starts work');
    expect(
      said,
      'and it still has to name what starts a workflow, or a person who wants the work done is ' +
        'left without the next move (DESIGN §8).',
    ).toContain('/run');
  });

  it('leaves the old sentence exactly as it was for a lead that can only read', () => {
    rememberWhatTheLeadCanDo(READING);
    const said = hint();

    expect(
      said,
      'a lead narrowed to Read, Grep and Glob changes nothing and runs nothing, so the sentence ' +
        'this repo shipped for two weeks is the true one — and it is the one that has to stand ' +
        'here, word for word.',
    ).toContain('only /run starts work');
    expect(
      said,
      'and it has to be that sentence WHOLE, not a rewrite that happens to carry those words. ' +
        'Rewriting it now would be a change nobody asked for, and it would make the sentence ' +
        'above look earned when it is not.',
    ).toBe(whereItGoes([]));
  });

  it('changes the sentence on the answer alone, with nothing else on the screen moving', () => {
    rememberWhatTheLeadCanDo(READING);
    const reading = hint();
    rememberWhatTheLeadCanDo(FULL);
    const full = hint();

    expect(
      reading,
      'the two leads got the same sentence, so the row is not reading the answer at all and both ' +
        'cases above pass on one constant. This is the control the whole file stands on: what ' +
        'the row says has to depend on what the lead can do, and on nothing else. It said: ' +
        JSON.stringify(full),
    ).not.toBe(full);
  });

  it('drops the folder clause for a lead that is not held to the folder', () => {
    rememberWhatTheLeadCanDo({ ...FULL, heldToTheFolder: false });
    const anywhere = hint();
    rememberWhatTheLeadCanDo(FULL);
    const here = hint();

    expect(
      here.toLowerCase(),
      'a lead held to the folder has to say so: "changes files" without the boundary reads as a ' +
        'promise smaller than the truth for one dial and larger for the other.',
    ).toContain('in this folder');
    expect(
      anywhere.toLowerCase(),
      'and a lead that is NOT held to it must not say it is. That sentence is the same class of ' +
        'untruth this task removes, pointed the other way: the person reads a boundary that the ' +
        'argv does not carry. It said: ' +
        JSON.stringify(anywhere),
    ).not.toContain('in this folder');
  });
});
