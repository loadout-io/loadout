/* Jak daleko sięga jedna rzecz, którą model wie — JEDEN słownik na obie połowy sekcji.
 *
 * PO CO TO ISTNIEJE, zmierzone 2026-08-31. Ta sama oś miała dwa brzmienia po dwóch stronach
 * jednej sekcji: notatka sięgająca poza bieżący projekt mówiła o sobie „Every project",
 * a umiejętność dokładnie w tym samym położeniu — „Everywhere". Dwa napisy na jeden fakt
 * czyta się jako dwie różne rzeczy i nie ma stąd drogi powrotnej: człowiek nie ma jak się
 * dowiedzieć, że to jedna oś, bo nic na ekranie tego nie mówi (niezmiennik 13).
 *
 * „Every project", a nie „Everywhere", bo tworzy PARĘ z drugą pozycją. „This project"
 * i „Every project" różnią się jednym słowem i to słowo niesie całą różnicę; „This project"
 * i „Everywhere" wyglądają jak dwa niepowiązane napisy, między którymi trzeba się domyślić
 * relacji.
 *
 * SŁOWA GRANICY TU NIE MA. `scope` notatki (`everywhere`, `this-project`) i `Landing`
 * umiejętności są nazwami drutu i na ekran nie jadą nigdy (niezmiennik 14) — ten plik jest
 * jedynym miejscem, w którym jedno zamienia się w drugie.
 */

/** Sięga tylko tam, gdzie człowiek teraz pracuje. */
export const THIS_PROJECT = 'This project';

/** Sięga wszędzie — do każdego projektu na tej maszynie. */
export const EVERY_PROJECT = 'Every project';

/**
 * Sięga do jednego agenta i do nikogo więcej.
 *
 * „Only", bo to jest CAŁA treść tego zakresu: sama nazwa agenta obok długości jest nazwą bez
 * zdania, a człowiek nie ma jak zgadnąć, czy to autor, czy adresat.
 */
export function onlyAgent(agent: string): string {
  return 'Only ' + agent;
}
