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
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';

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
import { BUDGET_HELP } from '../run/limits/budget';

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

/** Ta sama podłoga, co po obu stronach granicy: kwota poniżej centa nie jest sufitem. */
const SMALLEST = 0.01;

/* DWA ZDANIA OPISUJĄCE POLE KWOTY — i pole WSKAZUJE na nie po identyfikatorze.
 *
 * 2026-08-31: `aria-describedby` jest tu jedyną drogą, którą treść tych akapitów dociera do
 * czytnika ekranu jako OPIS TEJ kontrolki. Wpisanie ich w `<label>` nie wchodzi w grę: tekst
 * etykiety staje się NAZWĄ pola, więc kontrolka nazywałaby się całym akapitem (zmierzone
 * 2026-08-28 na siedmiu czerwonych kryteriach e2e). Opis idzie w `aria-describedby`, nazwa
 * zostaje krótka. */
const WHICH_RUNS = 'default-budget-which-runs';
const NOT_COUNTED = 'default-budget-not-counted';

/**
 * Pudełko z kwotą, którą OSTATNIO oddano dyskowi.
 *
 * Na ekranie jest to `useRef`, w kryterium zwykły obiekt — [`saveTheAmountOnce`] nie ma prawa
 * wiedzieć, skąd ono jest. `null` znaczy „nic jeszcze nie poszło albo człowiek pisze od nowa".
 */
export interface LastAmountSent {
  current: string | null;
}

/** Wszystko, czego potrzebuje JEDNO oddanie wpisanej kwoty dyskowi. */
export interface AmountBeingSaved {
  /** Co człowiek ma w tej chwili wpisane. `null` znaczy „nie pisze". */
  readonly typed: string | null;
  /** Kwota, którą już oddano dyskowi — jedyna rzecz, która przeżywa oba zdarzenia pola. */
  readonly lastSent: LastAmountSent;
  /** Zapis, słowo w słowo z `state/settings.ts`. Oddaje zdanie odmowy albo `null`. */
  readonly save: (dollars: number) => Promise<string | null>;
  /** Gdzie ma trafić zdanie, którym odpowiedział dysk. */
  readonly said: (sentence: string | null) => void;
  /** Wołane wtedy i tylko wtedy, kiedy dysk kwotę przyjął: szkic nie ma już czego trzymać. */
  readonly taken: () => void;
}

/**
 * Oddaje dyskowi jedną skończoną kwotę — DOKŁADNIE RAZ.
 *
 * 2026-08-31, ZMIERZONE. Pole kwoty ma dwa zakończenia pisania i oba są prawdziwe: Enter
 * i wyjście z pola. Człowiek, który wciska Enter i dopiero potem klika gdzie indziej, robi
 * jedno i drugie — a poprzednia wersja wysyłała wtedy TĘ SAMĄ kwotę dwa razy. Warunek
 * `typing === null` tego nie łapał, bo React nie podmienia domknięcia w trakcie: `setTyping(null)`
 * z pierwszego wywołania ląduje dopiero po `await`, a drugie zdarzenie pada w tym samym renderze
 * i widzi dalej starą wartość. Skutek: dwa zapisy pliku `~/.loadout/settings.json` na jedną
 * decyzję i dwa zdania odmowy na jedną pomyłkę.
 *
 * ZAPADKĄ JEST WYSŁANA KWOTA, NIE FLAGA „leci". Flaga zamknęłaby wyłącznie okno lotu, a to samo
 * podwojenie powstaje także wtedy, gdy odpowiedź dysku wróci przed przemalowaniem ekranu.
 * Zapadkę zdejmuje dopiero NOWY klawisz w polu (`onChange` niżej), więc ta sama kwota wpisana
 * jeszcze raz po odmowie ma prawo pojechać ponownie.
 *
 * KWOTY NIE POPRAWIAMY PO CICHU. Puste pole to `Number('')`, czyli zero, a zero jest biegiem,
 * który nie ma prawa ruszyć — więc jedzie do Rusta i wraca zdaniem, które człowiek czyta w tym
 * samym akapicie, co każdą inną odmowę tego ekranu. Liczba podstawiona tutaj wyglądałaby na
 * ekranie tak, jakby to on ją wpisał (`state/settings.ts`, `chooseDefaultBudgetUsd`).
 *
 * Szkic ZOSTAJE po odmowie i znika po potwierdzeniu: odrzucona kwota, która sama się kasuje,
 * zabiera człowiekowi jedyną rzecz, którą ma poprawić.
 */
