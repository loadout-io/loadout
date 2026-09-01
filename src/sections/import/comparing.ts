/* Kto jest porównywany TERAZ, co po tym zostaje na ekranie i które odpowiedzi są już nieaktualne.
 *
 * PO CO OSOBNY MODUŁ, 2026-08-31. `stopComparing()` wołało Rusta i nic poza tym: lokalne
 * `comparing` czyściła dopiero odpowiedź `compareCopies`, w `.finally()`. Kiedy ta odpowiedź
 * nie wracała — agent zawieszony, kanał zerwany, `stop_comparing_copies` odrzucone — wiersz
 * mówił „An agent is comparing the copies now." BEZ KOŃCA, a każdy inny wiersz miał wyłączone
 * pytanie, bo warunek patrzy na `comparing !== null`. Limitu czasu nie ma nigdzie, więc jedynym
 * wyjściem było zamknięcie okna i utrata całego planu.
 *
 * Dowód mieszka w czystym module, bo to jest przejście stanu między kliknięciem a odpowiedzią
 * z drugiej strony granicy: w tym repo nie ma jsdom, a `renderToStaticMarkup` nie odpala
 * `onClick`. Niezmiennik 29 daje na to trzy drogi i to jest pierwsza z nich.
 *
 * MONOTONICZNY NUMER PYTANIA, NIE FLAGA (niezmiennik 7). „Stop" unieważnia TO pytanie, nie
 * każde następne, więc `ask` tylko rośnie, a odpowiedź przedstawia się numerem, z którym
 * wyruszyła. Flaga „zatrzymane" przeciekłaby na pytanie zadane sekundę później i człowiek nie
 * dostałby już żadnej odpowiedzi do końca życia okna.
 *
 * JEDEN OBIEKT NA CAŁĄ TĘ DROGĘ, razem z odpowiedziami. Gdyby odpowiedzi mieszkały w osobnym
 * stanie, spóźniona odpowiedź musiałaby sprawdzić numer pytania W DRUGIM miejscu — a domknięcie
 * obietnicy widzi stan z renderu, w którym powstało, czyli zawsze ten sprzed Stopu.
 */
import type { Comparison } from './setup';

/** Zdanie przy KONKRETNEJ pozycji: przerwane pytanie i nieudane pytanie mówią o jednym wierszu,
 *  nie o całym oknie. Pasek `role="alert"` na górze zostaje przy Scanie i Imporcie. */
export interface Said {
  item: string;
  sentence: string;
}

export interface Comparing {
  /** Pozycja, przy której agent pracuje teraz. Jeden naraz, tak jak po tamtej stronie granicy. */
  at: string | null;
  /** Numer pytania — rośnie przy każdym pytaniu i przy każdym Stopie. */
  ask: number;
  said: Said | null;
  answers: Readonly<Record<string, Comparison>>;
}

export const IDLE: Comparing = { at: null, ask: 0, said: null, answers: {} };

/** Co wiersz mówi po Stopie. Nazywa NASTĘPNY RUCH, nie sam fakt: człowiek, który przerwał
 *  pytanie, stoi dokładnie tam, gdzie przed nim, i ma prawo wiedzieć, co mu zostało. */
export const STOPPED =
  'You stopped that comparison. Nothing about this item changed — ask again, or decide about it yourself.';

/** Nowe pytanie o kopie jednej pozycji. Zdanie po poprzednim znika: dotyczyło innego pytania. */
export function asking(now: Comparing, item: string): Comparing {
  return { at: item, ask: now.ask + 1, said: null, answers: now.answers };
}

/**
 * „Stop": ekran zwalnia się SAM i natychmiast, nie czekając na dowód z drugiej strony.
 *
 * Dowód, że agent zszedł, dalej wraca odpowiedzią na `compareCopies` — ale wiersz nie ma prawa
 * być jego zakładnikiem. Wywołanie, które nie wraca, zabierało tu cały ekran bez drogi wyjścia.
 */
export function stopped(now: Comparing): Comparing {
  if (now.at === null) return now;
  return {
    at: null,
    ask: now.ask + 1,
    said: { item: now.at, sentence: STOPPED },
    answers: now.answers,
  };
}

/**
 * Odpowiedź na pytanie numer `mine`.
 *
 * `said === null` znaczy „człowiek to zatrzymał" i jest WARTOŚCIĄ, nie odmową (niezmiennik 7):
 * wiersz wraca wtedy do swojego pytania, bez odpowiedzi i bez zdania o awarii.
 */
export function answered(
  now: Comparing,
  mine: number,
  item: string,
  said: Comparison | null,
): Comparing {
  // Spóźniona odpowiedź na pytanie, którego już nie ma: zdanie o dwóch kopiach pod pozycją,
  // z której człowiek dawno zszedł, jest zdaniem o czymś, co się nie dzieje.
  if (now.ask !== mine) return now;
  return {
    at: null,
    ask: now.ask,
    said: null,
    answers: said === null ? now.answers : { ...now.answers, [item]: said },
  };
}

/** Pytanie, które się nie udało — zdanie ląduje przy tej pozycji, nie w pasku nad tabelą. */
export function refused(now: Comparing, mine: number, item: string, said: string): Comparing {
  if (now.ask !== mine) return now;
  return { at: null, ask: now.ask, said: { item, sentence: said }, answers: now.answers };
}

/** Zdania zapasowe dla dwóch wywołań tej drogi. Stoją tu, a nie w `setup.tsx`, żeby zdanie
 *  i dopisek o następnym ruchu powstawały w jednym miejscu. */
export const COULD_NOT_ASK = 'Loadout could not ask an agent about those copies.';
export const COULD_NOT_STOP = 'Loadout could not stop that agent.';

/** Co Rust powiedział o nieudanym Stopie, plus następny ruch. Doklejamy zawsze: odmowa
 *  z tamtej strony nazywa przyczynę, a nie to, co człowiekowi zostało. */
export function stopFailed(said: string): string {
  return `${said} This row is free again — ask again, or decide about it yourself.`;
}

/** To samo dla pytania, które nie doszło. */
export function askFailed(said: string): string {
  return `${said} Ask another agent, or decide about it yourself.`;
}
