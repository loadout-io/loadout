/* Rejestr sekcji — jedyne miejsce, w którym jest napisane, ile sekcji ma powłoka, jak się
 * nazywają i co czyta się na każdej z nich, dopóki jest pusta. Kolejność jest częścią kontraktu
 * (ARCHITECTURE §3, decyzja D5), więc mieszka w tablicy, a nie w kolejności importów
 * gdziekolwiek indziej.
 *
 * Ten plik jest znanym przekazaniem własności: T-08, T-09, T-11, T-13, T-14, T-17 i T-19
 * dopisują tu po jednej linii, mimo że go nie posiadają (TASK.md, „Świadomie poza zakresem").
 *
 * Zdania pustych ekranów są tutaj, a nie w komponencie: pusty ekran to zaproszenie, nie akapit
 * polityki (DESIGN §6), a jedno zdanie na sekcję trzymane obok etykiety nie da się rozjechać
 * z listą sekcji.
 */

export const SECTIONS = [
  { id: 'run', label: 'Run', empty: 'Your work will show up here.' },
  { id: 'workflows', label: 'Workflows', empty: 'Workflows you build will be listed here.' },
  { id: 'agents', label: 'Agents', empty: 'Agents you add will be listed here.' },
  /* JEDNA POZYCJA ZAMIAST DWÓCH, decyzja właściciela 2026-08-31.
   *
   * Do tego dnia stały tu `skills` i `memory`, a człowiek wybierał dwa razy w odpowiedzi
   * na jedno swoje pytanie: „co ten model wie o mojej pracy". Różnica między jednym a drugim
   * jest przy tym najważniejszą rzeczą na tym ekranie — notatka w użyciu wchodzi do KAŻDEGO
   * promptu, a po umiejętność model sięga sam, kiedy pasuje — i była powiedziana raz,
   * mimochodem. Dwie sąsiednie pozycje menu nie mówiły jej wcale.
   *
   * MAGAZYNY ZOSTAJĄ OSOBNE (`src/state/skills.ts`, `src/state/memory.ts`) i to jest
   * rozstrzygnięcie, nie zaniechanie: umiejętność bywa cudza i wykonywalna, więc przechodzi
   * przez przegląd bezpieczeństwa z blokującymi znaleziskami; notatka jest własna
   * i deklaratywna, i konkuruje o twardy limit długości. Scalony jest EKRAN, nie polityka. */
  { id: 'knowledge', label: 'Knowledge', empty: 'What your agents know will be listed here.' },
  /* `lab` PRZYSZŁO Z TRUNKU, nie z tej gałęzi, i zostaje osobną pozycją. 2026-08-31, scalenie.
   *
   * Kusi, żeby dołożyć je do Knowledge — obie rzeczy dotyczą agentów. To byłby błąd tej samej
   * klasy, którą Knowledge właśnie naprawia, tylko w drugą stronę: Knowledge odpowiada na
   * pytanie „co model WIE", a Lab na „czy ten agent DZIAŁA". Pierwsze jest biblioteką, którą
   * się kuruje, drugie pomiarem, który się uruchamia. Jedna szuflada na oba dawałaby wybór
   * dwa razy w odpowiedzi na dwa różne pytania. */
  { id: 'lab', label: 'Lab', empty: 'Sets you build to test agents will be listed here.' },
  { id: 'triggers', label: 'Triggers', empty: 'Configured triggers will be listed here.' },
  { id: 'settings', label: 'Settings', empty: 'What Loadout does by default lives here.' },
] as const;

/* Bez routera, bez URL-i, bez historii: T8 §6.2 mówi wprost, że to jest `type Section`
 * w stanie interfejsu. Typ jest WYPROWADZONY z tablicy wyżej — dzięki temu nie da się dopisać
 * wariantu bez sekcji ani sekcji bez wariantu, a to jest dokładnie ta para, która rozjeżdża
 * się po cichu. */
export type Section = (typeof SECTIONS)[number]['id'];

export type SectionEntry = (typeof SECTIONS)[number];

/** Wpis o tym identyfikatorze. */
export function sectionEntry(id: Section): SectionEntry {
  /* `find` zawsze trafia, bo `Section` jest zbudowany z identyfikatorów tej właśnie tablicy —
   * ale kompilator tego nie udowodni, a rzucanie wyjątkiem w widoku zabiera cały ekran zamiast
   * jednej sekcji. Pierwszy wpis jest tu więc wyłącznie po to, żeby funkcja była całkowita. */
  return SECTIONS.find((entry) => entry.id === id) ?? SECTIONS[0];
}
