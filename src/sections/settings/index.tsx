/* Ekran Settings: co Loadout robi domyślnie, kiedy człowiek nie powiedział inaczej.
 *
 * DZIŚ DWA WYBORY, I TO JEST CAŁA ZAWARTOŚĆ TEJ SEKCJI. Kto prowadzi rozmowę, był do
 * 2026-08-29 decyzją podejmowaną PRZY KAŻDYM BIEGU: wskazanie żyło w oknie (`run/lead.ts`),
 * zaczynało się puste po każdym uruchomieniu i człowiek wybierał tę samą osobę od nowa. To ta
 * sama pomyłka, którą przy folderze pracy naprawił workspace.
 *
 * SUFIT WYDATKU JEST DRUGIM WYBOREM TEGO SAMEGO KSZTAŁTU i wylądował tu tego samego dnia
 * (T-208). Do tego dnia bieg, przy którym nikt nie pomyślał o pieniądzach, leciał bez żadnego
 * ograniczenia i nic tego nie mówiło — a „nikt nie pomyślał" jest stanem domyślnym, nie
 * wyjątkiem. Zdjąć sufit z JEDNEGO biegu nadal wolno, na pasku Run, i wtedy ekran mówi to na
 * głos (`run/limits/budget.tsx`, `NO_CEILING_SAID`).
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
  chooseDefaultBudgetUsd,
  chooseDefaultLead,
  defaultBudgetUsd,
  defaultLead,
  loadSettings,
  subscribeToDefaultBudget,
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

/**
 * Nazwa kontrolki sufitu wydatku — z tego samego powodu stała, co [`DEFAULT_LEAD_LABEL`].
 *
 * Znak dolara jest częścią nazwy, a nie ozdobą: pole przyjmuje samą liczbę, więc bez waluty
 * w nazwie „75" nie mówi, czy chodzi o dolary, minuty, czy o liczbę kroków.
 */
export const DEFAULT_BUDGET_LABEL = 'Default spend limit $';

/** Klasa domu dla pola — ta sama, którą bierze pasek Run i pięć pozostałych sekcji. */
const FIELD = 'field';

/** Ta sama podłoga, co po obu stronach granicy: kwota poniżej centa nie jest sufitem. */
const SMALLEST = 0.01;

/** Zapisany agent, w tym, czego ten ekran od niego potrzebuje: wskazanie i widoczna nazwa. */
interface Lead {
  readonly id: string;
  readonly name: string;
}

export default function SettingsScreen(): ReactElement {
  const chosen = useSyncExternalStore(subscribeToDefaultLead, defaultLead, defaultLead);
  const ceiling = useSyncExternalStore(
    subscribeToDefaultBudget,
    defaultBudgetUsd,
    defaultBudgetUsd,
  );
  const [leads, setLeads] = useState<readonly Lead[]>([]);
  /** Zdanie, którym odmówił dysk — słowo w słowo od Rusta. `null`, kiedy nie odmówił. */
  const [said, setSaid] = useState<string | null>(null);
  /**
   * Co człowiek ma w tej chwili wpisane w polu kwoty. `null` znaczy „nie pisze" — pokazujemy
   * wtedy to, co pamięta plik.
   *
   * SZKIC NIE JEST WYBOREM, więc „dysk pierwszy" zostaje w mocy: zapisana kwota zmienia się
   * dopiero po powrocie z `save_settings`, a to pole pokazuje po prostu klawisze, które padły.
   * Bez szkicu tej kontrolki NIE DA SIĘ obsłużyć: zapis przy każdym znaku odrzuca „0" w drodze
   * do „0.5" i zabiera człowiekowi to, co właśnie napisał — a wybór wysyłany w połowie liczby
   * jest zapisem, o który nikt nie prosił.
   */
  const [typing, setTyping] = useState<string | null>(null);

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

  /**
   * Oddaje wpisaną kwotę dyskowi — po odejściu z pola albo po Enterze, czyli wtedy, kiedy
   * człowiek skończył ją pisać.
   *
   * KWOTY NIE POPRAWIAMY PO CICHU. Puste pole to `Number('')`, czyli zero, a zero jest biegiem,
   * który nie ma prawa ruszyć — więc jedzie do Rusta i wraca zdaniem, które człowiek czyta w tym
   * samym akapicie, co każdą inną odmowę tego ekranu. Liczba podstawiona tutaj wyglądałaby na
   * ekranie tak, jakby to on ją wpisał (`state/settings.ts`, `chooseDefaultBudgetUsd`).
   *
   * Szkic ZOSTAJE po odmowie i znika po potwierdzeniu: odrzucona kwota, która sama się kasuje,
   * zabiera człowiekowi jedyną rzecz, którą ma poprawić.
   */
  async function spendAtMost(): Promise<void> {
    if (typing === null) return;
    const refusal = await chooseDefaultBudgetUsd(Number(typing));
    setSaid(refusal);
    if (refusal === null) setTyping(null);
  }

  return (
    <section data-settings-screen className="flex h-full flex-col">
      <header className="flex h-13 items-center border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Settings</h1>
      </header>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {said === null ? null : <p className="mb-3 max-w-160 text-body text-attend">{said}</p>}

        {/* SUFIT STOI NAD LIDEREM I RENDERUJE SIĘ ZAWSZE, także na maszynie bez ani jednego
            zapisanego agenta. To nie jest dodatek do wyboru lidera: bieg, którego nikt nie
            ograniczył, kosztuje pieniądze niezależnie od tego, czy jest kogo wskazać na
            prowadzącego, a do 2026-08-29 był to stan DOMYŚLNY. Zmierzone koszty prawdziwych
            biegów właściciela z fazy 8: od $11 do $67,78, a jeden bieg przerwał limit konta,
            nie aplikacja. */}
        <div className="mb-4 max-w-160 rounded-md border border-line bg-panel p-4">
          <label className="block text-ui text-ink" htmlFor="default-budget-usd">
            {DEFAULT_BUDGET_LABEL}
          </label>
          {/* JEDNO ZDANIE POD KONTROLKĄ, bo liczba bez granicy jest zagadką: mówi, KTÓRE biegi
              ta kwota obejmuje i gdzie się ją nadpisuje na jeden raz. */}
          <p className="mt-1 max-w-120 text-body text-muted">
            Every run stops at this much unless you type another amount in the run strip.
          </p>
          {/* Kwota jest wartością maszynową, więc mono — reguła semantyczna z DESIGN §4.
              `min` i `step` są atrybutami kontrolki, nie zdaniem obok niej: napis „at least a
              cent" pod polem niczego nie zatrzymuje.

              ZAPIS PO ODEJŚCIU Z POLA ALBO PO ENTERZE, nie po każdym znaku — powód w całości
              stoi przy `spendAtMost`. Klawiatura i mysz dają tę samą drogę, bo człowiek kończy
              pisać liczbę raz jednym, raz drugim. */}
          <input
            id="default-budget-usd"
            aria-label={DEFAULT_BUDGET_LABEL}
            type="number"
            inputMode="decimal"
            min={SMALLEST}
            step={SMALLEST}
            className={FIELD + ' mt-3 w-32 text-right font-mono'}
            value={typing ?? String(ceiling)}
            onChange={(event) => {
              setTyping(event.target.value);
            }}
            onBlur={() => {
              void spendAtMost();
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter') void spendAtMost();
            }}
          />
        </div>

        {leads.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-3 py-10">
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
