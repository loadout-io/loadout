/* Ekran sekcji Agents: nagłówek, jedna ścieżka dodawania i lista agentów, każdy ze swoim
 * vendorem.
 *
 * CIENKI Z ZAŁOŻENIA. Formularz (`agent-form.tsx`, T-11) jest wylądowany i to on jest tu
 * całym „dodawaniem" — drugiego nie piszemy (niezmiennik 23). Między komponentem a sekcją
 * brakowało trzech rzeczy i tylko one są w tym pliku: nagłówka z licznikiem, jednej kontrolki
 * dodawania i listy.
 *
 * DLACZEGO PRZYCISK ODSŁANIA FORMULARZ, A NIE TWORZY PLIKU OD RAZU. `＋ Create` na liście
 * workflow tworzy plik natychmiast, bo pusty workflow jest poprawny — pusty AGENT nie jest:
 * `AgentForm` budzi `Save` dopiero, gdy nazwa i instrukcje są wypełnione [T4 §8.1], a agent
 * bez instrukcji to sama nazwa. Kontrolka odsłania więc formularz, czyli robi dokładnie to,
 * co obiecuje (niezmiennik 16), i nie zostawia na dysku pliku, którego walidator odrzuci.
 *
 * ZGŁOSZENIE DLA CZŁOWIEKA (zmierzone 2026-08-16). `AgentsState` zna `load`, `duplicate`
 * i `delete`, ale NIE ZNA tworzenia — mennica `newId` i `save` siedzą w `AgentsIo`, którego
 * magazyn nie wystawia. Zapis nowego agenta jedzie tu przez adapter tej sekcji, obok magazynu,
 * i zaraz po nim ekran czyta katalog od nowa. Właściwe miejsce na tę ścieżkę to
 * `createAgentsStore` albo `src/sections/agents/io.ts` — plik, który `src/state/agents.ts`
 * wymienia z nazwy, a którego nikt nie napisał. Oba są poza blokiem OWNS tego zadania
 * (AGENTS.md §7).
 *
 * O migawce serwerowej zustanda i o tym, dlaczego magazyn czyta się tu przez
 * `useSyncExternalStore`, przeczytaj w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement } from 'react';
import { useEffect, useState, useSyncExternalStore } from 'react';
import type { Agent, AgentsIo, Vendor } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import { AgentForm } from './agent-form';
import * as Disk from './io';

/** Magazyn agentów — dokładnie ten, który oddaje `createAgentsStore`. */
export type AgentsStore = ReturnType<typeof createAgentsStore>;

export interface AgentsScreenProps {
  /** Bez propsu ekran bierze swój prawdziwy magazyn, z propsem ten z testu. */
  store?: AgentsStore;
}

/* Adapter dysku sekcji — PRAWDZIWY, od 2026-08-17.
 *
 * Do tego dnia stała tu zaślepka: `list` oddawał pustą tablicę, a `newId`, `save` i `remove`
 * odmawiały zdaniem „Loadout cannot reach the folder that holds agents yet". Jej komentarz
 * mówił, że `src/sections/agents/io.ts` nie istnieje i że warstwę IPC dowozi T-27.
 *
 * T-27 dowiozło. Plik istniał od kilkunastu godzin, eksportował dokładnie te cztery funkcje
 * i NIKT go nie wołał — jedynym miejscem w repo, które go importowało, był test. Sekcja przez
 * ten czas była trwale pusta, a Create odmawiał pod palcem, i nic tego nie zgłaszało: zaślepka
 * odpowiadająca „nic tam nie leży" czyta się dokładnie tak samo jak pusta biblioteka.
 *
 * Kształt modułu jest lustrem `AgentsIo`, więc podstawia się w całości. Adnotacja typu nie jest
 * ozdobą: to ona sprawdza, że moduł NADAL spełnia interfejs magazynu — funkcja usunięta po
 * tamtej stronie granicy przestaje się kompilować tutaj, zamiast odmawiać pod palcem. */
const DISK: AgentsIo = Disk;

