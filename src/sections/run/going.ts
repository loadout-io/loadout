/**
 * Bieg, który idzie **teraz**, albo `null`.
 *
 * Stan modułu, nie stan komponentu, i to jest ta sama decyzja, co przy `runFeed`
 * (`src/sections/run/feed/live.ts`): bieg nie kończy się dlatego, że człowiek wszedł do
 * Agentów. Zapadka trzymana w komponencie znika razem z ekranem sekcji, a wtedy powrót do
 * Pracy i kliknięcie Start startują drugi bieg tego samego workflow.
 *
 * 2026-09 (Z-27): zapadka wyszła z `io.ts`, bo polityka nad krawędzią musi zapytać o nią PRZED
 * zmianą karty. `commands-wired.test.ts` żąda nazwy komendy dla każdej funkcji eksportowanej
 * z `io.ts`, a to pytanie komendą nie jest — osobny moduł zachowuje tę granicę prawdziwą.
 */
/* `unknown`, nie `void`, od 2026-08-23: zapadkę biorą także wznowienie i powtórzenie kroku,
 * a te oddają zdanie o zmienionym pliku. Zapadka nigdy nie czyta tej wartości — pilnuje
 * wyłącznie tego, czy bieg jeszcze trwa — więc typ ma o niej milczeć, zamiast wymuszać
 * rzutowanie u każdego wołającego. */
let going: Promise<unknown> | null = null;

/**
 * Co powiedzieć drugiemu naciśnięciu Run, kiedy pierwszy bieg jeszcze nie wrócił.
 *
 * ZDANIE NAZYWA NASTĘPNY RUCH (DESIGN §8), bo odmowa bez wyjścia zostawia człowieka dokładnie
 * tam, gdzie był — a tutaj wyjście jest jedno kliknięcie dalej. Mówi też DLACZEGO: bez powodu
 * czyta się to jak ograniczenie na złość, a prawdziwy powód jest finansowy — Loadout prowadzi
 * jeden bieg naraz, żeby Stop zawsze sięgał tego, który pracuje.
 *
 * NIE JEST TO DRUGA KOPIA `ALREADY_GOING` z `src-tauri/src/ipc.rs`, choć czyta się podobnie,
 * i nie da się jej stamtąd wziąć: zapadka odpowiada ZAMIAST wołać Rusta — i musi tak robić,
 * bo dwa biegi jednego workflow to dwa zestawy agentów piszących po tych samych plikach
 * (niezmiennik 12) — więc po tamtej stronie granicy nikt tej sytuacji nie widzi.
 */
export const ONE_RUN_AT_A_TIME =
  'That run is still going, and Loadout leads one at a time so that Stop always reaches the one ' +
  'that is working. Press Stop first, then press Run again.';

/** Czy jeden bieg nadal trzyma zapadkę. */
export function aRunIsGoing(): boolean {
  return going !== null;
}

/** Trzyma zapadkę do chwili rozstrzygnięcia tej obietnicy przez jej właściciela. */
export function holdTheRun(run: Promise<unknown>): void {
  going = run;
}

/** Zwalnia zapadkę po każdej drodze zejścia biegu. */
export function letTheRunGo(): void {
  going = null;
}
