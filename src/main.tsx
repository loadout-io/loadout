/* Montaż. Jedyny arkusz stylów, jaki ta aplikacja ładuje, jest importowany TUTAJ i nazywa się
 * `./styles/global.css` — dlatego kryterium palety kompiluje właśnie ten plik, a nie `theme.css`
 * z ręki. Paleta zamknięta w pliku, którego aplikacja nie ładuje, nie zamyka niczego [T8 §6.4].
 */
import { useEffect } from 'react';
import type { ReactElement } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { useTriggers } from './state/triggers';
import { useSectionStore } from './ui/shell/section-store';
import './styles/global.css';

function Root(): ReactElement {
  const section = useSectionStore((state) => state.section);

  /* Trigger nie nalezy do ekranu Triggers. Zegar ma zyc tak dlugo jak otwarte okno, takze
   * wtedy, gdy czlowiek ani razu nie wejdzie do tej sekcji. Cleanup jest symetryczny, zeby
   * ponowny montaz roota nie zostawil drugiego interwalu pytajacego o te same sprawy. */
  useEffect(() => {
    useTriggers.getState().startWatching();
    return () => {
      useTriggers.getState().stopWatching();
    };
  }, []);

  return <App section={section} />;
}

const host = document.getElementById('root');
if (host !== null) {
  createRoot(host).render(<Root />);
}
