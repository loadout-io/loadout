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
 * ZNACZNIKA ROZMIESZCZENIA TU NIE MA I TO JEST NAPRAWA, NIE BRAK (zmierzone 2026-08-18).
 * Do tego dnia każdy wiersz zainstalowanej umiejętności nosił napis „Ready for Claude and
 * Codex", policzony przez `readyFor(placed)` z argumentem wpisanym na sztywno jako `true`.
 * Na dysku właściciela to było NIEPRAWDĄ dla wszystkich dziesięciu umiejętności: `notatki`
 * i `spotkanie` leżą tylko w `~/.claude/skills`, osiem `superset-*` tylko w
 * `~/.agents/skills`, żadna w obu. Kłamał nie napis, a jego źródło: `InstalledWire`
 * (`src-tauri/src/commands/skills.rs`) niesie WYŁĄCZNIE `name` i `fromTheInternet`, a
 * `list_skills_inner` zwija oba katalogi vendorów do jednego `BTreeSet` nazw — informacja
 * o tym, KTÓRY katalog trzymał plik, ginie po drugiej stronie granicy i nie ma jak tu
 * dojechać. Znacznik policzony z danych, których nie ma, jest zmyśloną relacją
 * (niezmiennik 17), więc nie ma go wcale. Wraca w tym samym commicie, w którym `InstalledWire`
 * dostaje pole per katalog — zgłoszone człowiekowi.
 *
 * O migawce serwerowej zustanda i o tym, dlaczego magazyn czyta się tu przez
 * `useSyncExternalStore`, przeczytaj w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement } from 'react';
import { useEffect, useSyncExternalStore } from 'react';
import { useSkills } from '../../state/skills';
import { ReviewCard } from './review-card';

/** Magazyn umiejętności. Jest singletonem — `src/state/skills.ts` nie ma fabryki. */
export type SkillsStore = typeof useSkills;

export interface SkillsScreenProps {
  /** Bez propsu ekran bierze swój prawdziwy magazyn, z propsem ten z testu. */
  store?: SkillsStore;
}

/* Klasy komponentów z DESIGN §6. */
const PRIMARY = 'h-9 rounded-sm bg-accent px-4 text-ui text-bg';
const SECONDARY = 'h-8 rounded-sm border border-line-strong bg-raised px-3 text-ui text-ink';
/* POLE BIERZE KLASE DOMU, NIE WLASNY OPIS.
 *
 * `theme.css` ma klase `.field` od pierwszego dnia: studnia, mocny obrys, promien z pasma, kroj
 * maszynowy i `user-select: text` — to ostatnie jest czescia pola, nie ozdoba, bo `body` wylacza
 * zaznaczanie w calej aplikacji. Do 2026-08-19 wolaly ja DWA miejsca, a cztery sekcje przepisywaly
 * ten sam wyglad recznie w dwunastu stalych — i rozjechaly sie: tu obrys byl `--line`, w Skills
 * `--line-strong`. Jeden fakt, jedno miejsce (niezmiennik 13); dwa opisy tego samego pola czyta
 * sie jak dwa rozne stany, a nie jak dwa pola.
 *
 * Skupienia tu nie ma z tego samego powodu. `theme.css` daje `.field:focus` obwodke w akcencie
 * i globalny `:focus-visible` obrys — jedna regula na cala aplikacje. Dopisanie tego samego
 * narzedziem na kazdym polu byloby trzecia kopia decyzji, ktora juz jest podjeta. */
const FIELD = 'field';
/* Pole na ZDANIE, nie na adres. `FIELD` wyżej jest monospace z powodu: trzyma URL-a, a w adresie
   liczy się każdy znak z osobna. Odpowiedź na „kiedy tego użyć" jest prozą i w monospace czyta
   się jak dane do sprawdzenia, a nie jak zdanie do napisania. */
const ANSWER = 'field';
/* „Co zrobić" jest ciałem `SKILL.md`, więc bywa akapitem — pole jednowierszowe pokazywałoby
   z niego okno o szerokości ośmiu słów. Wysokość z `.fld textarea` w makiecie. */
const ANSWER_LONG = 'field';
const LABEL = 'text-label text-muted';
const ROW = 'flex flex-col gap-1';
/* `button-danger` z DESIGN §6: jak `button-secondary`, ale obrys `--fail-edge` i tekst
 * `--fail`, BEZ WYPEŁNIENIA — akcja niszcząca ma być rozpoznawalna, a nie najgłośniejsza. */
