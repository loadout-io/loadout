/* JEDYNY dom domyślnego lidera po stronie okna (niezmiennik 13).
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
import { readSettings, saveSettings } from './settings-io';

let chosen = '';
const listeners = new Set<() => void>();

/** Identyfikator agenta, który prowadzi domyślnie, albo `''`, dopóki nikt nie wybierał. */
export function defaultLead(): string {
  return chosen;
}

/** Prenumerata w kształcie, którego chce `useSyncExternalStore`. */
export function subscribeToDefaultLead(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function remember(id: string): void {
  if (id === chosen) return;
  chosen = id;
  for (const listener of listeners) listener();
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

/** Jedno pytanie do dysku na okno; następni wołający dostają tę samą obietnicę. */
let asked: Promise<string | null> | null = null;

/**
 * Czyta wybór z dysku. Oddaje zdanie odmowy dla człowieka albo `null`, kiedy się udało.
 *
 * Wołane przy montowaniu przez ekran Settings i przez pasek Run — bo to Run jest miejscem,
 * w którym ten wybór widać, a okno otwarte prosto na Run nigdy nie przechodzi przez Settings.
 */
export function loadSettings(): Promise<string | null> {
  asked ??= readSettings()
    .then((settings) => {
      remember(leadIn(settings));
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
 */
export function chooseDefaultLead(id: string): Promise<string | null> {
  return saveSettings({ defaultLead: id })
    .then((settings) => {
      remember(leadIn(settings));
      /* Zapisany wybór jest od tej chwili tym, co odda `loadSettings()` następnemu ekranowi:
       * bez tego powrót na Run czytałby dysk odpowiedzią zapamiętaną przed zapisem. */
      asked = Promise.resolve(null);
      return null;
    })
    .catch((error: unknown) => why(error, 'Loadout could not save who leads by default.'));
}
