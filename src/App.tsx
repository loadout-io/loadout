/* Powłoka: chrome plus DOKŁADNIE jedna sekcja.
 *
 * „Dokładnie jedna" znaczy jedna w drzewie, nie jedna widoczna. Pięć sekcji zamontowanych naraz
 * i cztery schowane CSS-em to „always-mounted route stack", przez który poprzedni prototyp renderował
 * 142 elementy niosące tekst przy suficie 60 [raport 03 §4.1]. Dlatego niżej nie ma ani pętli
 * po SECTIONS, ani atrybutu `hidden`, ani `display: none` — jest jeden `<main>` i jeden wpis.
 *
 * Czego tu świadomie nie ma: paska loadoutu, szyny agentów, paska postępu. Nie ma biegu, więc
 * nie ma czego pokazywać, a atrapa w powłoce zostaje w niej na zawsze (niezmiennik 17).
 *
 * T-25 dokłada do tego jedną rzecz, całą we wnętrzu `<main>`: sekcja, która ma swój ekran
 * (`src/sections/<id>/index.tsx`), pokazuje TEN ekran; sekcja, która go nie ma, pokazuje zdanie
 * ze swojego wpisu w rejestrze. Zdanie przychodzi z `sectionEntry(id).empty` i tylko stamtąd —
 * literał przepisany tutaj rozjechałby się z rejestrem przy pierwszej zmianie brzmienia
 * (niezmiennik 13).
 */
import type { ReactElement } from 'react';
import type { Section, SectionEntry } from './ui/sections';
import { sectionEntry } from './ui/sections';
import type { ScreenMap } from './ui/screens';
import { discoverScreens, isScreen } from './ui/screens';
import { NAV_WIDTH, SideNav } from './ui/shell/titlebar';

/* Odkrywanie biegnie RAZ, przy wczytaniu modułu, a nie przy każdym renderze: jego odpowiedź
 * zależy wyłącznie od tego, jakie pliki są w paczce, a to w trakcie życia okna nie zmienia się
 * ani razu. Wywołanie w ciele komponentu przeliczałoby tę samą stałą przy każdym przełączeniu
 * sekcji i dawało za każdym razem nowe referencje komponentów, czyli przemontowanie ekranu. */
const DISCOVERED: ScreenMap = discoverScreens();

export interface AppProps {
  section: Section;
  /**
   * Ekrany sekcji. Powłoka jest STEROWANA: mapa wchodzi propsem, więc test nie potrzebuje
   * ani jednego prawdziwego pliku sekcji. Bez propsu powłoka bierze to, co znalazła sama.
   */
  screens?: ScreenMap;
}

export function App({ section, screens = DISCOVERED }: AppProps): ReactElement {
  const entry = sectionEntry(section);
  /* Wielka litera, bo to idzie do JSX jako znacznik. `isScreen`, a nie samo `!== undefined`:
   * pod mapą z dysku może leżeć cokolwiek, a wartość, która nie jest komponentem, ma kosztować
   * JEDNĄ sekcję — jej pusty ekran — a nie całe okno. To to samo pytanie, które przy odkrywaniu
   * zadaje `screensFrom`, i zadaje je ta sama funkcja (niezmiennik 23). */
  const Screen = screens[entry.id];
  return (
    /* Dwie kolumny, nie pionowy stos: nawigacja stoi OBOK treści, więc do sufitu gęstości
     * z ARCHITECTURE §7 wnosi zero. Geometria jest lustrem reguły `.app` z makiety
     * (`grid-template-columns:196px minmax(0,1fr)`), a `minmax(0,1fr)` zamiast `1fr` dlatego,
     * że bez tego szeroka treść ekranu rozpycha kolumnę zamiast się przewijać. */
    <div
      className="grid h-full bg-bg"
      style={{ gridTemplateColumns: `${String(NAV_WIDTH)}px minmax(0,1fr)` }}
    >
      <SideNav section={section} />
      <main data-section={entry.id} className="min-h-0 min-w-0 p-4">
        {isScreen(Screen) ? <Screen /> : <EmptySection entry={entry} />}
      </main>
    </div>
  );
}

/* `empty-state` z DESIGN §6 — i JEDYNY fragment tego pliku, który jest kopią czegoś innego.
 *
 * Znak `◇` w ramce i jedno zdanie stoją już w `src/ui/primitives/empty-state.tsx`, tylko że tam
 * `data-empty` siedzi na OTACZAJĄCYM `<div>`, więc treść tak oznaczonego elementu to „◇ zdanie",
 * nie samo zdanie. Kryterium 6 z T-01 to przepuszczało (liczyło słowa), kryteria 2 i 5 z T-25
 * porównują z `sectionEntry(id).empty` znak w znak — i na prymitywie nie da się ich przejść.
 * `src/ui/primitives/empty-state.tsx` nie jest w bloku OWNS tego zadania, a przeniesienie
 * `data-empty` na `<p>` to zapis poza zakresem (AGENTS.md §7), więc znacznik jest tutaj na
 * elemencie, który niesie samo zdanie, a prymitywu ten plik nie woła.
 *
 * To jest dług, nie rozwiązanie: jeden wygląd w dwóch ciałach rozjedzie się przy pierwszej
 * zmianie w DESIGN §6. Zapisane jako uwaga dla człowieka 2026-08-16 razem z naprawą, która
 * kasuje kopię w całości — `data-empty` na `<p>` w prymitywie i `<EmptyState>` z powrotem tutaj.
 */
function EmptySection({ entry }: { entry: SectionEntry }): ReactElement {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3">
      <span className="flex size-8 items-center justify-center rounded-sq border border-dashed border-line-strong text-muted">
        ◇
      </span>
      <p data-empty className="text-ink">
        {entry.empty}
      </p>
    </div>
  );
}
