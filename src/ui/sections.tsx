/* Rejestr sekcji — jedyne miejsce, w którym jest napisane, ile sekcji ma powłoka i jak się
 * nazywają. Kolejność jest częścią kontraktu (ARCHITECTURE §3, decyzja D5), więc mieszka
 * w tablicy, a nie w kolejności importów gdziekolwiek indziej.
 *
 * Ten plik jest znanym przekazaniem własności: T-08, T-09, T-11, T-13, T-14, T-17 i T-19
 * dopisują tu po jednej linii, mimo że go nie posiadają (TASK.md, „Świadomie poza zakresem").
 *
 * SZKIELET (faza kontraktowa T-01): pusta tablica. Pięć wpisów dokłada faza implementacji —
 * gdyby stały tu teraz, połowa kryterium sekcji byłaby zielona przed napisaniem powłoki.
 */

export type Section = 'run' | 'workflows' | 'agents' | 'skills' | 'memory';

export interface SectionEntry {
  id: Section;
  label: string;
}

export const SECTIONS: readonly SectionEntry[] = [];
