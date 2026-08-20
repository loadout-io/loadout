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
 * SKĄD BIERZE SIĘ SAMA TOŻSAMOŚĆ. Z licznika w tym module, a nie z `crypto.randomUUID()` ani
 * z zegara, i to jest wybór o dwóch nazwanych powodach. Pierwszy: ta wartość nigdy nie dotyka
 * dysku ani drutu jako klucz trwały — terminal jest stanem UI (niezmiennik 4), więc jedyne, czego
 * od niej wymagamy, to żeby dwa terminale jednego okna nie zderzyły się ze sobą. Drugi: licznik
 * jest DETERMINISTYCZNY, więc kryterium czyta w komunikacie porażki `terminal-2` zamiast uuida,
 * którego nie da się powtórzyć — a to jest różnica między raportem, z którym da się coś zrobić,
 * i takim, który trzeba odtwarzać.
 *
 * DLACZEGO PREFIKS, A NIE SAMA LICZBA. Bo w tym samym polu `id` stoją dziś DWA rodzaje wartości:
 * tożsamość terminalu (stąd) i ścieżka folderu (`./store.ts`, `cardForRun` — karta biegu, który
 * właśnie ruszył, nazywa się folderem tego biegu, bo bieg jest jeden na aplikację i należy do
 * folderu, nie do karty). Prefiks `terminal-` nie zderzy się ze ścieżką bezwzględną nigdy, bo ta
 * zaczyna się od `/`. Dzień, w którym bieg dostanie własną tożsamość na drucie (etap B), jest
 * dniem, w którym ten drugi rodzaj wartości znika.
 */
import type { WorkspaceTab } from '../../../state/run-tabs';

/**
 * Ile terminali to okno już wybiło.
 *
 * Na poziomie modułu, bo tożsamość ma być unikalna w całym oknie, a nie w obrębie jednego
 * ekranu: wyjście do Agentów odmontowuje sekcję Bieg, a licznik trzymany w komponencie
 * wróciłby wtedy do zera i drugi terminal dostałby tożsamość pierwszego.
 */
let minted = 0;

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
export function newTerminal(folder: string, name: string): WorkspaceTab {
  minted += 1;
  return {
    id: 'terminal-' + String(minted),
    name,
    path: folder,
    /* Zero, i nie ma tu innej możliwej wartości: świeży terminal jest miejscem, w którym jeszcze
     * nikt nie pracuje. Liczba pracujących agentów przychodzi z listy agentów biegu i pisze ją
     * ekran (`../index.tsx`), więc kropka „tu coś chodzi" nad nowo otwartą kartą byłaby relacją,
     * której w danych nie ma (niezmiennik 17). */
    agents: 0,
  };
}
