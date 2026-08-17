/* Kontrolka „co uruchomić i ile naraz" — jedyne miejsce w oknie, przez które da się ZACZĄĆ bieg.
 *
 * DLACZEGO TEN PLIK W OGÓLE POWSTAŁ. `src/sections/run/io.ts` eksportuje `start` i `stop` od
 * T-27 i do 2026-08-17 nie miał ani jednego produkcyjnego wołającego: jedynym miejscem w repo,
 * które go importowało, był jego własny test. Silnik był gotowy, komendy zarejestrowane,
 * a okno nie miało czym ich zawołać — czyli aplikacji nie dało się uruchomić z aplikacji.
 * To ta sama rodzina, co zaślepki w sekcjach: mechanizm wylądował, nikt go nie podłączył.
 *
 * DLACZEGO WYBÓR Z LISTY, A NIE WIERSZ POLECEŃ. Makieta (`docs/mockup/index.html`) stawia tu
 * wiersz wejścia z `/plan · /run`, czyli parser komend — a parser, który rozumie jedno słowo,
 * jest gorszy niż lista, bo obiecuje więcej, niż umie. Lista czyta katalog przez ten sam
 * adapter, którego używa sekcja Workflow, więc nie powstaje druga odpowiedź na pytanie
 * „jakie workflow istnieją" (niezmiennik 13).
 *
 * Nazwa pliku jedzie do Rusta, nie cały workflow: to plik na dysku jest prawdą (niezmiennik 4),
 * a kopia treści wysłana z okna byłaby drugim opisem tego samego pliku — i tym, który się
 * rozjedzie, gdy ktoś zapisze workflow między wyborem a kliknięciem.
 */
import type { ReactElement } from 'react';
import { useEffect, useState } from 'react';

import { AtOnce, DEFAULT_AT_ONCE } from './limits/at-once';
import { list } from '../workflows/io';
import { start, stop } from './io';

/** Pozycja listy: nazwa pliku i to, jak workflow nazywa sam siebie. */
interface Choice {
  path: string;
  name: string;
}

const PRIMARY = 'h-9 rounded-sq bg-accent px-4 text-ui text-bg disabled:opacity-40';
const DANGER = 'h-7 rounded-sq border border-fail-edge px-3 text-ui text-fail';
const FIELD = 'h-8 rounded-sq border border-line-strong bg-well px-2 font-mono text-mono text-ink';

export interface StartProps {
  /** Czy bieg już trwa. Wchodzi propsem, bo to magazyn biegu wie, a nie ten plik. */
  running: boolean;
}

export function Start({ running }: StartProps): ReactElement {
  const [choices, setChoices] = useState<readonly Choice[]>([]);
  const [picked, setPicked] = useState('');
  const [atOnce, setAtOnce] = useState(DEFAULT_AT_ONCE);
  const [said, setSaid] = useState<string | null>(null);

  /* Katalog czytamy przy wejściu na sekcję. Pliki są prawdą, a ekran jest ich widokiem —
   * lista trzymana w pamięci między wejściami pokazywałaby workflow skasowany obok. */
  useEffect(() => {
    let alive = true;
    list()
      .then((entries) => {
        if (!alive) return;
        setChoices(entries.map((e) => ({ path: e.path, name: e.workflow.name })));
      })
      .catch((error: unknown) => {
        if (!alive) return;
        /* Odmowa Rusta jest już napisana po ludzku; własne zdanie dokładamy tylko wtedy,
         * gdy jego nie ma — cicha porażka czyta się jak pusty katalog. */
        const why = error instanceof Error ? error.message.trim() : '';
        setSaid(why.length > 0 ? why : 'Loadout could not read the workflows folder.');
      });
    return () => {
      alive = false;
    };
  }, []);

  const chosen = picked === '' ? (choices[0]?.path ?? '') : picked;

  async function go(): Promise<void> {
    if (chosen === '') return;
    setSaid(null);
    try {
      await start(chosen, atOnce);
    } catch (error: unknown) {
      const why = error instanceof Error ? error.message.trim() : '';
      setSaid(why.length > 0 ? why : 'Loadout could not start that run.');
    }
  }

  async function halt(): Promise<void> {
    setSaid(null);
    try {
      await stop();
    } catch (error: unknown) {
      const why = error instanceof Error ? error.message.trim() : '';
      setSaid(why.length > 0 ? why : 'Loadout could not stop the run.');
    }
  }

  return (
    <div className="flex shrink-0 flex-col gap-2 rounded-sq border border-line bg-panel p-3">
      <div className="flex items-center gap-3">
        <select
          aria-label="Workflow to run"
          className={FIELD}
          value={chosen}
          disabled={running || choices.length === 0}
          onChange={(event) => {
            setPicked(event.target.value);
          }}
        >
          {choices.length === 0 ? (
            <option value="">No workflows saved yet</option>
          ) : (
            choices.map((choice) => (
              <option key={choice.path} value={choice.path}>
                {choice.name}
              </option>
            ))
          )}
        </select>

        {running ? (
          <button type="button" className={DANGER} onClick={() => void halt()}>
            Stop
          </button>
        ) : (
          <button
            type="button"
            className={PRIMARY}
            disabled={chosen === ''}
            onClick={() => void go()}
          >
            Start
          </button>
        )}
      </div>

      {/* Limit siedzi obok Startu, a nie w ustawieniach: to decyzja podejmowana przy każdym
       * biegu, bo zależy od tego, co jeszcze chodzi na tej maszynie. */}
      <AtOnce value={atOnce} onChange={setAtOnce} />

      {said === null ? null : (
        <p data-said className="text-body text-fail">
          {said}
        </p>
      )}
    </div>
  );
}
