/* Tożsamość terminalu — jedno miejsce, w którym powstaje odpowiedź na pytanie „która to karta".
 *
 * PO CO TO ISTNIEJE. Do 2026-08-20 karta BYŁA folderem: `id === folder`, a magazyn kart mówił
 * wprost „w jednym zakresie może stać najwyżej jedna karta" (`./store.ts`). Skutek widział
 * właściciel: `＋` na pasku wołał wybór katalogu, wybrany folder stawał się nowym ZAKRESEM,
 * a pasek kart tego zakresu był pusty — czyli kliknięcie w `＋` nie dokładało karty **nigdy**.
 * Zgłoszenie brzmiało: „jak klikam plusik to powinno po prostu odpalać nowy nasz terminal
 * i sobie tam możemy kolejne workflow w naszym scope co mamy zaznaczone".
 *
 * Terminal musi więc mieć własną tożsamość, a folder staje się jego POLEM. Nośnikiem folderu
 * jest `WorkspaceTab.path` i nie powstaje obok niego drugie pole na tę samą ścieżkę
 * (niezmiennik 13): karta niesie tę wartość od pierwszego dnia, a `id` przestaje ją dublować.
 *
 * DLACZEGO OSOBNY PLIK, A NIE FUNKCJA W `./store.ts`. Bo bicie tożsamości jest czynnością
 * czystą — nie dotyka magazynu, nie dotyka granicy Tauri i da się je osądzić bez okna —
 * a magazyn kart jest egzemplarzem tego okna razem z dwiema czynnościami, które dotykają
 * granicy (zatrzymanie biegu i założenie karty w chwili startu).
 *
 * 2026-08-20 — SZKIELET. Ciało jest `throw`, więc kryterium pada w czasie WYKONANIA, a nie na
 * zbieraniu plików: vitest przewraca się już na brakującym imporcie, a to jest podpis, który
 * bramka odrzuca jako „nie policzone" (AGENTS.md §2a p. 5). Podkreślenia przy nazwach parametrów
 * są częścią tej samej tymczasowości — ciało, które ich nie czyta, dałoby TS6133, a ten kod
 * bramka melduje jako awarię własnej konfiguracji, nie jako czerwień zadania.
 */
import type { WorkspaceTab } from '../../../state/run-tabs';

/**
 * Nowy terminal w tym folderze — świeża tożsamość, folder w polu.
 *
 * Dwa wywołania z tym samym folderem oddają DWA różne terminale i to jest cała treść tej
 * funkcji: człowiek, który nacisnął `＋` dwa razy, prosił o dwa miejsca do pracy w projekcie,
 * który już wybrał, a nie o wybranie projektu drugi raz.
 *
 * @param folder katalog zakresu, w którym ten terminal stoi. Nie jest tu wybierany ani
 *   sprawdzany: wybór mieszka w magazynie zakresów (`src/state/workspaces.ts`), a terminal go
 *   tylko NIESIE.
 * @param name napis, który karta mówi o sobie na pasku. Nazwa workflow wchodzi tu dopiero
 *   wtedy, kiedy w tym terminalu ruszy bieg (`./store.ts`, `cardForRun`).
 */
export function newTerminal(_folder: string, _name: string): WorkspaceTab {
  throw new Error('not implemented');
}
