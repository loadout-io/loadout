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
import { TitleBar } from './ui/shell/titlebar';

export interface AppProps {
  section: Section;
  /**
   * Ekrany sekcji. Powłoka jest STEROWANA: mapa wchodzi propsem, więc test nie potrzebuje
   * ani jednego prawdziwego pliku sekcji. Bez propsu powłoka bierze to, co znalazła sama.
   */
  screens?: ScreenMap;
}

export function App({ section, screens }: AppProps): ReactElement {
  const entry = sectionEntry(section);
  return (
    <div className="flex h-full flex-col bg-bg">
      <TitleBar section={section} />
      <main data-section={entry.id} className="min-h-0 flex-1 p-4">
        {sectionBody(screens, entry)}
      </main>
    </div>
  );
}

/* SZKIELET FAZY KONTRAKTU — odpowiednik `todo!()` z Rusta i jedyne miejsce, w którym T-25
 * zmienia zachowanie powłoki.
 *
 * Rzuca, bo wyboru „ekran albo zdanie z rejestru" jeszcze nie ma. Dzięki temu kryteria padają
 * W CZASIE WYKONANIA, na braku zachowania, a nie przy wczytywaniu modułu — a to jest różnica
 * między czerwienią, która coś poświadcza, a podpisem z NOT_A_REAL_RED (AGENTS.md §2a p. 5).
 * Z tego samego powodu domyślna mapa ekranów NIE jest tu jeszcze liczona stałą modułową:
 * rzucenie w czasie wczytywania modułu wywróciłoby zbieranie testów, czyli nie uruchomiłoby
 * niczego. Implementacja zastępuje to ciało w całości; nic z tej funkcji nie ma prawa dożyć
 * pełnej bramki.
 */
function sectionBody(_screens: ScreenMap | undefined, _entry: SectionEntry): ReactElement {
  throw new Error('not implemented');
}
