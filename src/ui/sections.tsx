/* Rejestr sekcji — jedyne miejsce, w którym jest napisane, ile sekcji ma powłoka, jak się
 * nazywają, PO CO się do nich przychodzi, czego każda potrzebuje, żeby dała się użyć, i co
 * czyta się na każdej z nich, dopóki jest pusta. Kolejność jest częścią kontraktu
 * (ARCHITECTURE §3, decyzja D5), więc mieszka w tablicy, a nie w kolejności importów
 * gdziekolwiek indziej.
 *
 * Ten plik jest znanym przekazaniem własności: T-08, T-09, T-11, T-13, T-14, T-17 i T-19
 * dopisują tu po jednej linii, mimo że go nie posiadają (TASK.md, „Świadomie poza zakresem").
 *
 * Zdania pustych ekranów są tutaj, a nie w komponencie: pusty ekran to zaproszenie, nie akapit
 * polityki (DESIGN §6), a jedno zdanie na sekcję trzymane obok etykiety nie da się rozjechać
 * z listą sekcji.
 *
 * ── KOLEJNOŚĆ ZMIENIONA 2026-08-31, i to jest zmiana produktu, nie porządków ────────────────
 *
 * Stało tu `run, workflows, agents, …`, czyli od końca drogi do jej początku. Człowiek, który
 * otwiera tę aplikację pierwszy raz, nie ma czego uruchomić: workflow to agenci w rzędzie, więc
 * bez agenta nie ma czego postawić w rzędzie, a bez rzędu nie ma czego uruchomić. Lista, która
 * zaczyna się od Run, każe mu zacząć od jedynej rzeczy, której zrobić nie może.
 *
 * Nowa kolejność jest drogą: **zrób** (Agents, Workflows) → **uruchom** (Run, Triggers) →
 * **wiedz** (Knowledge, Lab), a Settings osobno na dole, bo nie jest miejscem, do którego się
 * przychodzi pracować. Wyrocznią jest makieta (`docs/mockup/index.html`, `<nav class="nav">`)
 * i sądzi ją `src/ui/shell/shell-matches-mockup.test.tsx`.
 *
 * Co ta kolejność ZMIENIA POZA MENU, i dlaczego to jest bezpieczne: `src/ui/palette/keys.ts`
 * wyprowadza z tej tablicy litery skoku (pierwsza sekcja z daną literą ją bierze) oraz numery
 * `⌘1`…`⌘7` (pozycja w tablicy). Siedem identyfikatorów ma dalej siedem różnych pierwszych
 * liter, więc ani jeden skok nie zniknął; numery zmieniły się razem z listą i to jest ta sama
 * jedna prawda, czytana raz.
 *
 * Sekcja, na której powłoka się otwiera, NIE jest pierwszym wierszem tej tablicy — mieszka
 * w `src/ui/shell/section-store.ts` i dalej jest nią Run.
 */

/** Po co człowiek przychodzi do tej grupy pozycji. `null` znaczy „ta pozycja stoi osobno". */
export type Purpose = 'Make' | 'Run' | 'Know' | null;

