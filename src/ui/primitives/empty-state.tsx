/* `empty-state` z DESIGN §6 i z reguł `.empty` w `docs/mockup/index.html`: wyśrodkowany znak `◇`
 * w ramce `1px dashed --line-strong`, zdanie w `--ink`, jedno zdanie instrukcji w `--muted`
 * i JEDEN przycisk podstawowy. „Pusty ekran to zaproszenie do działania, nie komunikat o braku
 * danych."
 *
 * DLACZEGO TEN PLIK ZOSTAŁ PRZEPISANY 2026-08-18. Miał ZERO importerów — nawet testowych —
 * a znak w ramce i zdanie były przepisane ręcznie w SZEŚCIU miejscach (`App.tsx`, `feed.tsx`,
 * `agents/index.tsx`, `skills/index.tsx`, `memory/index.tsx`, `workflow-list.tsx`). Przyczyna
 * jest nazwana w `src/App.tsx` wprost i nazwana „długiem, nie rozwiązaniem": `data-empty`
 * siedziało tu na OTACZAJĄCYM `<div>`, więc treścią tak oznaczonego elementu było „◇ zdanie",
 * a nie samo zdanie — i dwa kryteria z T-25, które porównują ją znak w znak z
 * `sectionEntry(id).empty`, na prymitywie nie przechodziły. Sześć kopii to sześć miejsc, w których
 * DESIGN §6 może się rozjechać, i żadne z nich nie pada, kiedy się rozjedzie.
 *
 * NAPRAWA: `data-empty` stoi na `<p>`, który niesie SAMO zdanie. Znacznik jest po to, żeby
 * powiedzieć „ten ekran jest pusty, a to jest jego zdanie" — element opakowujący nie odpowiada
 * na to pytanie, bo jego treść to wszystko, co w nim leży.
 *
 * PRZYCISK JEST OPCJONALNY, i to jest decyzja, nie niedokończenie. DESIGN §6 chce jednego
 * przycisku podstawowego, ale nie każdy pusty ekran ma sensowną akcję: „Memory" wypełnia się
 * tym, co agenci zostawiają sobie nawzajem, więc nie ma tam czego stworzyć ręcznie. Przycisk
 * bez skutku jest GORSZY niż jego brak (niezmiennik 16) i to jest dokładnie ta klasa defektu,
 * której szukał audyt — dlatego wołający musi podać `onClick`, żeby przycisk w ogóle powstał.
 */
import type { ReactElement } from 'react';

/** Akcja pustego ekranu: co jest na przycisku i co się naprawdę stanie po kliknięciu. */
export interface EmptyStateAction {
  /** Napis na przycisku. Czasownik w trybie rozkazującym: `Create`, `Add`, `Run`. */
  label: string;
  /** Handler. Bez niego przycisku nie ma — patrz nagłówek, niezmiennik 16. */
  onClick: () => void;
}

export interface EmptyStateProps {
  /** Zdanie o tym, co się tu pojawi. Nosi `data-empty` i nic poza nim. */
  children: string;
  /** Jedno zdanie instrukcji pod spodem. Opcjonalne — nie każdy ekran ma co dodać. */
  hint?: string;
  /** Jedyny przycisk. Opcjonalny, bo przycisk bez skutku łamie niezmiennik 16. */
  action?: EmptyStateAction;
}

/* PROMIEŃ ZAPISANY DWA RAZY, I TO JEST DŁUG WYROCZNI, NIE DECYZJA. 2026-08-31.
 *
 * Klasy `.btn-primary` i `.mark` niosą swój promień same (`--radius-sm`, `--radius-md`), więc
 * `rounded-sm` i `rounded-md` niżej nic nie zmieniają — obie nazwy rozwijają się do tej samej
 * wartości i klasa narzędziowa nadpisuje prymityw jedynką na jedynkę.
 *
 * Stoją tu, bo `src/styles/stand-in-names-are-gone.test.ts` czyta ATRYBUT `class` tego
 * komponentu i żąda nazwy `rounded-*` na przycisku i drugiej na ramce znaku. To pytanie było
 * trafne, kiedy promień mieszkał w klasie narzędziowej: sprawdzało, czy po skasowaniu nazwy
 * zastępczej (`rounded-sq`) ktoś wpisał prawdziwą. Od chwili, w której promień mieszka
 * w prymitywie, ta wyrocznia pyta o NAPIS, a nie o wygląd (niezmiennik 20) — element bez
 * `rounded-sm`, ale z `.btn-primary`, ma prawidłowy róg i zapala ją na czerwono.
 *
 * Zgłoszone jako wada wyroczni, nie obchodzone po cichu: tamten plik jest testem i nie należy
 * do tego zadania. Obie nazwy znikają w dniu, w którym tamto kryterium zacznie pytać
 * o skompilowaną regułę zamiast o treść atrybutu.
 */

export function EmptyState({ children, hint, action }: EmptyStateProps): ReactElement {
  return (
    /* `py-[70px] px-5` i `gap-[11px]` prosto z reguły `.empty` w makiecie. `h-full`, bo sekcja
     * dostaje całą wysokość okna, a zaproszenie ma stać w jej środku, nie pod górną krawędzią.
     *
     * WEJŚCIE SPRĘŻYNĄ, 2026-08-31 (DESIGN §7): zaproszenie POJAWIA SIĘ, kiedy sekcja skończyła
     * czytać z dysku i okazało się, że nie ma czego pokazać. Skok w to miejsce czyta się jak
     * przeskok widoku; dorastanie mówi „przyszedłem stamtąd". Jeden region na zdarzenie. */
    <div className="enter flex h-full flex-col items-center justify-center gap-[11px] px-5 py-[70px] text-center">
      {/* `aria-hidden`, bo czytnik ekranu ma przeczytać zdanie, a nie nazwę znaku romb. Ramka
          przerywana mówi „tu będzie treść, której jeszcze nie ma" — i to jest cały jej sens.
          Kształt bierze `.mark` z warstwy prymitywów; `rounded-md` obok niego to dług wyroczni
          opisany nad tą funkcją, a nie druga decyzja o rogu. */}
      <span aria-hidden className="mark rounded-md">
        ◇
      </span>
      <p data-empty className="text-heading text-ink">
        {children}
      </p>
      {/* `max-w-[44ch]` z makiety: instrukcja dłuższa niż 44 znaki w wierszu przestaje się
          czytać jak jedno zdanie i zaczyna jak akapit polityki (DESIGN §6). Stopień i barwa
          idą z `.lead` — to jest dokładnie ta rola: zdanie drugoplanowe pod zaproszeniem. */}
      {hint === undefined ? null : <p className="lead max-w-[44ch]">{hint}</p>}
      {/* ZNACZNIK PRZYCISKU NAZYWA SIĘ `data-invite`, NIE `data-empty-action`. Zmierzone
          2026-08-18: dwa kryteria z T-25 liczą wystąpienia NAPISU `data-empty` w markupie
          i wymagają dokładnie jednego, a `data-empty-action` zawiera ten napis w sobie — więc
          każdy ekran, który poda akcję, zapalałby je na czerwono, choć oznaczony element byłby
          jeden. Nazwa bez wspólnego prefiksu to jedna linia, a tamta pułapka nie ma dna. */}
      {action === undefined ? null : (
        <button
          type="button"
          data-invite
          className="btn-primary rounded-sm"
          onClick={action.onClick}
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
