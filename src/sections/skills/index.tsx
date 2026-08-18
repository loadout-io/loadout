/* Ekran sekcji Skills: nagłówek, jedna ścieżka dodawania i lista umiejętności, każda ze swoim
 * stanem rozmieszczenia.
 *
 * CIENKI Z ZAŁOŻENIA. Karta przeglądu (`review-card.tsx`, T-19) jest wylądowana i to ona
 * pokazuje wciągniętą umiejętność: ciało, znaleziska i przycisk dodania. Drugiej karty ani
 * drugiego przepływu wciągania tu nie ma (niezmiennik 23) — między komponentem a sekcją
 * brakowało nagłówka, POLA NA LINK i listy, i tylko to jest w tym pliku.
 *
 * DLACZEGO UMIEJĘTNOŚĆ CZEKAJĄCA JEST WIERSZEM LISTY, A NIE OSOBNYM PANELEM. Makieta rysuje
 * ją dwa razy — raz w panelu „Add a skill", raz jako kafelek z chipem `needs a check`
 * (`docs/mockup/index.html:716-735`) — a to jest jeden fakt w dwóch miejscach (niezmiennik 13).
 * Tutaj jest jedno miejsce: wiersz tej umiejętności, a karta przeglądu siedzi w nim.
 *
 * ZNACZNIK ROZMIESZCZENIA JEST LICZONY, NIE WPISANY. Umiejętność leży w katalogach vendorów
 * (`.claude/skills/` i `.agents/skills/`, ARCHITECTURE §9) albo jeszcze nie leży nigdzie,
 * bo czeka na człowieka — i to jest jedyna różnica rozmieszczenia, jaką ten magazyn dziś
 * niesie. `InstalledSkill` w `src/state/skills.ts` ma dokładnie dwa pola, `name`
 * i `fromTheInternet`, i ani jednego o vendorach, więc dwie POZYCJE `installed` nie mają dziś
 * jak różnić się rozmieszczeniem: takiego stanu nie da się nawet zasiać. Pełne odczytanie
 * („dla ilu z sześciu") wymaga pola per vendor od T-18, czyli zapisu w cudzym pliku
 * (AGENTS.md §7) — zgłoszone człowiekowi. Kiedy to pole wyląduje, zmienia się `readyFor`
 * i ani jedna linia niżej.
 *
 * O migawce serwerowej zustanda i o tym, dlaczego magazyn czyta się tu przez
 * `useSyncExternalStore`, przeczytaj w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement } from 'react';
import { useEffect, useState, useSyncExternalStore } from 'react';
import { useSkills } from '../../state/skills';
import { ReviewCard } from './review-card';

/** Magazyn umiejętności. Jest singletonem — `src/state/skills.ts` nie ma fabryki. */
export type SkillsStore = typeof useSkills;

export interface SkillsScreenProps {
  /** Bez propsu ekran bierze swój prawdziwy magazyn, z propsem ten z testu. */
  store?: SkillsStore;
}

/* Klasy komponentów z DESIGN §6. */
const PRIMARY = 'h-9 rounded-sq bg-accent px-4 text-ui text-bg';
const SECONDARY = 'h-8 rounded-sq border border-line-strong bg-raised px-3 text-ui text-ink';
const FIELD = 'h-8 rounded-sq border border-line-strong bg-well px-2 font-mono text-mono text-ink';
/* `chip`: nasycony wariant znaczy „czeka na ciebie" (DESIGN §3 — `--attend` odpowiada na
 * pytanie „co czeka na moją decyzję"), neutralny znaczy „nic od ciebie nie chce". */
const CHIP_WAITING =
  'h-5 rounded-sq border border-attend-edge bg-attend-wash px-2 text-label text-attend';
const CHIP_QUIET = 'h-5 rounded-sq border border-line bg-raised px-2 text-label text-muted';

/**
 * Vendorzy, w których katalogi ląduje umiejętność [ARCHITECTURE §9]. Jedna tablica, bo
 * znacznik ma LICZYĆ vendorów, a nie powtarzać zdanie wpisane ręcznie w dwóch miejscach.
 */
const VENDORS = ['Claude', 'Codex'] as const;

/**
 * Dla kogo ta umiejętność jest gotowa.
 *
 * `installed` znaczy „leży już w obu katalogach" — zapis idzie do obu naraz i nie ma dziś
 * stanu pośredniego (`src/state/skills.ts`, `install`). Umiejętność, która czeka na człowieka,
 * nie leży jeszcze nigdzie. To jest CAŁE miejsce, w którym mieszka odpowiedź na pytanie
 * „dla ilu vendorów": pole per vendor od T-18 zmienia tę funkcję i nic poza nią.
 */
function readyFor(placed: boolean): readonly string[] {
  return placed ? VENDORS : [];
}

/** Znacznik, który czyta człowiek. Liczba vendorów jest w nim wypowiedziana, nie policzona okiem. */
function readySays(ready: readonly string[]): string {
  return ready.length === 0 ? 'Ready for nobody yet' : 'Ready for ' + ready.join(' and ');
}