export async function saveTheAmountOnce(one: AmountBeingSaved): Promise<void> {
  if (one.typed === null || one.lastSent.current === one.typed) return;
  one.lastSent.current = one.typed;
  const refusal = await one.save(Number(one.typed));
  one.said(refusal);
  if (refusal === null) one.taken();
}

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
  /**
   * Kwota, którą już oddano dyskowi — jedyna rzecz, która przeżywa oba zakończenia pisania.
   *
   * `useRef`, a nie `useState`, i to jest wymóg: zapadka musi być widoczna dla DRUGIEGO
   * zdarzenia z tego samego renderu, a stan Reacta dociera dopiero do następnego. Cały powód
   * stoi przy [`saveTheAmountOnce`].
   */
  const lastSent = useRef<string | null>(null);

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
   * człowiek skończył ją pisać. Cała polityka „raz i tylko raz" stoi w [`saveTheAmountOnce`].
   */
  async function spendAtMost(): Promise<void> {
    await saveTheAmountOnce({
      typed: typing,
      lastSent,
      save: chooseDefaultBudgetUsd,
      said: setSaid,
      taken: () => {
        setTyping(null);
      },
    });
  }

  return (
    <section data-settings-screen className="flex h-full flex-col">
      <header className="screen-head glass">
        <h1 className="text-title text-ink">Settings</h1>
      </header>

      <div className="screen-body">
        {/* WEJŚCIE, bo to zdanie jest jedyną odpowiedzią na zapis: pole samo się nie rusza,
            a odmowa, która pojawia się skokiem, czyta się jak przeskok widoku (DESIGN §7).
            Jeden region na jedno zdarzenie — sufit z ARCHITECTURE §7 wynosi dwa. */}
        {said === null ? null : (
          <p className="lead enter mb-3 max-w-160" data-tone="attend">
            {said}
          </p>
        )}

        {/* SUFIT STOI NAD LIDEREM I RENDERUJE SIĘ ZAWSZE, także na maszynie bez ani jednego
            zapisanego agenta. To nie jest dodatek do wyboru lidera: bieg, którego nikt nie
            ograniczył, kosztuje pieniądze niezależnie od tego, czy jest kogo wskazać na
            prowadzącego, a do 2026-08-29 był to stan DOMYŚLNY. Zmierzone koszty prawdziwych
            biegów właściciela z fazy 8: od $11 do $67,78, a jeden bieg przerwał limit konta,
            nie aplikacja. */}
        <div className="card mb-4 max-w-160">
          <label className="label block" htmlFor="default-budget-usd">
            {DEFAULT_BUDGET_LABEL}
          </label>
          {/* JEDNO ZDANIE POD KONTROLKĄ, bo liczba bez granicy jest zagadką: mówi, KTÓRE biegi
              ta kwota obejmuje i gdzie się ją nadpisuje na jeden raz. */}
          <p id={WHICH_RUNS} className="lead mt-1 max-w-120">
            Every run stops at this much unless you type another amount in the run strip.
          </p>
          {/* CZEGO TA KWOTA NIE OBEJMUJE — NA EKRANIE, NIE POD KURSOREM.
              2026-08-31: do tego dnia zdanie stało wyłącznie w atrybucie `title` pola „Spend at
              most $" na pasku Run, czyli w dymku, który pojawia się po sekundzie trzymania myszy
              w bezruchu i nie istnieje ani dla klawiatury, ani dla czytnika ekranu, ani na
              dotyku. Sufit wyglądał więc na twardy, a kroki jednego z dwóch dostawców nie
              dokładały do tej kwoty ani centa (niezmiennik 29).
              OSOBNY AKAPIT, nie druga połowa zdania wyżej: tamto mówi, KTÓRE biegi ta kwota
              obejmuje, to mówi, CZEGO nie obejmuje wcale. Sklejone w jedno drugie zdanie czyta
              się jak przypis do pierwszego.
              BEZ TONU `attend`, choć zdanie dotyczy pieniędzy: ten kolor odpowiada na pytanie
              „co czeka na moją uwagę" (DESIGN §3), a napis stojący na ekranie ZAWSZE nie czeka
              na nic. Na tym ekranie `attend` należy do zdania odmowy wyżej i ma tam zostać jedno.
              Napis mieszka w `run/limits/budget.tsx` razem z resztą słów sufitu wydatku, żeby
              kryterium mogło go CZYTAĆ, nie przepisywać (niezmiennik 13). */}
          <p id={NOT_COUNTED} data-not-counted className="lead mt-1 max-w-120">
            {BUDGET_HELP}
          </p>
          {/* Kwota jest wartością maszynową, więc mono — reguła semantyczna z DESIGN §4.
              Krój przyjeżdża TERAZ RAZEM ZE STOPNIEM w klasie `.field`, więc `font-mono`
              dopisane obok zniknęło: deklarowało rodzinę drugi raz.
              `min` i `step` są atrybutami kontrolki, nie zdaniem obok niej: napis „at least a
              cent" pod polem niczego nie zatrzymuje.

              ZAPIS PO ODEJŚCIU Z POLA ALBO PO ENTERZE, nie po każdym znaku — powód w całości
              stoi przy `spendAtMost`. Klawiatura i mysz dają tę samą drogę, bo człowiek kończy
              pisać liczbę raz jednym, raz drugim. */}
          <input
            id="default-budget-usd"
            aria-label={DEFAULT_BUDGET_LABEL}
            aria-describedby={`${WHICH_RUNS} ${NOT_COUNTED}`}
            type="number"
            inputMode="decimal"
            min={SMALLEST}
            step={SMALLEST}
            className="field mt-3 w-32 text-right"
            value={typing ?? String(ceiling)}
            onChange={(event) => {
              /* NOWY KLAWISZ ZDEJMUJE ZAPADKĘ, więc następne zakończenie pisania ma prawo
                 pojechać na dysk — także wtedy, gdy człowiek po odmowie wystukał dokładnie tę
                 samą kwotę jeszcze raz. Bez tej linii zapadka z `saveTheAmountOnce` zamknęłaby
                 pole na jedną kwotę do końca życia ekranu. */
              lastSent.current = null;
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
            <span className="mark">◇</span>
            <p data-empty className="text-ink">
              No agents saved yet.
            </p>
            <p className="lead max-w-120 text-center">
              Add one in Agents, then say here who should lead every run.
            </p>
          </div>
        ) : (
          <div className="card max-w-160">
            <label className="label block" htmlFor="default-lead">
              {DEFAULT_LEAD_LABEL}
            </label>
            {/* JEDNO ZDANIE POD KONTROLKĄ, bo wybór bez granicy jest obietnicą: ten wskazany
                agent prowadzi rozmowę, dopóki człowiek nie wskaże innego na pasku Run — i to
                wskazanie z paska NIE przepisuje tego wyboru. */}
            <p className="lead mt-1 max-w-120">
              This agent leads every run until you pick someone else in the run strip.
            </p>
            <select
              id="default-lead"
              aria-label={DEFAULT_LEAD_LABEL}
              className="field mt-3"
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
