/* JEDYNY dom tego, co Loadout robi domyślnie, po stronie okna (niezmiennik 13). Trzy fakty:
 * kto prowadzi rozmowę, ile wolno wydać na jeden bieg i czy boczne menu stoi zwinięte.
 *
 * CZYM TO JEST, A CZYM NIE JEST. „Domyślny lider" to jeden globalny wybór, który Run bierze,
 * kiedy człowiek nie powiedział inaczej w pasku. Run go POKAZUJE i nie trzyma drugiej kopii:
 * `src/sections/run/lead.ts` pyta stąd, kiedy jego własne wskazanie okna jest puste. Dwie kopie
 * jednego faktu rozjeżdżają się przy pierwszym zapisie, a rozjazd tego akurat faktu wygląda na
 * ekranie jak lider, który się myli — bo agent odpowiadający innym modelem niż wskazany wygląda
 * dokładnie tak samo.
 *
 * DYSK PIERWSZY, tak jak w `./workspaces.ts`. `chooseDefaultLead` zmienia stan DOPIERO po
 * powrocie z `save_settings` i oddaje zdanie odmowy albo `null`. Odwrotna kolejność to defekt,
 * który już raz w tym repo wystąpił: agent zniknięty z listy przy NIEUDANYM usunięciu wracał po
 * restarcie, bo okno uwierzyło sobie, a nie plikowi.
 *
 * MODUŁ, NIE ZUSTAND, i to jest ten sam wybór, co przy `sections/run/lead.ts` oraz
 * `sections/run/limits/chosen.ts`: kształt, którego chce `useSyncExternalStore`, czytelny także
 * spoza drzewa Reacta. Powłoka montuje dokładnie jedną sekcję (`src/App.tsx`), więc wyjście
 * z Settings do Run i z powrotem odmontowuje oba ekrany — wybór ma przeżyć to bez mrugnięcia.
 *
 * ODCZYT JEST IDEMPOTENTNY. `loadSettings()` pyta dysk RAZ na okno i oddaje tę samą obietnicę
 * każdemu następnemu wołającemu. Dwa ekrany wołają ją przy montowaniu, a wersja pytająca za
 * każdym razem kasowałaby świeży wybór odpowiedzią na żądanie wysłane przed nim.
 */
import { why } from '../ipc/why';
import type { Settings } from './settings-io';
import { readSettings, saveSettings } from './settings-io';

let chosen = '';
const listeners = new Set<() => void>();

/**
 * Ile wolno wydać na bieg, dopóki dysk nie odpowiedział.
 *
 * 2026-08-29 — DRUGI LITERAŁ TEJ LICZBY W REPO, ŚWIADOMY I OPISANY. Pierwszy stoi w Ruście
 * (`commands::settings::SHIPPED_CEILING_USD`) i to on jest wyborem, który Loadout WYSYŁA
 * w świat; ten odpowiada na inne pytanie — co obowiązuje w oknie, zanim wróci pierwsze
 * `read_settings`. Bez niego istniałaby chwila, w której `budgetUsd()` oddaje „bez sufitu",
 * a Start w tej chwili jest dokładnie tym cichym biegiem bez ograniczenia, który to zadanie
 * usuwa. Odpowiedź z dysku nadpisuje tę liczbę przy pierwszym powrocie i od tej chwili prawdą
 * jest plik (niezmiennik 4) — rozjazd obu literałów może więc kosztować najwyżej mrugnięcie
 * kontrolki, nigdy biegu bez sufitu.
 */
const CEILING_BEFORE_THE_DISK_ANSWERS = 75;

let ceiling = CEILING_BEFORE_THE_DISK_ANSWERS;
const budgetListeners = new Set<() => void>();

/**
 * Czy boczne menu stoi zwinięte do samych ikon. Trzeci wybór tego pliku, od 2026-08-31.
 *
 * `false`, czyli rozwinięte, dopóki dysk nie odpowie — i to jest ta sama decyzja, co przy
 * suficie wydatku obok: pierwsza chwila życia okna ma pokazywać stan, w którym widać wszystko.
 * Odpowiedź z pliku nadpisuje to przy pierwszym powrocie `read_settings`.
 */
let narrow = false;
const navListeners = new Set<() => void>();