export const SECTIONS = [
  {
    id: 'agents',
    label: 'Agents',
    purpose: 'Make',
    /* CO TA POZYCJA TRZYMA — półka, której liczba stoi przy niej w menu. `null` znaczy „tego
     * się tu nie liczy" i tak jest wszędzie poza dwiema półkami biblioteki: liczba przy
     * pozycji ma odpowiadać na „ile tego mam", a nie być ozdobą przy każdym wierszu. */
    holds: 'agents',
    /* Pierwszy przystanek drogi: nic go nie blokuje i nic nie ma prawa go zablokować. */
    needs: null,
    empty: 'Agents you add will be listed here.',
  },
  {
    id: 'workflows',
    label: 'Workflows',
    purpose: 'Make',
    holds: 'workflows',
    /* CZEGO BRAKUJE, POWIEDZIANE ZDANIEM, NIE KŁÓDKĄ. Sama kłódka mówi „nie wolno" i zostawia
     * człowieka z pytaniem, którego nie ma komu zadać; zdanie mówi, co zrobić, żeby zniknęła.
     * `shelf` jest tym, co się LICZY, i liczy się to z biblioteki (`what-you-have.ts`) —
     * pozycja przygaszona bez odczytu byłaby twierdzeniem o danych, których nikt nie widział
     * (niezmiennik 17). */
    needs: { shelf: 'agents', why: 'Make an agent first — a workflow is agents in a row' },
    empty: 'Workflows you build will be listed here.',
  },
  {
    id: 'run',
    label: 'Run',
    purpose: 'Run',
    holds: null,
    needs: { shelf: 'workflows', why: 'Needs a workflow to run' },
    empty: 'Your work will show up here.',
  },
  {
    id: 'triggers',
    label: 'Triggers',
    purpose: 'Run',
    holds: null,
    needs: { shelf: 'workflows', why: 'Needs a workflow to start on its own' },
    empty: 'Configured triggers will be listed here.',
  },
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
  {
    id: 'knowledge',
    label: 'Knowledge',
    purpose: 'Know',
    holds: null,
    /* Notatkę można napisać, zanim istnieje ktokolwiek, kto ją przeczyta — i tak bywa: człowiek
     * zaczyna od spisania tego, co jego kod ma z sobą wspólnego. Nic tu nie blokujemy. */
    needs: null,
    empty: 'What your agents know will be listed here.',
  },
  /* `lab` PRZYSZŁO Z TRUNKU, nie z tej gałęzi, i zostaje osobną pozycją. 2026-08-31, scalenie.
   *
   * Kusi, żeby dołożyć je do Knowledge — obie rzeczy dotyczą agentów. To byłby błąd tej samej
   * klasy, którą Knowledge właśnie naprawia, tylko w drugą stronę: Knowledge odpowiada na
   * pytanie „co model WIE", a Lab na „czy ten agent DZIAŁA". Pierwsze jest biblioteką, którą
   * się kuruje, drugie pomiarem, który się uruchamia. Jedna szuflada na oba dawałaby wybór
   * dwa razy w odpowiedzi na dwa różne pytania. */
  {
    id: 'lab',
    label: 'Lab',
    purpose: 'Know',
    holds: null,
    needs: { shelf: 'agents', why: 'Needs an agent to try things on' },
    empty: 'Sets you build to test agents will be listed here.',
  },
  {
    id: 'settings',
    label: 'Settings',
    holds: null,
    /* Poza trzema grupami i na dole listy: nie przychodzi się tu pracować, przychodzi się
     * zmienić to, jak pracuje reszta. */
    purpose: null,
    needs: null,
    empty: 'What Loadout does by default lives here.',
  },
] as const;

/* Bez routera, bez URL-i, bez historii: T8 §6.2 mówi wprost, że to jest `type Section`
 * w stanie interfejsu. Typ jest WYPROWADZONY z tablicy wyżej — dzięki temu nie da się dopisać
 * wariantu bez sekcji ani sekcji bez wariantu, a to jest dokładnie ta para, która rozjeżdża
 * się po cichu. */
export type Section = (typeof SECTIONS)[number]['id'];

export type SectionEntry = (typeof SECTIONS)[number];

/** Półka biblioteki, której pustka zamyka sekcję. */
export type Shelf = 'agents' | 'workflows';

/** Wpis o tym identyfikatorze. */
export function sectionEntry(id: Section): SectionEntry {
  /* `find` zawsze trafia, bo `Section` jest zbudowany z identyfikatorów tej właśnie tablicy —
   * ale kompilator tego nie udowodni, a rzucanie wyjątkiem w widoku zabiera cały ekran zamiast
   * jednej sekcji. Pierwszy wpis jest tu więc wyłącznie po to, żeby funkcja była całkowita. */
  return SECTIONS.find((entry) => entry.id === id) ?? SECTIONS[0];
}
