/* Co historia pokazuje TERAZ — jedno pole na okno, i nic poza nim.
 *
 * DLACZEGO MAGAZYN NA POZIOMIE MODUŁU, a nie `useState` w ekranie pracy. Ten sam powód, co przy
 * `../session/open.ts`: ekran sekcji odmontowuje się, kiedy człowiek wejdzie do Agentów, a to,
 * co otworzył, ma to przeżyć. Druga, ważniejsza połowa powodu jest testowa — to repo nie ma
 * jsdom, więc `onClick` nie odpala się w żadnym kryterium. Handler trzymający stan wewnątrz
 * komponentu byłby kodem, którego żadne kryterium nie umie dotknąć, i to jest dokładnie ta
 * rodzina, z której wzięły się kontrolki bez skutku (niezmiennik 16). Tutaj kryterium woła to,
 * co woła wiersz listy.
 *
 * LISTA ZOSTAJE, KIEDY OTWIERAMY JEDEN BIEG, i to nie jest wygoda: „wróć" ma wrócić do tej samej
 * listy, a nie odpytać dysk drugi raz. Drugi odczyt oddałby inną listę, gdyby w międzyczasie
 * ruszył bieg — czyli człowiek nacisnąłby „wróć" i zobaczył coś innego niż to, z czego wyszedł.
 *
 * CZEGO TU NIE MA: odpowiedzi na pytanie „gdzie pracujemy". Zakres, z którego ta lista przyszła,
 * jest tu ZAPISANY (`folder`), ale nie liczony — liczy go `../history-command.ts` w chwili
 * naciśnięcia, z jedynego magazynu zakresów, jaki jest (niezmiennik 13). Zapisany, bo „wróć"
 * i wybór wiersza muszą pytać o TEN zakres, z którego lista powstała, także wtedy, gdy człowiek
 * przełączył boczne menu, zanim kliknął.
 */
import { why } from '../../../ipc/why';
import type { CouldForget, PastRun, PastRunRow } from '../io';
import {
  forgetRun,
  forgetRunBranches,
  forgetRunsOlderThan,
  forgetWhatTheOldRunsLeft,
  listRuns,
  whatThisFolderCouldForget,
} from '../io';

/**
 * Ile dni ma mieć bieg, żeby kontrolka daty proponowała go zdjąć — dopóki nikt nie zmieni liczby.
 *
 * Trzydzieści, a nie siedem: bieg sprzed miesiąca to bieg, do którego nikt nie wrócił przez
 * miesiąc, a bieg sprzed tygodnia bywa tym, którego wynik ktoś właśnie porównuje.
 */
export const FORGET_AFTER_DAYS = 30;

/** Co widać: nic, lista albo jeden otwarty bieg. */
export interface PastState {
  /** Czy panel historii w ogóle stoi na ekranie. */
  readonly open: boolean;
  /** Zakres, z którego ta lista przyszła. `null` znaczy „katalog, pod którym wstała aplikacja". */
  readonly folder: string | null;
  /** Biegi tego zakresu, od najnowszego. */
  readonly rows: readonly PastRunRow[];
  /** Bieg otwarty do odczytu, albo `null` — wtedy widać listę. */
  readonly opened: PastRun | null;
  /** Co Loadout powiedział o TYM panelu (np. czemu nie dało się otworzyć wiersza). */
  readonly said: string | null;
  /**
   * Co ten folder mógłby zapomnieć — albo `null`, dopóki Rust nie odpowiedział.
   *
   * `null`, a nie zera: „jeszcze nie pytaliśmy" i „nie ma czego zdejmować" to dwa różne stany,
   * a zdanie o zerach postawione nad folderem, którego nikt nie policzył, jest zdaniem o czymś,
   * czego nie sprawdziliśmy (niezmiennik 17).
   */
  readonly could: CouldForget | null;
  /** Ile dni wpisano w kontrolce daty. Podgląd nad nią liczy się DLA TEJ liczby. */
  readonly olderThanDays: number;
}

