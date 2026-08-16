/* Jedna karta paska (makieta `docs/mockup/index.html:359-363`): kropka, nazwa folderu, `×`.
 *
 * KROPKA JEST WĘZŁEM, KTÓREGO ALBO NIE MA. Karta bez pracujących agentów nie ma prawa do
 * koloru stanu (DESIGN §3), więc nie ma tu miejsca trzymanego na kropkę i wygaszanego
 * przezroczystością: element z `opacity: 0` dalej zajmuje szerokość, dalej przesuwa napis
 * i dalej przyciąga wzrok przy najlżejszym rozjaśnieniu motywu. Kropka jest albo jej nie ma.
 *
 * KOLOR KROPKI TO `--accent` I NIC POZA TYM. Accent znaczy „teraz" i jest jedynym kolorem
 * interaktywnym w całej aplikacji (DESIGN §3). `--attend`, `--fail` i `--human` odpowiadają
 * na pytania, których karta w tle nie zadaje: karta mówi o sobie dokładnie jedno zdanie —
 * „tu coś chodzi" — a nie „tu coś się zepsuło" i nie „tu ktoś czeka na twoją decyzję"
 * (ARCHITECTURE §6a reguła 4). Cztery kolory stanu i ani jeden więcej to cały słownik
 * semantyczny; piąty sens dołożony do istniejącego koloru jest tym samym błędem, co piąty kolor.
 *
 * DWA PRZYCISKI, NIE JEDEN. Makieta rysuje kartę jako jeden `<button>` z `×` w środku, ale
 * przycisk w przycisku nie istnieje w HTML-u, a `×` musi mieć własny handler (niezmiennik 16):
 * wybranie karty i zamknięcie karty to dwie różne czynności i muszą być dwoma różnymi celami
 * klawiatury. Stąd wiersz z dwoma przyciskami zamiast jednego.
 *
 * Oba handlery są WYMAGANE. Kontrolka bez handlera nie wchodzi do repo; poprzedni prototyp ma trzy
 * martwe przyciski, i to nie dlatego, że ktoś je zaprojektował jako martwe.
 *
 * KTÓRA KARTA JEST NA WIERZCHU, MÓWI DOKŁADNIE JEDEN ATRYBUT — `aria-current` na przycisku
 * wyboru (niezmiennik 13). Wygląd bierze się z tego samego atrybutu przez wariant
 * `aria-[current=true]:`, więc nie ma drugiej kopii tej prawdy w klasie; tak samo robi
 * przełącznik sekcji w `ui/shell/titlebar.tsx`.
 */
import type { ReactElement } from 'react';
import type { WorkspaceTab } from '../../../state/workspaces';

export interface TabProps {
  /** Folder, o którym ta karta opowiada. */
  readonly workspace: WorkspaceTab;
  /** Czy to jest karta, na którą człowiek patrzy. */
  readonly active: boolean;
  /** Wymagany: wybranie karty (niezmiennik 16). */
  readonly onSelect: (id: string) => void;
  /** Wymagany: `×`. Zamykanie z żywym biegiem pyta — decyduje o tym magazyn kart. */
  readonly onClose: (id: string) => void;
}

export function Tab({ workspace, active, onSelect, onClose }: TabProps): ReactElement {
  return (
    <span className="flex shrink-0 items-center">
      <button
        type="button"
        onClick={() => {
          onSelect(workspace.id);
        }}
        aria-current={active ? 'true' : undefined}
        /* Pełna ścieżka mieszka w podpowiedzi, nie w napisie: karta ma 34 px wysokości i tyle
         * szerokości, ile zostanie po pozostałych, a `~/Projects/…` w napisie zabrałoby całą
         * różnicę między trzema kartami a dwiema. */
        title={workspace.path}
        className="flex h-7 items-center gap-2 rounded-sq pr-1 pl-2 text-ui text-muted aria-[current=true]:bg-raised aria-[current=true]:text-ink"
      >
        {/* Węzeł, albo go nie ma. Nigdy przygaszony — patrz nagłówek pliku. */}
        {workspace.agents > 0 ? (
          <span data-live-dot className="size-1.5 shrink-0 rounded-dot bg-accent" />
        ) : null}
        {workspace.name}
      </button>

      <button
        type="button"
        onClick={() => {
          onClose(workspace.id);
        }}
        /* Nazwa folderu w etykiecie, bo `×` bez niej czyta się z czytnika ekranu jako „close"
         * przy każdej z trzech kart tak samo. */
        aria-label={'Close ' + workspace.name}
        className="h-7 rounded-sq px-1 text-muted"
      >
        ×
      </button>
    </span>
  );
}