export default function SkillsScreen({ store = useSkills }: SkillsScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);

  /* ODCZYT PRZY WEJŚCIU W SEKCJĘ — bez tego cała ścieżka odczytu jest martwa.
   *
   * Magazyn dostał `load()` w T-38 AC-6 i do 2026-08-18 NIE MIAŁ ANI JEDNEGO WOŁAJĄCEGO:
   * komenda po stronie Rusta istniała, krawędź `io.ts` istniała, magazyn umiał się wypełnić —
   * i ekran nigdy o nic nie pytał. To jest ta sama rodzina, co płótno przed T-26 i `wireChannel`
   * przed T-38: mechanizm wylądował, ma testy, nikt go nie zawołał. Objaw dla człowieka jest
   * dokładnie taki, jak przy braku funkcji: otwierasz sekcję i nie ma w niej tego, co leży
   * na dysku (niezmiennik 4 — pliki są prawdą).
   *
   * `void`, bo odmowa jest już obsłużona w magazynie i ląduje w jego stanie jako zdanie dla
   * człowieka; drugie `catch` tutaj byłoby drugim miejscem, w którym mieszka ta sama decyzja.
   * Pusta tablica zależności: sekcja pyta RAZ na zamontowanie, a nie na każdy render. */
  useEffect(() => {
    void store.getState().load();
  }, [store]);
  /* Adres wklejany przez człowieka. `null` znaczy, że pole jest zamknięte — jedno miejsce
   * na to pytanie (niezmiennik 13), a nie osobna flaga „czy otwarte" obok wartości, która
   * potrafi się z nią rozjechać. */
  const [link, setLink] = useState<string | null>(null);

  const empty = state.installed.length === 0 && state.pending === null;

  const openLink = (): void => {
    setLink((typed) => typed ?? '');
  };

  return (
    <section className="flex h-full flex-col">
      <header className="flex h-13 items-center gap-3 border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Skills</h1>

        {/* Licznik i przycisk w nagłówku żyją tylko wtedy, gdy jest co liczyć — przy zerze
            mówi to samo zaproszenie niżej (niezmiennik 13). Liczymy WYŁĄCZNIE to, co leży
            w katalogach: umiejętność czekająca na przeczytanie nie jest jeszcze zapisana. */}
        {empty ? null : (
          <>
            <span className="font-mono text-mono text-muted">{`${String(state.installed.length)} saved`}</span>
            <button data-create type="button" className={`ml-auto ${PRIMARY}`} onClick={openLink}>
              ＋ Add a skill
            </button>
          </>
        )}
      </header>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {link === null ? null : (
          <form
            className="mx-auto mb-6 flex max-w-160 flex-col gap-2 rounded-sq border border-line bg-panel p-4"
            onSubmit={(event) => {
              event.preventDefault();
              /* Cała droga bajtów — polityka adresu, limity, skan — mieszka po stronie Rusta
               * za `useSkills.review`. Ekran wie tylko, że człowiek coś wkleił. */
              void store.getState().review(link);
              setLink(null);
            }}
          >
            <label htmlFor="skill-link" className="text-label text-muted">
              Link
            </label>
            <input
              id="skill-link"
              className={FIELD}
              value={link}
              onChange={(event) => {
                setLink(event.target.value);
              }}
            />
            <div className="flex items-center gap-2">
              <button type="submit" className={SECONDARY}>
                Read it
              </button>
              <button
                type="button"
                className={SECONDARY}
                onClick={() => {
                  setLink(null);
                }}
              >
                Cancel
              </button>
            </div>
          </form>
        )}

        {/* Zdanie od magazynu: odmowa instalacji albo link, którego nie dało się przeczytać.
            Bez tego jedyną odpowiedzią na kliknięcie jest cisza, a człowiek klika drugi raz. */}
        {state.message === null ? null : (
          <p className="mx-auto mb-6 max-w-160 text-body text-attend">{state.message}</p>
        )}

        {empty ? (
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <span className="flex size-8 items-center justify-center rounded-sq border border-dashed border-line-strong text-muted">
              ◇
            </span>
            {/* `data-empty` na elemencie z samym zdaniem — tak samo jak w `src/App.tsx`. */}
            <p data-empty className="text-ink">
              No skills yet.
            </p>
            <p className="text-muted">Paste a link, or write one yourself.</p>
            <button data-create type="button" className={PRIMARY} onClick={openLink}>
              ＋ Add a skill
            </button>
          </div>
        ) : (
          <ul className="mx-auto flex max-w-160 flex-col gap-3">
            {/* Czekająca stoi PIERWSZA: jest jedyną rzeczą w tej sekcji, która czegoś od
                człowieka chce, a rzecz wymagająca decyzji nie ma leżeć pod listą gotowych. */}
            {state.pending === null ? null : (
              <li
                data-skill={state.pending.name}
                className="flex flex-col gap-3 rounded-sq border border-attend-edge bg-panel p-3"
              >
                <span data-ready className={CHIP_WAITING}>
                  {readySays(readyFor(false))}
                </span>
                {/* Nazwy nie piszemy drugi raz — niesie ją nagłówek karty (niezmiennik 13). */}
                <ReviewCard
                  item={state.pending}
                  acknowledged={state.acknowledged}
                  onAcknowledge={(findingId) => {
                    store.getState().acknowledge(findingId);
                  }}
                  onAdd={() => {
                    void store.getState().add();
                  }}
                />
              </li>
            )}

            {state.installed.map((skill) => (
              <li
                key={skill.name}
                data-skill={skill.name}
                className="flex items-center gap-2 rounded-sq border border-line bg-panel p-3"
              >
                <h2 className="text-heading text-ink">{skill.name}</h2>
                <span data-ready className={CHIP_QUIET}>
                  {readySays(readyFor(true))}
                </span>
                {/* Znacznik pochodzenia jest TRWAŁY i przeżywa instalację [T5 §5.4]: gasnący
                    po zapisie mówiłby o umiejętności z sieci to samo, co o napisanej ręcznie. */}
                {skill.fromTheInternet ? (
                  <span className={`ml-auto ${CHIP_QUIET}`}>From the internet</span>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}