const CLOSED: PastState = {
  open: false,
  folder: null,
  rows: [],
  opened: null,
  said: null,
  could: null,
  olderThanDays: FORGET_AFTER_DAYS,
};

let now: PastState = CLOSED;

const listeners = new Set<() => void>();

/**
 * Otwiera panel na LIŚCIE biegów tego zakresu.
 *
 * `could` jest tym, co wołający już wie: `/history` nad folderem bez ani jednego biegu musi
 * zapytać o leżaki, ZANIM zdecyduje, czy panel w ogóle otwierać (`../history-command.ts`), więc
 * odpowiedź jedzie tędy zamiast być czytana drugi raz (2026-09, Z-46). `null` znaczy „nie
 * pytałem" i wtedy pytamy tutaj.
 */
export function showHistory(
  folder: string | null,
  rows: readonly PastRunRow[],
  could: CouldForget | null = null,
): void {
  now = { ...CLOSED, open: true, folder, rows, could };
  publish();
  // PYTAMY OD RAZU, bo to jest jedyne miejsce, w którym człowiek te liczby zobaczy, a zdanie
  // dorysowane sekundę później jest zdaniem, które ktoś przeczyta — puste miejsce nie jest
  // (2026-09, Z-46).
  if (could === null) void learnWhatCouldGo();
}

/**
 * Co ten folder mógłby zapomnieć — pytanie zadane, ZANIM panel wstanie.
 *
 * 2026-09 (Z-46) — istnieje dla jednej drogi: `/history` nad folderem, w którym nie ma ani jednego
 * biegu, odpowiadał zdaniem „nothing has run here yet" i panelu nie otwierał. To jest jednak stan,
 * do którego prowadzi „Forget runs older than …": katalogi biegów schodzą, a gałęzie po nich
 * zostają — i wtedy nie ma już ŻADNEJ drogi, którą człowiek mógłby je zobaczyć albo zdjąć.
 *
 * Kształt sprawdza [`countsIn`], więc odpowiedź, która nie niesie liczb, jest tu tym samym, co
 * brak odpowiedzi (niezmiennik 5).
 */
export async function whatCouldGoIn(folder: string | null): Promise<CouldForget | null> {
  try {
    return countsIn(await whatThisFolderCouldForget(folder, FORGET_AFTER_DAYS));
  } catch {
    return null;
  }
}

/** Otwiera JEDEN bieg do odczytu. Lista zostaje pod spodem, żeby „wróć" miało dokąd wrócić. */
export function showPastRun(run: PastRun): void {
  now = { ...now, open: true, opened: run, said: null };
  publish();
}

/** „Wróć" z otwartego biegu do listy, z której się w niego weszło. */
export function backToTheList(): void {
  if (now.opened === null) return;
  now = { ...now, opened: null, said: null };
  publish();
}

/**
 * Zdanie w panelu — o tym panelu, nie o biegu.
 *
 * TUTAJ, A NIE W STRUMIENIU, i to jest ta sama zasada, którą stosuje ekran pracy: odpowiedź na
 * to, co człowiek właśnie kliknął, ma stanąć tam, gdzie klikał. Wiersz historii, którego nie da
 * się otworzyć, jest faktem o tym panelu — w strumieniu, pod modalem, którego nie widać, byłby
 * odpowiedzią schowaną przed pytającym (niezmiennik 29).
 */
export function sayInHistory(said: string): void {
  if (!now.open) return;
  now = { ...now, said };
  publish();
}

/** Zamyka panel. Ekran pracy pod nim jest dokładnie taki, jaki był. */
export function closeHistory(): void {
  if (!now.open) return;
  now = CLOSED;
  publish();
}

/** Co widać. Ta sama migawka dla okna i dla renderu serwerowego. */
export function pastNow(): PastState {
  return now;
}

