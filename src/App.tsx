/* Powłoka: chrome plus DOKŁADNIE jedna sekcja.
 *
 * „Dokładnie jedna" znaczy jedna w drzewie, nie jedna widoczna. Pięć sekcji zamontowanych naraz
 * i cztery schowane CSS-em to „always-mounted route stack", przez który poprzedni prototyp renderował
 * 142 elementy niosące tekst przy suficie 60 [raport 03 §4.1]. Dlatego niżej nie ma ani pętli
 * po SECTIONS, ani atrybutu `hidden`, ani `display: none` — jest jeden `<main>` i jeden wpis.
 *
 * Czego tu świadomie nie ma: paska loadoutu, szyny agentów, paska postępu. Nie ma biegu, więc
 * nie ma czego pokazywać, a atrapa w powłoce zostaje w niej na zawsze (niezmiennik 17).
 */
import type { ReactElement } from 'react';
import type { Section } from './ui/sections';
import { sectionEntry } from './ui/sections';
import { EmptyState } from './ui/primitives/empty-state';
import { TitleBar } from './ui/shell/titlebar';

export interface AppProps {
  section: Section;
}

export function App({ section }: AppProps): ReactElement {
  const entry = sectionEntry(section);
  return (
    <div className="flex h-full flex-col bg-bg">
      <TitleBar section={section} />
      <main data-section={entry.id} className="min-h-0 flex-1 p-4">
        <EmptyState>{entry.empty}</EmptyState>
      </main>
    </div>
  );
}
