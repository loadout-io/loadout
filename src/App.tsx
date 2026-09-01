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
import { CommandPalette } from './ui/palette';
import { ScreenBoundary } from './ui/shell/screen-boundary';
import { useSectionStore } from './ui/shell/section-store';
import { NAV_WIDTH, PANE_GAP, SideNav } from './ui/shell/titlebar';

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
    /* Fragment, nie trzecia kolumna siatki: paleta jest warstwą NAD powłoką i przy zamkniętej
       palecie renderuje `null`, więc widok domyślny — ten, który mierzy sufit gęstości
       z ARCHITECTURE §7 — nie dostaje ani jednego elementu więcej (`ui/palette/index.tsx`). */
    <>
      {/* Dwie kolumny, nie pionowy stos: nawigacja stoi OBOK treści, więc do sufitu gęstości
       * z ARCHITECTURE §7 wnosi zero. Geometria jest lustrem reguły `.app` z makiety
       * (`grid-template-columns:196px minmax(0,1fr)`), a `minmax(0,1fr)` zamiast `1fr` dlatego,
       * że bez tego szeroka treść ekranu rozpycha kolumnę zamiast się przewijać. */}
      <div
        className="aurora grid h-full bg-bg"
        style={{
          gridTemplateColumns: `${String(NAV_WIDTH)}px minmax(0,1fr)`,
          /* KARTKI PŁYWAJĄ. Jeden stopień skali odstępów oddziela je od krawędzi okna i od
           siebie, a pod nimi widać aurorę — statyczną winietę przy lewej krawędzi, dzięki
           której szkło ZAWSZE ma co załamywać. To rozwiązanie domu i ma konsekwencję, która
           oszczędza całą klasę pracy: nie potrzebujemy `transparent: true` ani `windowEffects`,
           więc strona Rusta zostaje nietknięta, a wygląd nie zależy od tapety użytkownika.

           Te 8 px WCHODZI do budżetu chrome z ARCHITECTURE §7 i są policzone: 8 + 1 + 32 + 52
           = 93 przy sufi 96 (kryterium AC-1 sumuje je z makiety, nie z tego komentarza). */
          padding: PANE_GAP,
          gap: PANE_GAP,
        }}
      >
        <SideNav section={section} />
        {/* TREŚĆ JEST PAPIEREM, nie szkłem. Reguła nadrzędna systemu, wprost z jego nazwy:
          szkło jest chrome. Kartka treści jest nieprzejrzysta, bo pod tekstem i pod kodem,
          które człowiek ma przeczytać, szkło nie wchodzi nigdy. */}
        <main data-section={entry.id} className="paper min-h-0 min-w-0">
          {/* OSŁONA WOKÓŁ SEKCJI, nie wokół roota: błąd renderu ma kosztować JEDNĄ sekcję,
            a nie okno razem z nawigacją, czyli razem z jedyną drogą wyjścia z tej sekcji
            (`ui/shell/screen-boundary.tsx`, zmierzone 2026-08-18). `key` na identyfikatorze
            sekcji kasuje stan osłony przy każdym przejściu: bez tego jedna zepsuta sekcja
            zostawiałaby zdanie o awarii na ekranie także po przełączeniu na zdrową. */}
          <ScreenBoundary
            key={entry.id}
            section={entry.id}
            onLeave={
              entry.id === 'run'
                ? null
                : () => {
                    useSectionStore.getState().go('run');
                  }
            }
          >
            {isScreen(Screen) ? <Screen /> : <EmptySection entry={entry} />}
          </ScreenBoundary>
        </main>
      </div>
      {/* KLAWIATURA CAŁEGO OKNA MIESZKA TU, w jednym miejscu, i to jest cały rozmiar wpięcia.
          Nasłuch wisi na dokumencie, więc `⌘K`, `?` i skok `G` + litera działają nad każdą
          sekcją — także nad tą, która jeszcze nie ma ekranu. Sekcje nie dostają ani jednego
          propsu i nie wiedzą o palecie nic: gdyby wiedziały, ta sama decyzja mieszkałaby
          w siedmiu miejscach (niezmiennik 13). */}
      <CommandPalette />
    </>
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
 *
 * POŁOWA DŁUGU SPŁACONA 2026-08-31: geometria znaku nie jest już przepisana, tylko nazwana
 * klasą `.mark` z warstwy prymitywów. Dwa ciała zostają, ale nie mogą się już rozjechać
 * KSZTAŁTEM — obie kopie czytają tę samą regułę. Zostaje wyłącznie rozjazd tego, GDZIE siedzi
 * `data-empty`, czyli dokładnie ta rzecz, przez którą ta kopia w ogóle istnieje.
 */
function EmptySection({ entry }: { entry: SectionEntry }): ReactElement {
  return (
    /* WEJŚCIE SPRĘŻYNĄ, 2026-08-31 (DESIGN §7). Puste zaproszenie POJAWIA SIĘ w chwili, w której
       człowiek przełączył sekcję: `key` na osłonie kasuje poprzednie drzewo, więc to jest
       przybycie, a nie przepisanie treści w miejscu. Element pojawiający się skokiem czyta się
       jak przeskok widoku — oko nie wie, czy patrzy na to samo miejsce.
       JEDEN region, więc sufit dwóch z ARCHITECTURE §7 zostaje niewydany: to zdanie stoi
       wyłącznie wtedy, kiedy sekcja NIE MA ekranu, więc nigdy nie animuje się razem z nim. */
    <div className="enter flex h-full flex-col items-center justify-center gap-3">
      <span className="mark">◇</span>
      <p data-empty className="text-ink">
        {entry.empty}
      </p>
    </div>
  );
}