/** Czy boczne menu stoi zwinięte do samych ikon. */
export function navIsCollapsed(): boolean {
  return narrow;
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToNavCollapsed(listener: () => void): () => void {
  navListeners.add(listener);
  return () => {
    navListeners.delete(listener);
  };
}

function rememberNav(collapsed: boolean): void {
  if (collapsed === narrow) return;
  narrow = collapsed;
  for (const listener of navListeners) listener();
}

/**
 * Zwija albo rozwija boczne menu. Oddaje zdanie odmowy dla człowieka albo `null`.
 *
 * OKNO PIERWSZE, PLIK DRUGI — i to jest JEDYNY wybór w tym pliku, który tak działa, więc powód
 * jest tu wypisany, a nie domyślny. Lider i sufit są DYSK-PIERWSZE, bo oba są obietnicą
 * o PRZYSZŁYM biegu: stan okna, który wyprzedził plik, pokazuje wtedy lidera, który nie
 * poprowadzi, i sufit, który nie zatrzyma — a przy suficie pomyłka kosztuje pieniądze. Tryb
 * menu nie jest obietnicą o niczym przyszłym: jest szerokością kolumny, którą człowiek widzi
 * NATYCHMIAST. Czekanie na potwierdzenie z dysku zamieniłoby kliknięcie w kontrolkę, po której
 * przez moment nic się nie dzieje, a nieudany zapis — w kontrolkę martwą (niezmiennik 16).
 *
 * CO KOSZTUJE NIEUDANY ZAPIS, powiedziane wprost, żeby nikt nie musiał tego zgadywać: menu
 * zostaje tam, gdzie człowiek je postawił, i wraca do poprzedniego trybu przy następnym
 * uruchomieniu. Zdanie odmowy wraca stąd wołającemu — to on decyduje, czy ma je gdzie pokazać.
 *
 * NIESIE CAŁY WPIS, jak oba zapisy niżej: plik jest jeden i zapis niosący jedną trzecią
 * skasowałby dwie pozostałe.
 */
export function collapseNav(collapsed: boolean): Promise<string | null> {
  rememberNav(collapsed);
  return saveSettings({ defaultLead: chosen, defaultBudgetUsd: ceiling, navCollapsed: collapsed })
    .then(kept)
    .catch((error: unknown) => why(error, 'Loadout could not remember the side nav mode.'));
}

/** Identyfikator agenta, który prowadzi domyślnie, albo `''`, dopóki nikt nie wybierał. */
export function defaultLead(): string {
  return chosen;
}

/** Ile wolno wydać na jeden bieg, dopóki człowiek nie wpisze innej kwoty na pasku Run. */
export function defaultBudgetUsd(): number {
  return ceiling;
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToDefaultLead(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToDefaultBudget(listener: () => void): () => void {
  budgetListeners.add(listener);
  return () => {
    budgetListeners.delete(listener);
  };
}

function remember(id: string): void {
  if (id === chosen) return;
  chosen = id;
  for (const listener of listeners) listener();
}

function rememberCeiling(dollars: number): void {
  if (dollars === ceiling) return;
  ceiling = dollars;
  for (const listener of budgetListeners) listener();
}

/* Odpowiedź granicy, kiedy nikt jej nie podstawił.
 *
 * Rust zawsze oddaje wpis, więc `null` nie przychodzi z produkcji — przychodzi z atrapy granicy
 * w kryteriach przeglądarkowych (`e2e/harness.ts` odpowiada `null` na komendę, której scena nie
 * wymieniła). Okno nie ma prawa się na tym przewrócić (niezmiennik 5 w duchu), a pusty wybór
 * jest tu uczciwą odpowiedzią: nikt nie wybierał. */
function leadIn(answer: unknown): string {
  if (typeof answer !== 'object' || answer === null) return '';
  const said = (answer as { defaultLead?: unknown }).defaultLead;
  return typeof said === 'string' ? said : '';
}

/* Sufit z odpowiedzi granicy — a kiedy go w niej nie ma, ZOSTAJE TEN, KTÓRY JUŻ MAMY.
 *
 * Nie zero i nie „bez sufitu", i to jest różnica wobec pustego wskazania lidera obok: brak
 * wskazania jest uczciwą odpowiedzią („nikt nie wybierał"), a brak kwoty nie jest odpowiedzią
 * w ogóle — bieg, który wolno rozliczyć na zero, nie ma prawa ruszyć, a bieg bez sufitu jest
 * tym, czego to zadanie zabrania robić po cichu. Klucza nie ma w dwóch przypadkach i oba są
 * normalne: plik zapisany przez wcześniejszą wersję Loadouta (Rust wstawia tam swoją liczbę
 * przy odczycie) oraz atrapa granicy w kryteriach przeglądarkowych, która odpowiada `null` na
 * komendę, której scena nie wymieniła. */
function ceilingIn(answer: unknown): number {
  if (typeof answer !== 'object' || answer === null) return ceiling;
  const said = (answer as { defaultBudgetUsd?: unknown }).defaultBudgetUsd;
  return typeof said === 'number' && Number.isFinite(said) ? said : ceiling;
}

/* Tryb menu z odpowiedzi granicy — a kiedy go w niej nie ma, ZOSTAJE TEN, KTÓRY JUŻ MAMY.
 *
 * Ta sama decyzja, co przy suficie obok, i z tego samego powodu: klucza nie ma w dwóch
 * normalnych przypadkach — plik zapisany przez wcześniejszą wersję Loadouta oraz atrapa granicy
 * w kryteriach przeglądarkowych, która odpowiada `null` na komendę, której scena nie wymieniła.
 * „Nie wiem" nie ma prawa rozwinąć menu, które człowiek przed chwilą zwinął. */
function navIn(answer: unknown): boolean {
  if (typeof answer !== 'object' || answer === null) return narrow;
  const said = (answer as { navCollapsed?: unknown }).navCollapsed;
  return typeof said === 'boolean' ? said : narrow;
}

/** Jedno pytanie do dysku na okno; następni wołający dostają tę samą obietnicę. */
let asked: Promise<string | null> | null = null;

/**
 * Czyta oba wybory z dysku. Oddaje zdanie odmowy dla człowieka albo `null`, kiedy się udało.
 *
 * Wołane przy montowaniu przez ekran Settings i przez pasek Run — bo to Run jest miejscem,
 * w którym te wybory widać, a okno otwarte prosto na Run nigdy nie przechodzi przez Settings.
 */
export function loadSettings(): Promise<string | null> {
  asked ??= readSettings()
    .then((settings) => {
      remember(leadIn(settings));
      rememberCeiling(ceilingIn(settings));
      rememberNav(navIn(settings));
      return null;
    })
    .catch((error: unknown) => why(error, 'Loadout could not read what it does by default.'));
  return asked;
}

/**
 * Zapisuje domyślnego lidera. Oddaje zdanie odmowy dla człowieka albo `null`, kiedy dysk
 * potwierdził.
 *
 * Stan bierzemy z ODPOWIEDZI, nie z argumentu: Rust przycina identyfikator, więc wartość
 * złożona tutaj z tego, co wysłaliśmy, byłaby drugim opisem tego samego pliku — i tym, który
 * się rozjedzie.
 *
 * NIESIE CAŁY WPIS, nie samo wskazanie: plik jest jeden i zapis niosący połowę skasowałby
 * drugą połowę (`src-tauri/src/commands/settings.rs`, `save_settings_inner`).
 */
export function chooseDefaultLead(id: string): Promise<string | null> {
  return saveSettings({ defaultLead: id, defaultBudgetUsd: ceiling, navCollapsed: narrow })
    .then(kept)
    .catch((error: unknown) => why(error, 'Loadout could not save who leads by default.'));
}

/**
 * Zapisuje domyślny sufit wydatku. Oddaje zdanie odmowy dla człowieka albo `null`, kiedy dysk
 * potwierdził.
 *
 * TA SAMA DROGA, CO PRZY LIDERZE, i to jest wymóg, nie symetria dla ozdoby: dwa pola jednego
 * pliku zapisywane dwoma sposobami to dwie różne klasy awarii przy przerwanym zapisie — dokładnie
 * to zdanie stoi w nagłówku `commands::settings`.
 *
 * KWOTY NIE POPRAWIAMY PO CICHU. Sufit poniżej centa odrzuca Rust zdaniem, które ekran pokazuje;
 * liczba podstawiona tutaj wyglądałaby na ekranie tak, jakby człowiek ją tak wpisał — a to jest
 * ten jeden wybór, przy którym pomyłka kosztuje pieniądze.
 */
export function chooseDefaultBudgetUsd(dollars: number): Promise<string | null> {
  return saveSettings({ defaultLead: chosen, defaultBudgetUsd: dollars, navCollapsed: narrow })
    .then(kept)
    .catch((error: unknown) =>
      why(error, 'Loadout could not save how much a run may spend by default.'),
    );
}

/** Co robimy z potwierdzonym wpisem — jedno miejsce dla obu zapisów wyżej. */
function kept(settings: Settings): null {
  remember(leadIn(settings));
  rememberCeiling(ceilingIn(settings));
  rememberNav(navIn(settings));
  /* Zapisane wybory są od tej chwili tym, co odda `loadSettings()` następnemu ekranowi:
   * bez tego powrót na Run czytałby dysk odpowiedzią zapamiętaną przed zapisem. */
  asked = Promise.resolve(null);
  return null;
}
