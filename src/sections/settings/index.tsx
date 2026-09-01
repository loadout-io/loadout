/* Ekran Settings: co Loadout robi domyślnie, kiedy człowiek nie powiedział inaczej.
 *
 * DZIŚ JEDEN WYBÓR, I TO JEST CAŁA ZAWARTOŚĆ TEJ SEKCJI. Kto prowadzi rozmowę, był do
 * 2026-08-29 decyzją podejmowaną PRZY KAŻDYM BIEGU: wskazanie żyło w oknie (`run/lead.ts`),
 * zaczynało się puste po każdym uruchomieniu i człowiek wybierał tę samą osobę od nowa. To ta
 * sama pomyłka, którą przy folderze pracy naprawił workspace.
 *
 * WYBÓR MIESZKA W `src/state/settings.ts`, NIE TUTAJ (niezmiennik 13). Ten ekran go pokazuje
 * i zmienia; Run pokazuje ten sam fakt i też go nie kopiuje. Stan zamknięty w `useState` tego
 * komponentu ginąłby przy każdym przejściu do Run, bo powłoka montuje dokładnie jedną sekcję
 * (`src/App.tsx`).
 *
 * DYSK PIERWSZY. `chooseDefaultLead` zmienia wartość dopiero po powrocie z zapisu i oddaje
 * zdanie odmowy albo `null` — kontrolka pokazująca nowy wybór przed potwierdzeniem z dysku
 * kłamie dokładnie tam, gdzie kłamstwo najdrożej kosztuje: po restarcie wybór wraca stary.
 *
 * BIBLIOTEKA AGENTÓW CZYTANA TYM SAMYM ADAPTEREM, którego używa sekcja Agents i pasek Run, więc
 * nie powstaje druga odpowiedź na pytanie „kogo mam zapisanego". Magazyn tamtej sekcji jest
 * FABRYKĄ, a jego jedyna instancja jest prywatna w `sections/agents/index.tsx` — sięgnięcie po
 * nią znaczyłoby zbudowanie drugiej.
 */
import type { ReactElement } from 'react';
import { useEffect, useState, useSyncExternalStore } from 'react';

import { why } from '../../ipc/why';
import {
  chooseDefaultLead,
  defaultLead,
  loadSettings,
  subscribeToDefaultLead,
} from '../../state/settings';
import { list as savedAgents } from '../agents/io';

/**
 * Nazwa kontrolki wyboru.
 *
 * Stała, a nie napis wpisany w JSX, i to z tego samego powodu, dla którego `LEAD_LABEL` mieszka
 * w `sections/run/lead.ts`: kryterium ma ją CZYTAĆ, nie przepisywać. Napis wpisany z palca po
 * obu stronach jest zielony także wtedy, gdy kontrolka i test mówią o dwóch różnych rzeczach.
 *
 * Słowo jest z tabeli DESIGN §8: `orchestrator` jest na liście żargonu, a `lead agent` jest jego
 * zamiennikiem (niezmiennik 14).
 */
export const DEFAULT_LEAD_LABEL = 'Default lead agent';

/** Klasa domu dla pola — ta sama, którą bierze pasek Run i pięć pozostałych sekcji. */
const FIELD = 'field';

/** Zapisany agent, w tym, czego ten ekran od niego potrzebuje: wskazanie i widoczna nazwa. */
interface Lead {
  readonly id: string;
  readonly name: string;
}

export default function SettingsScreen(): ReactElement {
  const chosen = useSyncExternalStore(subscribeToDefaultLead, defaultLead, defaultLead);
  const [leads, setLeads] = useState<readonly Lead[]>([]);
  /** Zdanie, którym odmówił dysk — słowo w słowo od Rusta. `null`, kiedy nie odmówił. */
  const [said, setSaid] = useState<string | null>(null);

  /* Biblioteka czytana przy wejściu na sekcję. Pliki są prawdą, a ekran jest ich widokiem —
   * lista trzymana w pamięci między wejściami pokazywałaby agenta skasowanego obok. */
  useEffect(() => {
    let alive = true;
    savedAgents()
      .then((agents) => {
        if (!alive) return;
        setLeads(agents.map((agent) => ({ id: agent.id, name: agent.name })));
      })
      .catch((error: unknown) => {
        if (!alive) return;
        setSaid(why(error, 'Loadout could not read the agents you have saved.'));
      });
    /* Odczyt wyboru jest idempotentny (`state/settings.ts`), więc wejście tu po tym, jak pasek
     * Run już zapytał, nie pyta drugi raz i nie ma jak skasować świeżego wyboru. */
    void loadSettings().then((refusal) => {
      if (!alive || refusal === null) return;
      setSaid(refusal);
    });
    return () => {
      alive = false;
    };
  }, []);

  /* Wskazanie, którego nie ma na wczytanej liście, nie zniknie po cichu: zostaje w pliku,
   * a kontrolka pokazuje wtedy zaproszenie zamiast pustego okienka. Agent skasowany w Agents
   * jest dokładnie tym przypadkiem i nie jest awarią. */
  const onTheList = leads.some((one) => one.id === chosen);

  async function pick(id: string): Promise<void> {
    setSaid(await chooseDefaultLead(id));
  }

  return (
    <section data-settings-screen className="flex h-full flex-col">
      <header className="flex h-13 items-center border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Settings</h1>
      </header>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {said === null ? null : <p className="mb-3 max-w-160 text-body text-attend">{said}</p>}

        {leads.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <span className="flex size-8 items-center justify-center rounded-md border border-dashed border-line-strong text-muted">
              ◇
            </span>
            <p data-empty className="text-body text-ink">
              No agents saved yet.
            </p>
            <p className="max-w-120 text-center text-body text-muted">
              Add one in Agents, then say here who should lead every run.
            </p>
          </div>
        ) : (
          <div className="max-w-160 rounded-md border border-line bg-panel p-4">
            <label className="block text-ui text-ink" htmlFor="default-lead">
              {DEFAULT_LEAD_LABEL}
            </label>
            {/* JEDNO ZDANIE POD KONTROLKĄ, bo wybór bez granicy jest obietnicą: ten wskazany
                agent prowadzi rozmowę, dopóki człowiek nie wskaże innego na pasku Run — i to
                wskazanie z paska NIE przepisuje tego wyboru. */}
            <p className="mt-1 max-w-120 text-body text-muted">
              This agent leads every run until you pick someone else in the run strip.
            </p>
            <select
              id="default-lead"
              aria-label={DEFAULT_LEAD_LABEL}
              className={FIELD + ' mt-3'}
              value={onTheList ? chosen : ''}
              onChange={(event) => {
                void pick(event.target.value);
              }}
            >
              {onTheList ? null : <option value="">Pick a lead agent</option>}
              {leads.map((one) => (
                <option key={one.id} value={one.id}>
                  {one.name}
                </option>
              ))}
            </select>
          </div>
        )}
      </div>
    </section>
  );
}
