/* Montaż. Jedyny arkusz stylów, jaki ta aplikacja ładuje, jest importowany TUTAJ i nazywa się
 * `./styles/global.css` — dlatego kryterium palety kompiluje właśnie ten plik, a nie `theme.css`
 * z ręki. Paleta zamknięta w pliku, którego aplikacja nie ładuje, nie zamyka niczego [T8 §6.4].
 */
import type { ReactElement } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { useSectionStore } from './ui/shell/section-store';
import './styles/global.css';

function Root(): ReactElement {
  const section = useSectionStore((state) => state.section);
  return <App section={section} />;
}

const host = document.getElementById('root');
if (host !== null) {
  createRoot(host).render(<Root />);
}
