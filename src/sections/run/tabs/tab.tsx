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
 * # Stan tego pliku: SZKIELET (2026-08-16)
 *
 * Pusty fragment. Kryterium 4 pyta o obecność kropki na dwóch kartach i o jej NIEOBECNOŚĆ
 * na trzeciej — więc pusty fragment przechodzi połowę o nieobecności i pada na obu połowach
 * o obecności, co jest dokładnie tym, czego ta warstwa wymaga.
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

export function Tab(_props: TabProps): ReactElement {
  return <></>;
}
