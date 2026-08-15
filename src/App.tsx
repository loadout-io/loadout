/* Powłoka: chrome plus DOKŁADNIE jedna sekcja.
 *
 * „Dokładnie jedna" znaczy jedna w drzewie, nie jedna widoczna. Pięć sekcji zamontowanych naraz
 * i cztery schowane CSS-em to „always-mounted route stack", przez który poprzedni prototyp renderował
 * 142 elementy niosące tekst przy suficie 60 [raport 03 §4.1].
 *
 * SZKIELET (faza kontraktowa T-01): pusty div. Powłoka dopisuje się w fazie implementacji.
 */
import type { ReactElement } from 'react';
import type { Section } from './ui/sections';

export interface AppProps {
  section: Section;
}

export function App(_props: AppProps): ReactElement {
  return <div />;
}