/* Prawdziwy magazyn sekcji powstaje RAZ, przy wczytaniu modułu — magazyn budowany w ciele
 * komponentu gubiłby zawartość ekranu przy każdym przemontowaniu. */
const OWN_STORE = createAgentsStore(DISK);

/* Brzmienia vendorów. To jest DRUGA KOPIA dwuwierszowej tabeli `VENDORS` z `agent-form.tsx`
 * i jest długiem, nie rozwiązaniem: tamten plik jej nie eksportuje, a leży poza blokiem OWNS
 * tego zadania (AGENTS.md §7), więc jedna zmiana brzmienia rozjedzie te dwa miejsca i nikt się
 * o tym nie dowie. Naprawa jest jednoliniowa i należy do właściciela tamtego pliku:
 * `export const VENDORS`. Zapisane jako uwaga dla człowieka 2026-08-16 — tak samo, jak
 * `src/App.tsx` zapisał swoją kopię pustego ekranu.
 *
 * Nazwa z drutu (`claude-code`) nie ma prawa dojechać na ekran (niezmiennik 14) i to jest
 * jedyny powód, dla którego ta tabela w ogóle istnieje. */
const VENDOR_SAYS: Readonly<Record<Vendor, string>> = {
  'claude-code': 'Claude Code',
  codex: 'Codex',
};

/* Klasy komponentów z DESIGN §6. Wysokości idą po siatce 4px: 36px = `h-9` (button-primary),
 * 28px = `h-7` (button-quiet). */
const PRIMARY = 'h-9 rounded-sq bg-accent px-4 text-ui text-bg';
const QUIET = 'h-7 rounded-sq border border-line px-3 text-ui text-body';
/* `chip`, wariant neutralny: vendor jest tożsamością agenta, a nie jego stanem — nasycony
 * kolor znaczy w tej aplikacji „twoja kolej" albo „teraz" (DESIGN §3). */
const CHIP = 'h-5 rounded-sq border border-line bg-raised px-2 text-label text-muted';

/**
 * Nowy agent, zanim człowiek cokolwiek w nim wpisze.
 *
 * Wartości domyślne stoją tutaj, bo magazyn nie zna tworzenia — patrz zgłoszenie w nagłówku
 * pliku. Dwie z nich są decyzjami, nie wypełniaczem: `fileAccess` jest najwęższy z trzech,
 * bo prawo do zmieniania plików ma dawać człowiek, a nie wartość domyślna; `id` jest puste,
 * bo identyfikator wybija mennica po stronie Rusta przy zapisie [T4 §5.1], a nie ekran.
 */
function blankAgent(): Agent {
  return {
    schema: 1,
    id: '',
    name: '',
    summary: '',
    color: 'slate',
    instructions: '',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'look-only',
    giveUpAfterMinutes: 10,
    tools: 'everything',
    skills: [],
    connections: [],
    writeResultsTo: '',
  };
}

