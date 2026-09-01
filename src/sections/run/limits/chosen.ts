/* „Ile agentów naraz" — jeden fakt, jeden dom (niezmiennik 13).
 *
 * DLACZEGO NIE `useState` W KONTROLCE STARTU, GDZIE TO STAŁO DO 2026-08-18. Ta liczba ma dwóch
 * czytelników i każdy zadaje inne pytanie. Kontrolka pyta „co człowiek wybrał", bo wysyła to
 * do `run_workflow`. Pasek kart pyta „ile miejsc ma pula", bo bez sufitu zdanie „2 in use"
 * nigdy nie zdradza, ile ich w ogóle jest (ARCHITECTURE §6a) — a `src/sections/run/index.tsx`
 * podawał tam do dziś ZERO wpisane na sztywno. Stan zamknięty w komponencie zmusza więc drugiego
 * czytelnika do zgadywania, a zgadnięta liczba na ekranie jest gorsza niż jej brak.
 *
 * PULA JEST JEDNA NA CAŁĄ APLIKACJĘ (niezmiennik 11), więc i to pole jest jedno — tak samo mówi
 * akapit „czego tu nie ma" w `src/state/workspaces.ts`. Stan modułu, nie stan komponentu, bo
 * wyjście do Agentów odmontowuje ekran, a wybór człowieka nie ma prawa wtedy wrócić do trójki.
 *
 * DLACZEGO NIE ZUSTAND. Jedna liczba, bez selektorów i bez pochodnych — magazyn dałby tu
 * warstwę, której nikt nie czyta. Kształt jest dokładnie ten, którego chce
 * `useSyncExternalStore`, i ani pola więcej; ten sam zapis stoi w `../requested.ts`.
 */
import { defaultBudgetUsd, subscribeToDefaultBudget } from '../../../state/settings';
import { DEFAULT_AT_ONCE } from './at-once';

let chosen = DEFAULT_AT_ONCE;
const listeners = new Set<() => void>();

/** Ile agentów naraz ma biec — wybór człowieka, albo domyślne trzy, dopóki nie wybierał. */
export function atOnce(): number {
  return chosen;
}

