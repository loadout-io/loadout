/* Jedna karta paska (makieta `docs/mockup/index.html:359-363`): kropka, nazwa biegu, `×`.
 *
 * 2026-08-18 — KARTA MÓWI O BIEGU, NIE O FOLDERZE. Do dziś nazwą karty była nazwa folderu, bo
 * karta znaczyła folder. Właściciel rozstrzygnął inaczej: folder pracy jest zakresem wybieranym
 * w bocznym menu, a pasek kart pokazuje biegi w tym zakresie. Kształt karty nie zmienił się ani
 * o piksel — zmieniło się, CO stoi w jej dwóch napisach: `name` to nazwa workflow, który ten
 * bieg wykonuje, a `path` (podpowiedź) to folder, w którym pracuje. Dokładnie te dwa napisy
 * człowiek musi zobaczyć, żeby wiedzieć, co ubija znakiem `×`.
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
 * KTÓRA KARTA JEST NA WIERZCHU, MÓWI DOKŁADNIE JEDNO POLE — props `active` (niezmiennik 13).
 * Z niego bierze się i `aria-current` na przycisku wyboru, i klasa otoczki, w jednym wyrażeniu:
 * druga odpowiedź na to pytanie nie ma tu gdzie powstać. Napis i waga jadą wariantem
 * `aria-[current=true]:`, czyli przez ten sam atrybut — tak samo robi przełącznik sekcji
 * w `ui/shell/titlebar.tsx`.
 */
import type { ReactElement } from 'react';
import type { WorkspaceTab } from '../../../state/run-tabs';

export interface TabProps {
  /** Bieg, o którym ta karta opowiada: nazwa workflow, folder w podpowiedzi, kropka od agentów. */
  readonly workspace: WorkspaceTab;
  /** Czy to jest karta, na którą człowiek patrzy. */
  readonly active: boolean;
  /** Wymagany: wybranie karty (niezmiennik 16). */
  readonly onSelect: (id: string) => void;
  /** Wymagany: `×`. Zamykanie z żywym biegiem pyta — decyduje o tym magazyn kart. */
  readonly onClose: (id: string) => void;
}

/* Karta na pełną wysokość paska i z linią po prawej — makieta `.tab`: `border-right`, mono 12,
 * bez zaokrąglenia i bez własnego tła. Karta niższa od paska zostawia nad sobą i pod sobą pasek
 * innego koloru, czyli rysuje szczelinę, której w makiecie nie ma.
 *
 * KARTA NA WIERZCHU RÓŻNI SIĘ TRZEMA RZECZAMI, nie jedną: tłem `--panel` (pasek jest `--well`,
 * więc karta wygląda jak wysunięta ku widzowi), wagą 700 i AKCENTEM OD GÓRY (`box-shadow:
 * inset 0 2px var(--accent)`). Do 2026-08-18 była to jedna rzecz — `bg-raised` — czyli różnica
 * mniejsza niż między dwoma sąsiednimi powierzchniami motywu. */
/* MYJKA POD KURSOREM, dopisana 2026-08-31. Karta w tle nie odpowiadala na najechanie ani
 * jednym pikselem, wiec az do klikniecia nic nie mowilo, ze da sie ja kliknac. `--hover` jest
 * tym samym podswietleniem, ktore niosa prymitywy `.btn` i `.row`; karta nie bierze `.row`
 * w calosci, bo `.row[aria-current]` przemalowalaby karte na wierzchu na `--raised`, a ona
 * ma tam wlasne trzy roznice (`ON_TOP`). */
const TAB =
  'flex h-full items-center gap-2 pr-1 pl-[13px] font-mono text-mono text-muted' +
  ' transition-colors hover:bg-hover hover:text-ink' +
  ' aria-[current=true]:text-ink aria-[current=true]:text-mono-strong';

/* Tło i akcent siedzą na OTOCZCE, nie na przycisku wyboru, bo karta na wierzchu to cały jej
 * kawałek paska razem z `×` — inaczej znak zamknięcia zostaje na kolorze paska i wygląda jak
 * przyklejony do sąsiedniej karty.
 *
 * Akcent od góry przez `inset`, nie przez `border-top`: obwódka przesunęłaby treść karty o 2 px
 * w dół i tylko na karcie aktywnej, czyli napis skakałby przy każdym przełączeniu. */
const ON_TOP = 'bg-panel shadow-[inset_0_2px_var(--color-accent)]';

export function Tab({ workspace, active, onSelect, onClose }: TabProps): ReactElement {
  return (
    <span
      className={`flex h-full shrink-0 items-center border-r border-line ${active ? ON_TOP : ''}`}
    >
      <button
        type="button"
        onClick={() => {
          onSelect(workspace.id);
        }}
        /* Znacznik karty. Istnieje, żeby dało się zapytać o KARTY, a nie o „każdy przycisk
         * z podpowiedzią" — a to jest różnica, na której 2026-08-18 padły dwa kryteria, kiedy
         * na pasek wrócił `＋` (też przycisk, też z podpowiedzią). Nazwa mówi, czym ten element
         * jest, więc nowy przycisk obok nie zmienia odpowiedzi na pytanie „ile masz kart". */
        data-tab={workspace.id}
        aria-current={active ? 'true' : undefined}
        /* Pełna ścieżka mieszka w podpowiedzi, nie w napisie: karta ma 34 px wysokości i tyle
         * szerokości, ile zostanie po pozostałych, a `~/Projects/…` w napisie zabrałoby całą
         * różnicę między trzema kartami a dwiema. */
        title={workspace.path}
        className={TAB}
      >
        {/* Węzeł, albo go nie ma. Nigdy przygaszony — patrz nagłówek pliku.
            PULSUJE, i to jest ta jedna rzecz, którą karta w tle mówi o sobie: `animate-blip`
            z `src/styles/theme.css` robi SKOK `opacity 1 → 0.35` co 1,4 s (`steps(2, end)`),
            nie przejście — DESIGN §7 zabrania płynnego pulsowania wprost, bo czyta się jak
            oddychanie i oko goni ruch zamiast czytać. Wyłączenie przy `prefers-reduced-motion`
            robi jeden blok na końcu tamtego pliku, dla całej aplikacji naraz. */}
        {/* CORAL, nie akcent, od 2026-08-19: ta kropka odpowiada na pytanie „co się dzieje
            teraz", a nie „co jest klikalne" (T-45 rozszczepił token). Jest jednym z dwóch
            regionów, którym ARCHITECTURE §7 pozwala się ruszać. */}
        {workspace.agents > 0 ? (
          <span data-live-dot className="size-1.5 shrink-0 animate-blip rounded-pill bg-live" />
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
        /* `.btn-bare` z `theme.css`: znak bez obrysu i bez wypelnienia, az do najechania —
           obrys wokol jednego glifu rysuje pudelko, a nie przycisk. Wysokosc bierze pasek
           (`h-full`), bo karta ma siegac obu jego krawedzi. */
        className="btn-bare h-full px-1"
      >
        ×
      </button>
    </span>
  );
}