const DANGER = 'h-8 rounded-sm border border-fail-edge px-3 text-ui text-fail';
/* `chip`: neutralny wariant znaczy „nic od ciebie nie chce" (DESIGN §3 i §6). */
const CHIP_QUIET = 'h-5 rounded-pill border border-line bg-raised px-2 text-label text-muted';

/**
 * Gdzie to wyląduje — zdanie czytane PRZED naciśnięciem „Add this skill", nie po.
 *
 * Ta sekcja jest jedynym miejscem w Loadoucie, które pisze poza własną bibliotekę: cel to
 * katalogi, do których zaglądają narzędzia agentowe człowieka (`DESTINATION_DIRS`
 * w `src-tauri/src/skills/mod.rs`). Umiejętność dodana tutaj wchodzi więc do każdego
 * następnego uruchomienia tych narzędzi, także poza Loadoutem — i człowiek ma o tym wiedzieć
 * z ekranu, a nie z dokumentacji. Nazwy katalogów w tym zdaniu nie padają z rozmysłem: liczy
 * je Rust i to jest jedyne miejsce, w którym stoją (niezmiennik 13).
 */
export const WHERE_IT_LANDS =
  'This goes into the folders your agent apps read on this machine, so every later run can ' +
  'use it. Remove takes it back out.';

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
    /* DRUGI ODCZYT, DRUGIE PYTANIE. „Co leży w katalogach agentów" i „kogo mam zapisanych" to
     * dwa różne fakty i dwie różne komendy; bez tego wiersza wybór w trzecim wejściu byłby
     * pusty na każdej maszynie, czyli kontrolką, której nie da się użyć (niezmiennik 16).
     * Osobna akcja, a nie jedno wywołanie w `load()`: liczbę pytań tamtej ścieżki zamraża
     * `src/sections/read-paths-populate.test.ts`. */
    void store.getState().loadAgents();
  }, [store]);
  /* Co człowiek wpisał w panelu dodawania — adres ALBO trzy odpowiedzi. `null` znaczy, że
   * panel jest zamknięty: jedno miejsce na to pytanie (niezmiennik 13), a nie osobna flaga
   * „czy otwarty" obok treści, która potrafi się z nią rozjechać. */
  const empty = state.installed.length === 0 && state.pending === null;
  const panel = state.adding;

  /* Jedna funkcja na oba przyciski `data-create` — ten w nagłówku i ten na pustym ekranie.
   * Nigdy nie stoją w dokumencie naraz i otwierają ten sam panel. */
  const openPanel = (): void => {
    store.getState().openAdd();
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
            <button data-create type="button" className={`ml-auto ${PRIMARY}`} onClick={openPanel}>
              ＋ Add a skill
            </button>
          </>
        )}
      </header>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {/* DWA WEJŚCIA, JEDEN PANEL, JEDEN PRZYCISK, KTÓRY GO OTWIERA.
            Adres i umiejętność napisana tutaj to jedna decyzja z dwiema odpowiedziami, a nie
            dwie decyzje — drugie zaproszenie obok pierwszego byłoby dwiema odpowiedziami na
            pytanie „jak dodać umiejętność" (niezmiennik 13).

            2026-08-19 — DO TEGO DNIA PANEL PRZYJMOWAŁ WYŁĄCZNIE ADRES, dokładnie tyle, ile
            umiało `review_skill(url)` po drugiej stronie granicy. Pusty ekran obiecywał przy
            tym „Paste a link, or write one yourself", więc obietnica stała bez kontrolki —
            ten sam defekt, co kontrolka bez skutku, tylko odwrócony (niezmiennik 16), i
            droższy, bo człowiek szuka przycisku, którego nie ma, zamiast zgłosić jego brak.

            DWA `<form>`, NIE JEDEN. Enter w polu wysyła formularz, w którym to pole stoi —
            przy jednym formularzu Enter wpisany w nazwę odpalałby czytanie PUSTEGO adresu.
            Panel jest za to jeden i to on nosi `data-add-panel`.

            Treść panelu mieszka w magazynie (`state.adding`), nie w `useState` ekranu, i to
            nie jest ustępstwo na rzecz testu: odmowa z Rusta ma zostawić wpisany akapit na
            ekranie, więc pola muszą leżeć tam, gdzie ląduje odmowa (niezmiennik 13). */}
        {panel === null ? null : (
          <div
            data-add-panel
            className="mx-auto mb-6 flex max-w-160 flex-col gap-4 rounded-md border border-line bg-panel p-4"
          >
            <form
              className={ROW}
              onSubmit={(event) => {
                event.preventDefault();
                void store.getState().review(panel.link);
                store.getState().closeAdd();
              }}
            >
              <label htmlFor="skill-link" className={LABEL}>
                Link
              </label>
              <input
                id="skill-link"
                className={FIELD}
                value={panel.link}
                onChange={(event) => {
                  store.getState().typeInto({ link: event.target.value });
                }}
              />
              <button type="submit" className={`mt-1 mr-auto ${SECONDARY}`}>
                Read it
              </button>
            </form>

            {/* TRZY PYTANIA I DOKŁADNIE TRZY [T5 §8.3]: jak się nazywa, kiedy tego użyć, co
                zrobić. Czwartym w badaniu jest zakres („ten projekt / wszędzie") i tu go NIE
                MA z rozmysłu — zakres zostaje globalny, dokładnie jak na drodze adresu, a
                wybór jest osobnym zadaniem (T-44). Pytanie postawione bez skutku byłoby tą
                samą obietnicą bez kontrolki, którą ten commit zdejmuje.

                ETYKIETY SĄ PYTANIAMI, a nie nazwami pól `SKILL.md`. „When should the agent
                use it?" jest tym, co w pliku nazywa się `description`, i pytanie zadane wprost
                jest jedynym powodem, dla którego człowiek pisze tam prawdziwy warunek zamiast
                drugiego tytułu — a to jest pole, po którym model decyduje, czy w ogóle sięgnąć
                (T5 §8.3). Nazwa pola z pliku nie pada tu ani razu (niezmiennik 14).

                SLUGA TU NIE LICZYMY. „Review pull requests" zamienia się w katalog
                `review-pull-requests` po tamtej stronie granicy i tylko tam (`slug_of`).
                Policzony drugi raz tutaj rozjechałby się z tamtym na pierwszym znaku spoza
                ASCII, a rozjazd widać dopiero jako katalog o innej nazwie niż zdanie, które
                człowiek przeczytał (niezmiennik 13). Człowiek widzi go raz — w nagłówku karty
                przeglądu, która wraca z Rusta z policzoną nazwą. */}
            <form
              className="flex flex-col gap-2 border-t border-line pt-4"
              onSubmit={(event) => {
                event.preventDefault();
                /* Bez `closeAdd()`: panel zamyka sam magazyn i TYLKO po udanym zapisie.
                   Zamknięty tutaj, bezwarunkowo, zabierałby ze sobą trzy odpowiedzi za każdym
                   razem, gdy Rust odmówi — a wtedy człowiek czyta jedno zdanie o nazwie
                   i pisze akapit drugi raz. */
                void store.getState().writeItHere();
              }}
            >
              <div className={ROW}>
                <label htmlFor="skill-name" className={LABEL}>
                  What should it be called?
                </label>
                <input
                  id="skill-name"
                  data-question="name"
                  className={ANSWER}
                  value={panel.name}
                  onChange={(event) => {
                    store.getState().typeInto({ name: event.target.value });
                  }}
                />
              </div>

              <div className={ROW}>
                <label htmlFor="skill-when-to-use" className={LABEL}>
                  When should the agent use it?
                </label>
                <input
                  id="skill-when-to-use"
                  data-question="whenToUse"
                  className={ANSWER}
                  value={panel.whenToUse}
                  onChange={(event) => {
                    store.getState().typeInto({ whenToUse: event.target.value });
                  }}
                />
              </div>

              <div className={ROW}>
                <label htmlFor="skill-what-to-do" className={LABEL}>
                  What should it do?
                </label>
                <textarea
                  id="skill-what-to-do"
                  data-question="whatToDo"
                  className={ANSWER_LONG}
                  value={panel.whatToDo}
                  onChange={(event) => {
                    store.getState().typeInto({ whatToDo: event.target.value });
                  }}
                />
              </div>

              <button type="submit" data-write-it-yourself className={`mt-1 mr-auto ${SECONDARY}`}>
                Save this skill
              </button>
            </form>

            {/* TRZECIE WEJŚCIE: jedno zdanie człowieka i wybór tego, kto ma to napisać.
                W TYM SAMYM panelu, co adres i formularz — „dodaj umiejętność" jest jedną
                decyzją z trzema odpowiedziami, a nie trzema decyzjami (niezmiennik 13).

                2026-08-19 — do tego dnia OBIE istniejące drogi wymagały, żeby człowiek napisał
                treść sam: adres przyjmuje gotowy plik, formularz gotowe trzy odpowiedzi. Loadout
                miał przy tym dwa sterowniki agentów, żywy nadzór i dowód śmierci grupy — i ani
                jednej drogi, która zamienia zdanie człowieka w tekst od modelu.

                DRAFT NIE JEST ZAPISEM. Trzy pola lądują w formularzu wyżej, edytowalne, i to
                człowiek oddaje je dalej. Tekst poprawiony po drafcie przechodzi przez ten sam
                skan, co wpisany od zera (niezmiennik 23) — a tekst przeskanowany przed poprawką
                jest tekstem, którego nikt nie przeskanował.

                NAZWY VENDORÓW TU NIE MA I MIEĆ NIE MOŻE. Pozycje wyboru pochodzą z magazynu,
                czyli z dysku; informacja o tym, którym narzędziem biegnie agent, mieszka
                w jego zapisanej definicji po tamtej stronie granicy (`runsWith`), a każde
                zdanie o vendorze w tej sekcji byłoby zdaniem o czymś, czego nikt tu nie wie
                (`mounted.test.tsx` zamraża brak tych nazw w tym markupie). */}
            <form
              className="flex flex-col gap-2 border-t border-line pt-4"
              onSubmit={(event) => {
                event.preventDefault();
                void store.getState().askAnAgent();
              }}
            >
              <div className={ROW}>
                <label htmlFor="skill-what-you-want" className={LABEL}>
                  Or say what you want, and an agent writes it
                </label>
                <input
                  id="skill-what-you-want"
                  data-what-you-want
                  className={ANSWER}
                  value={state.want}
                  onChange={(event) => {
                    store.getState().sayWhatYouWant(event.target.value);
                  }}
                />
              </div>

              <div className={ROW}>
                <label htmlFor="skill-who-writes-it" className={LABEL}>
                  Who should write it?
                </label>
                {/* Pozycją jest `id`, a widać nazwę: nazwa jest jedyną częścią zapisanego
                    agenta, którą człowiek rozpoznaje, a `id` jedyną, która przeżywa zmianę
                    nazwy (T4 §5.1) — i to ona jedzie do Rusta. */}
                <select
                  id="skill-who-writes-it"
                  data-pick-an-agent
                  className={ANSWER}
                  value={state.chosenAgent}
                  onChange={(event) => {
                    store.getState().chooseAgent(event.target.value);
                  }}
                >
                  {state.agents.map((agent) => (
                    <option key={agent.id} data-agent={agent.id} value={agent.id}>
                      {agent.name}
                    </option>
                  ))}
                </select>
              </div>

              {/* PODMIANA KONTROLKI, dokładnie ta sama, którą robią Start i Stop w sekcji Praca
                  (`run/start.tsx`). „Napisz mi to" zostawione obok stanu „pisze" jest drugą turą
                  za drugie naciśnięcie, przy pierwszej, która dalej biegnie i dalej kosztuje.
                  Animacji nie ma żadnej: jedyna w aplikacji to kropka żywej karty (DESIGN §7). */}
              {state.writing ? (
                <>
                  <p data-writing className="text-body text-muted">
                    An agent is writing this skill now.
                  </p>
                  <button
                    type="button"
                    data-stop-writing
                    className={`mt-1 mr-auto ${DANGER}`}
                    onClick={() => {
                      void store.getState().stopWriting();
                    }}
                  >
                    Stop
                  </button>
                </>
              ) : (
                <button type="submit" data-ask-an-agent className={`mt-1 mr-auto ${SECONDARY}`}>
                  Write it for me
                </button>
              )}
            </form>

            {/* Wyjście z panelu jest jedno, bo panel jest jeden. Po jednym „Cancel" na wejście
                człowiek musiałby wiedzieć, które z trzech właśnie zamyka.

                W stanie „pisze" go NIE MA, i to jest ta sama podmiana, co wyżej: panel zamknięty
                w trakcie pisania zabiera ze sobą jedyną kontrolkę, która umie tego agenta
                zatrzymać, a agent pisze dalej i dalej kosztuje (niezmienniki 6 i 16). */}
            {state.writing ? null : (
              <button
                type="button"
                className={`mr-auto ${SECONDARY}`}
                onClick={() => {
                  store.getState().closeAdd();
                }}
              >
                Cancel
              </button>
            )}
          </div>
        )}

        {/* Zdanie od magazynu: odmowa instalacji albo link, którego nie dało się przeczytać.
            Bez tego jedyną odpowiedzią na kliknięcie jest cisza, a człowiek klika drugi raz. */}
        {state.message === null ? null : (
          <p className="mx-auto mb-6 max-w-160 text-body text-attend">{state.message}</p>
        )}

        {empty ? (
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <span className="flex size-8 items-center justify-center rounded-md border border-dashed border-line-strong text-muted">
              ◇
            </span>
            {/* `data-empty` na elemencie z samym zdaniem — tak samo jak w `src/App.tsx`. */}
            <p data-empty className="text-ink">
              No skills yet.
            </p>
            <p className="text-muted">Paste a link, or write one yourself.</p>
            <button data-create type="button" className={PRIMARY} onClick={openPanel}>
              ＋ Add a skill
            </button>
          </div>
        ) : (
          <>
            {/* Czekająca stoi PIERWSZA i na całej szerokości: jest jedyną rzeczą w tej sekcji,
                która czegoś od człowieka chce, a rzecz wymagająca decyzji nie ma leżeć pod
                listą gotowych ani w kolumnie obok nich. */}
            {state.pending === null ? null : (
              <section
                data-skill={state.pending.name}
                className="mx-auto mb-6 flex max-w-160 flex-col gap-3 rounded-md border border-attend-edge bg-panel p-3"
              >
                {/* Zdanie o miejscu stoi TU, a nie w polu na link: pole zamyka się w chwili
                    wklejenia, a decyzja „dodać czy nie" jest podejmowana dopiero nad tą kartą.
                    Ostrzeżenie widoczne wcześniej niż decyzja nie jest ostrzeżeniem. */}
                <p className="text-body text-muted">{WHERE_IT_LANDS}</p>
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
              </section>
            )}

            {/* Dwie kolumny, jak w makiecie (`docs/mockup/index.html`, `.grid.two`).
                Opisu w kafelku NIE MA, bo `InstalledWire` nie niesie ani `summary`, ani
                `description` — zdanie dopisane tutaj byłoby zmyślone (niezmiennik 17).
                Zgłoszone człowiekowi razem z polem per katalog. */}
            {state.installed.length === 0 ? null : (
              <ul className="mx-auto grid max-w-160 grid-cols-2 gap-3">
                {state.installed.map((skill) => (
                  <li
                    key={skill.name}
                    data-skill={skill.name}
                    className="flex flex-col gap-3 rounded-md border border-line bg-panel p-3"
                  >
                    <div className="flex items-center gap-2">
                      <h2 className="text-heading text-ink">{skill.name}</h2>
                      {/* Znacznik pochodzenia jest TRWAŁY i przeżywa instalację [T5 §5.4]:
                          gasnący po zapisie mówiłby o umiejętności z sieci to samo, co
                          o napisanej ręcznie. */}
                      {skill.fromTheInternet ? (
                        <span className={`ml-auto ${CHIP_QUIET}`}>From the internet</span>
                      ) : null}
                    </div>
                    {/* Jedyna droga powrotna z katalogów narzędzi agentowych. Magazyn po
                        udanym usunięciu czyta katalogi JESZCZE RAZ, więc wiersz znika dopiero
                        wtedy, gdy pliku naprawdę już tam nie ma (`src/state/skills.ts`). */}
                    <button
                      type="button"
                      data-remove={skill.name}
                      className={`mr-auto ${DANGER}`}
                      onClick={() => {
                        void store.getState().remove(skill.name);
                      }}
                    >
                      Remove
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </>
        )}
      </div>
    </section>
  );
}
