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
import type { PastRun, PastRunRow } from '../io';
import { forgetRunBranches } from '../io';

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
}

const CLOSED: PastState = {
  open: false,
  folder: null,
  rows: [],
  opened: null,
  said: null,
};

let now: PastState = CLOSED;

const listeners = new Set<() => void>();

/** Otwiera panel na LIŚCIE biegów tego zakresu. */
export function showHistory(folder: string | null, rows: readonly PastRunRow[]): void {
  now = { open: true, folder, rows, opened: null, said: null };
  publish();
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

function publish(): void {
  for (const listener of [...listeners]) listener();
}