/** Zapisuje wybór. Sufit i podłogę pilnuje sama kontrolka (`MIN_AT_ONCE`, `MAX_AT_ONCE`). */
export function setAtOnce(howMany: number): void {
  if (howMany === chosen) return;
  chosen = howMany;
  for (const listener of listeners) listener();
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToAtOnce(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/* SUFIT WYDATKU JEDNEGO BIEGU — jeden fakt, jeden dom (niezmiennik 13), dokładnie jak liczba
 * wyżej. Dwóch czytelników: kontrolka („co człowiek wpisał") i pasek biegu („z ilu"), więc
 * `useState` w kontrolce zmuszałby paska do zgadywania.
 *
 * 2026-08-29 — TRZY STANY, NIE DWA, I TO JEST CAŁE T-208. Do tego dnia stały tu dwa: liczba
 * albo `null`, przy czym `null` był WARTOŚCIĄ POCZĄTKOWĄ i znaczył „bez limitu". Skutek: bieg,
 * przy którym nikt nie pomyślał o pieniądzach, leciał bez sufitu — a „nikt nie pomyślał" jest
 * stanem domyślnym, nie wyjątkiem. Zmierzone koszty prawdziwych biegów właściciela z fazy 8:
 * od $11 do $67,78, a jeden bieg przerwał LIMIT KONTA, nie aplikacja.
 *
 *   `undefined`  nikt nic nie powiedział → bierzemy sufit z Settings (`state/settings.ts`,
 *                trwale `~/.loadout/settings.json`). To jest dziś stan początkowy.
 *   `null`       człowiek ZDJĄŁ sufit z tego biegu. Wolno mu, ale nie po cichu: ekran mówi
 *                wtedy zdanie `NO_CEILING_SAID` (`./budget.tsx`).
 *   liczba       ten jeden bieg ma własną kwotę, inną niż domyślna.
 *
 * Nadpisanie nie przepisuje pliku, tak samo jak wskazanie lidera na pasku (`../lead.ts`):
 * „ile wolno wydać na TEN bieg" i „ile wolno wydać na każdy, który nie powiedział inaczej" to
 * dwa różne fakty i mają dwa różne domy.
 *
 * 2026-08-29, DRUGA POPRAWKA — NADPISANIE JEST NA JEDEN BIEG I ZNIKA RAZEM Z NIM. Pierwsza
 * wersja zostawiała je na zawsze, więc jedno zdjęcie sufitu obowiązywało KAŻDY następny Start:
 * bieg po nim znowu leciał bez ograniczenia i znowu nikt tego nie zamawiał — czyli dokładnie ta
 * wada, którą zadanie miało skasować, tylko przesunięta o jeden bieg dalej. Sufit zdejmuje się
 * więc dla TEGO biegu, a nie dla wszystkich, które przyjdą po nim.
 */

/** Nadpisanie na NAJBLIŻSZY ręczny Start. Zdejmuje je [`takeTheBudget`], kiedy ten Start padnie. */
let budget: number | null | undefined;

/**
 * Sufit, z którym NAPRAWDĘ ruszył ostatni Start — czyli ten, o którym mówi pasek biegu.
 *
 * Osobne pole, bo to inne pytanie niż „ile dostanie następny bieg", a odpowiedzi rozjeżdżają się
 * w chwili, w której [`takeTheBudget`] zdejmuje nadpisanie. Bez niego chip „$3.41 of $20" nad
 * liniami biegu ograniczonego dwudziestką pokazywałby domyślną kwotę z Settings, czyli liczbę,
 * której ten bieg nigdy nie dostał. `undefined`, dopóki w tym oknie nic nie ruszyło.
 */
let carried: number | null | undefined;
const budgetListeners = new Set<() => void>();

/**
 * Sufit, z którym pojedzie NASTĘPNY ręczny bieg, albo `null`, kiedy człowiek go JAWNIE zdjął.
 *
 * Odpowiedź składa się z dwóch źródeł i pierwszeństwo jest ustalone: nadpisanie z paska bije
 * wybór z Settings, bo jest młodsze i dotyczy tego jednego biegu. Odwrotna kolejność znaczyłaby
 * kontrolkę, która u kogoś z zapisanym sufitem nie robi nic (niezmiennik 16).
 */
export function budgetUsd(): number | null {
  return budget === undefined ? defaultBudgetUsd() : budget;
}

/**
 * Sufit biegu, który idzie — albo tego, który zszedł ostatni. To o NIM mówi pasek.
 *
 * Dopóki w tym oknie nic nie ruszyło, odpowiada tym, co pojedzie z następnym Startem: pasek nie
 * ma wtedy innego biegu do opisania, a milczenie czytałoby się jak bieg bez sufitu.
 */
export function budgetOfTheRun(): number | null {
  return carried === undefined ? budgetUsd() : carried;
}

/**
 * Sufit dla biegu, który WŁAŚNIE rusza — i zdejmuje nadpisanie, bo dotyczyło jednego biegu.
 *
 * Wołane z `../io.ts`, w ciele `start`/`ask`, a nie z wartości domyślnej argumentu: wartości
 * domyślne liczą się PRZED zapadką `going`, więc drugie kliknięcie w tym samym tyknięciu pętli
 * zdarzeń — to, które Rusta nigdy nie zobaczy — zjadałoby kwotę wpisaną na bieg, który dopiero
 * co ruszył.
 */
export function takeTheBudget(): number | null {
  const carrying = budgetUsd();
  carried = carrying;
  budget = undefined;
  for (const listener of budgetListeners) listener();
  return carrying;
}

/**
 * Zapisuje sufit następnego biegu. `null` znaczy „człowiek go zdjął", `undefined` — „nikt nic nie
 * powiedział". Kontrolka pilnuje, że liczba jest kwotą (`./budget.tsx`).
 */
export function setBudgetUsd(dollars: number | null | undefined): void {
  if (dollars === budget) return;
  budget = dollars;
  for (const listener of budgetListeners) listener();
}

/**
 * Prenumerata w kształcie, którego chce `useSyncExternalStore`.
 *
 * Słucha OBU magazynów, bo [`budgetUsd`] składa odpowiedź z obu — ten sam ruch i ten sam powód,
 * co przy `subscribeToLead` (`../lead.ts`). Prenumerata pilnująca wyłącznie nadpisania z paska
 * pokazywałaby starą kwotę po zapisie w Settings aż do następnego renderu z innego powodu.
 */
export function subscribeToBudget(listener: () => void): () => void {
  budgetListeners.add(listener);
  const stopWatchingTheDefault = subscribeToDefaultBudget(listener);
  return () => {
    budgetListeners.delete(listener);
    stopWatchingTheDefault();
  };
}