/** Powiadomienie o zmianie; oddaje funkcję, która je odwołuje. Kształt `useSyncExternalStore`. */
export function subscribeToPast(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Co powiedzieć, kiedy Rust nie dał rady zdjąć gałęzi tego biegu. */
export const COULD_NOT_FORGET = 'Loadout could not take the branches of this run away.';

/**
 * „Forget the branches" — zdejmuje gałęzie, które otwarty bieg zostawił w repozytorium.
 *
 * TUTAJ, A NIE W KOMPONENCIE, i to jest ten sam powód, dla którego tutaj mieszka cały ten
 * magazyn: to repo nie ma jsdom, więc `onClick` nie odpala się w żadnym kryterium. Polityka
 * zamknięta w handlerze byłaby kodem, którego nic nie sądzi — czyli rodziną, z której biorą się
 * kontrolki bez skutku (niezmiennik 16). Kryterium woła dokładnie to, co woła przycisk.
 *
 * ZAKRES Z MAGAZYNU, nie z `activeWorkspace()`: ten bieg przyszedł z konkretnego folderu, więc
 * pytanie o jego gałęzie idzie do tego samego folderu, także wtedy, gdy człowiek przełączył
 * boczne menu, zanim nacisnął.
 *
 * LISTA PUSTOSZEJE DOPIERO PO ODPOWIEDZI. Wyczyszczenie jej od razu pokazywałoby „nie ma już
 * gałęzi" nad repozytorium, w którym wszystkie stoją — a odmowa przychodzi właśnie wtedy, gdy
 * któraś jest w tej chwili otwarta do pracy.
 *
 * Odmowa zostawia listę TAKĄ, JAKA BYŁA. Rust odmawia w całości, więc nie ma stanu pośredniego
 * do pokazania; gdyby git odmówił po drodze, panel zgadza się znowu po ponownym otwarciu biegu,
 * bo prawdą są pliki (niezmiennik 4).
 */
export async function forgetTheBranches(): Promise<void> {
  const run = now.opened;
  if (run === null) return;
  try {
    await forgetRunBranches(now.folder, run.folder);
  } catch (error: unknown) {
    sayInHistory(why(error, COULD_NOT_FORGET));
    return;
  }
  // Ten sam bieg, co przed pytaniem: człowiek mógł w międzyczasie wrócić do listy i otworzyć
  // inny, a wtedy odpowiedź o gałęziach tamtego biegu nie ma prawa przepisać tego, co widać.
  if (now.opened !== run) return;
  now = { ...now, opened: { ...run, branches: [] }, said: null };
  publish();
}

/** Co powiedzieć, kiedy Rust nie dał rady zdjąć tego biegu. */
export const COULD_NOT_FORGET_RUN = 'Loadout could not take this run away.';

/**
 * „Forget this run" — zdejmuje otwarty bieg razem z jego folderem i gałęziami.
 *
 * TUTAJ, A NIE W KOMPONENCIE, z tego samego powodu, co [`forgetTheBranches`] obok: to repo nie ma
 * jsdom, więc `onClick` nie odpala się w żadnym kryterium, a polityka zamknięta w handlerze byłaby
 * kodem, którego nic nie sądzi (niezmiennik 16).
 *
 * WRACAMY NA LISTĘ I ZDEJMUJEMY Z NIEJ WIERSZ, i to jest cała różnica wobec zapominania samych
 * gałęzi. Tam bieg zostaje otwarty, bo dalej istnieje — zniknęły tylko jego gałęzie. Tutaj bieg
 * PRZESTAŁ ISTNIEĆ: panel zostawiony na jego opisie pokazywałby strumienie i przekazania, których
 * nie ma już na dysku, a „wróć" wracałoby do listy z wierszem, którego nie da się otworzyć.
 *
 * Odmowa zostawia WSZYSTKO tak, jak było, bo Rust odmawia w całości: ani folderu, ani gałęzi.
 * Zdanie idzie w `said`, czyli tam, gdzie człowiek nacisnął (niezmiennik 29).
 */
export async function forgetThisRun(confirmedResultFolders?: readonly string[]): Promise<void> {
  const run = now.opened;
  if (run === null) return;
  const folder = now.folder;
  const expected = (run.resultFolders ?? []).map((one) => one.path).sort();
  const confirmed = [...(confirmedResultFolders ?? [])].sort();
  // WF-06: zgoda dotyczy wyświetlonych ścieżek, nie samego przycisku „Forget”. Rust ponownie
  // sprawdza aktualną listę na dysku; ten warunek nie udaje ochrony po stronie serwera.
  if (expected.length !== confirmed.length || expected.some((path, at) => path !== confirmed[at])) {
    sayInHistory(
      'These folders contain saved results. Confirm their exact paths before forgetting this run.',
    );
    return;
  }
  try {
    await forgetRun(folder, run.folder, confirmedResultFolders);
  } catch (error: unknown) {
    sayInHistory(why(error, COULD_NOT_FORGET_RUN));
    return;
  }
  // Ten sam bieg, co przed pytaniem: człowiek mógł w międzyczasie wrócić do listy i otworzyć
  // inny, a wtedy odpowiedź o tamtym biegu nie ma prawa przepisać tego, co widać.
  if (now.opened !== run) return;
  now = {
    ...now,
    rows: now.rows.filter((row) => row.folder !== run.folder),
    opened: null,
    said: null,
  };
  publish();
}

/** Co powiedzieć, kiedy Rust nie dał rady zdjąć tego, co zostało po starych biegach. */
export const COULD_NOT_SWEEP = 'Loadout could not take away what the old runs left behind.';

/** Co powiedzieć, kiedy Rust nie dał rady zapomnieć starych biegów. */
export const COULD_NOT_FORGET_OLD = 'Loadout could not forget the runs older than that.';

/**
 * Pyta Rusta, co ten folder mógłby zapomnieć, i wkłada odpowiedź do panelu.
 *
 * CICHO, KIEDY NIE MA ODPOWIEDZI, i to jest wybór: to jest liczenie w tle, o które nikt nie
 * prosił, a czerwone zdanie nad listą biegów uczyłoby ignorować czerwone zdania. Człowiek widzi
 * wtedy dokładnie to, co widział dotąd — listę bez akapitu o leżakach.
 *
 * KSZTAŁT SPRAWDZAMY, zamiast mu ufać: granica bywa atrapą (`e2e/harness.ts` odpowiada kształtem,
 * nie stanem), a starszy Loadout tej komendy nie zna wcale. Odpowiedź, która nie niesie liczb,
 * jest tu tym samym, co brak odpowiedzi (niezmiennik 5).
 */
export async function learnWhatCouldGo(): Promise<void> {
  if (!now.open) return;
  const folder = now.folder;
  const days = now.olderThanDays;
  let answer: unknown;
  try {
    answer = await whatThisFolderCouldForget(folder, days);
  } catch {
    return;
  }
  // Ten sam panel, co przed pytaniem: człowiek mógł go w międzyczasie zamknąć albo przełączyć
  // zakres, a wtedy odpowiedź o tamtym folderze nie ma prawa przepisać tego, co widać.
  if (!now.open || now.folder !== folder || now.olderThanDays !== days) return;
  now = { ...now, could: countsIn(answer) };
  publish();
}

/** Odpowiedź Rusta, kiedy naprawdę niesie liczby — inaczej `null`, czyli „nie wiemy". */
function countsIn(answer: unknown): CouldForget | null {
  if (typeof answer !== 'object' || answer === null) return null;
  const could = answer as CouldForget;
  return typeof could.workFolders === 'number' && typeof could.branches === 'number' ? could : null;
}

/**
 * Zmienia liczbę dni w kontrolce daty i przelicza podgląd dla NIEJ.
 *
 * Wyczyszczone pole daje `NaN`, a zero znaczyłoby „zapomnij wszystko" — obie wartości zostawiają
 * poprzednią liczbę, bo obie stoją nad kontrolką, która KASUJE, i żadna nie jest wyborem.
 */
export function askAboutRunsOlderThan(days: number): void {
  if (!Number.isFinite(days) || days < 1) return;
  if (!now.open || now.olderThanDays === days) return;
  // Podgląd starej liczby schodzi razem z nią: zdanie „3 runs" nad polem, w którym stoi już inna
  // liczba dni, mówi o czymś, o co nikt nie pytał.
  now = { ...now, olderThanDays: days, could: null };
  publish();
  void learnWhatCouldGo();
}

/**
 * „Forget them" — zdejmuje to, co zostawiły biegi, których Loadout nie zamknął.
 *
 * TUTAJ, A NIE W KOMPONENCIE, z tego samego powodu, co [`forgetTheBranches`] wyżej: to repo nie
 * ma jsdom, więc `onClick` nie odpala się w żadnym kryterium, a polityka zamknięta w handlerze
 * byłaby kodem, którego nic nie sądzi (niezmiennik 16).
 *
 * ZDANIE RUSTA IDZIE NA EKRAN ZAWSZE, także po udanym zdjęciu, i to jest cała treść tej
 * kontrolki: Rust zostawia katalog z niezapisaną zmianą i gałąź z commitem, którego nie ma reszta
 * projektu — a człowiek, który przeczyta samo „gotowe", naciśnie drugi raz nad tym samym stanem.
 */
export async function forgetTheLeftovers(): Promise<void> {
  const folder = now.folder;
  if (!now.open) return;
  try {
    const done = await forgetWhatTheOldRunsLeft(folder);
    sayInHistory(typeof done?.said === 'string' ? done.said : COULD_NOT_SWEEP);
  } catch (error: unknown) {
    sayInHistory(why(error, COULD_NOT_SWEEP));
  }
  // LICZBY PRZELICZAMY Z DYSKU, nie odejmujemy ich w oknie: to, co zostało, wie wyłącznie Rust,
  // a odjęcie „ile prosiliśmy" pokazałoby zero nad projektem, w którym stoi katalog z pracą.
  await learnWhatCouldGo();
}

/**
 * „Forget runs older than N days" — zdejmuje stare biegi razem z ich gałęziami i katalogami.
 *
 * LISTA WRACA Z DYSKU. Odpowiedź niesie liczby, nie nazwy, więc nie ma czego odfiltrować
 * w oknie — a lista zostawiona taka, jaka była, pokazywałaby wiersze, których nie da się już
 * otworzyć. Pliki są prawdą (niezmiennik 4), więc pytamy o nią tę samą krawędź, którą ten panel
 * wstał.
 */
export async function forgetTheOldRuns(): Promise<void> {
  const folder = now.folder;
  if (!now.open) return;
  try {
    const done = await forgetRunsOlderThan(folder, now.olderThanDays);
    sayInHistory(typeof done?.said === 'string' ? done.said : COULD_NOT_FORGET_OLD);
  } catch (error: unknown) {
    sayInHistory(why(error, COULD_NOT_FORGET_OLD));
    return;
  }
  try {
    const rows = await listRuns(folder);
    if (now.open && now.folder === folder) {
      now = { ...now, rows, opened: null };
      publish();
    }
  } catch {
    /* Lista, której nie dało się odczytać po zdjęciu, mówi o sobie sama przy następnym
     * `/history`. Zdanie o tym, co zeszło, stoi już na ekranie i jest tym, po co naciskano. */
  }
  await learnWhatCouldGo();
}

function publish(): void {
  for (const listener of [...listeners]) listener();
}