export default function AgentsScreen({ store = OWN_STORE }: AgentsScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  /* Szkic nowego agenta. `null` znaczy, że żadnego nie ma — jedno miejsce na to pytanie
   * (niezmiennik 13). Stan jest lokalny, bo dotyczy tego, co jest OTWARTE na ekranie,
   * a nie tego, co leży na dysku. */
  const [draft, setDraft] = useState<Agent | null>(null);
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    void store.getState().load();
  }, [store]);

  /* Jedna funkcja na całą sekcję i to jest cały sens niezmiennika 16: przycisk w nagłówku
   * i przycisk w zaproszeniu są dwoma wejściami do JEDNEJ ścieżki. Drugie kliknięcie nie
   * kasuje tego, co człowiek zdążył wpisać. */
  const startDraft = (): void => {
    setDraft((open) => open ?? blankAgent());
    setExpanded(false);
  };

  const save = (agent: Agent): void => {
    /* Nieudany zapis zostawia szkic otwarty i NIE dokłada wiersza do listy: ekran zgadza się
     * wtedy z tym, co naprawdę leży na dysku (niezmiennik 4). Odmowy nie ma dziś gdzie
     * postawić — `AgentsState` nie ma pola na zdanie dla człowieka, a błędy plików należą
     * do T-12. Ta sama decyzja stoi w `list/workflow-list.tsx` przy `create`. */
    void (async () => {
      const id = await DISK.newId();
      await DISK.save({ ...agent, id });
      setDraft(null);
      /* Katalog czytamy od nowa zamiast dokładać wiersz z pamięci: na dysku jest teraz plik,
       * a lista ma pokazywać dysk, nie nasze wyobrażenie o nim. */
      await store.getState().load();
    })();
  };

  const empty = state.agents.length === 0;

  return (
    <section className="flex h-full flex-col">
      <header className="flex h-13 items-center gap-3 border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Agents</h1>

        {/* Licznik i przycisk w nagłówku żyją tylko wtedy, gdy jest co liczyć. Przy zerze to
            samo mówi zaproszenie niżej, a `0 saved` obok `No agents yet.` to ten sam fakt
            w dwóch miejscach (niezmiennik 13) — i druga kontrolka dodawania na ekranie,
            na którym DESIGN §6 przewiduje dokładnie jedną. Ten sam układ ma wylądowana lista
            workflow. */}
        {empty ? null : (
          <>
            <span className="font-mono text-mono text-muted">{`${String(state.agents.length)} saved`}</span>
            <button data-create type="button" className={`ml-auto ${PRIMARY}`} onClick={startDraft}>
              ＋ Create
            </button>
          </>
        )}
      </header>

      <div className="flex min-h-0 flex-1">
        <div className="min-h-0 flex-1 overflow-auto p-4">
          {empty ? (
            <div className="flex h-full flex-col items-center justify-center gap-3">
              <span className="flex size-8 items-center justify-center rounded-sq border border-dashed border-line-strong text-muted">
                ◇
              </span>
              {/* `data-empty` siedzi na elemencie, który niesie SAMO zdanie — nie na ramce
                  z zaproszeniem. Tak samo robi `src/App.tsx` i z tego samego powodu: treścią
                  tak oznaczonego elementu ma być zdanie, a nie „◇ zdanie ＋ Create". */}
              <p data-empty className="text-ink">
                No agents yet.
              </p>
              <p className="text-muted">Add one, and a step in any workflow can be handed to it.</p>
              <button data-create type="button" className={PRIMARY} onClick={startDraft}>
                ＋ Create
              </button>
            </div>
          ) : (
            <ul className="grid grid-cols-2 gap-3">
              {state.agents.map((agent) => (
                <li
                  key={agent.id}
                  data-agent={agent.id}
                  className="flex flex-col gap-2 rounded-sq border border-line bg-panel p-3"
                >
                  <div className="flex items-center gap-2">
                    <h2 className="text-heading text-ink">{agent.name}</h2>
                    {/* Na czym ten agent biegnie. Obaj vendorzy są pierwszej kategorii (D3),
                        więc etykieta stoi przy KAŻDYM agencie, a nie tylko przy tym, który
                        odstaje od domyślnego. */}
                    <span className={CHIP}>{VENDOR_SAYS[agent.runsWith]}</span>
                  </div>
                  <p className="text-body text-muted">{agent.summary}</p>
                </li>
              ))}
            </ul>
          )}
        </div>

        {draft === null ? null : (
          <aside className="min-h-0 w-83 overflow-auto border-l border-line bg-panel p-4">
            <div className="flex items-center gap-2 pb-3">
              <h2 className="text-heading text-ink">New agent</h2>
              <button
                type="button"
                className={`ml-auto ${QUIET}`}
                onClick={() => {
                  setDraft(null);
                }}
              >
                Cancel
              </button>
            </div>

            <AgentForm
              value={draft}
              expanded={expanded}
              onChange={setDraft}
              onToggleMore={() => {
                setExpanded((open) => !open);
              }}
              onSave={() => {
                save(draft);
              }}
            />
          </aside>
        )}
      </div>
    </section>
  );
}
