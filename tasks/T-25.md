# T-25 — Powłoka montuje sekcje: koniec z pięcioma pustymi ekranami

To zadanie istnieje, bo bez niego cała reszta przechodzi bramkę i nie działa.

`src/App.tsx` (T-01) renderuje `<EmptyState>` dla każdej z pięciu sekcji. Dwadzieścia parę zadań
buduje komponenty, których **nic nie renderuje**: `src/ui/sections.tsx` wylicza sekcje z etykietą
i zdaniem pustego ekranu, ale nie ma pola na komponent, a nagłówek tego pliku opisuje przekazanie
własności, dla którego nie ma mechanizmu — „T-08, T-09, T-11, T-13, T-14, T-17 i T-19 dopisują tu
po jednej linii, mimo że go nie posiadają". Żadne z tych zadań nie ma `src/ui` ani `App.tsx`
w swoim bloku OWNS, `checks/quick-scope.sh` odrzuci zapis, a linii do dopisania i tak nie ma.

Cicha porażka jest tu wyjątkowo paskudna, bo **nie jest czerwona**. Kryteria sekcji to testy
komponentowe wołane wprost na plikach; przechodzą bez montażu. Bramka zaświeci na zielono na
wszystkim, a okno pokaże pięć pustych ekranów — i dowiemy się o tym dopiero wtedy, gdy ktoś
uruchomi aplikację.

Rozstrzygnięcie jest konwencją, nie rejestrem: **powłoka szuka `src/sections/<id>/index.tsx`.**
Każde zadanie sekcji tworzy własny `index.tsx` **wewnątrz poddrzewa, które już posiada** — zero
plików dzielonych, zero wpisów do cudzego OWNS, zero konfliktów przy landowaniu. Alternatywa
(pole `component` w rejestrze) robi z `src/ui/sections.tsx` drugi wspólny kręgosłup obok
`lib.rs`, z tą samą klasą kolizji — a front, w odróżnieniu od Rusta, niczego takiego nie wymaga.

**Read first:** `src/ui/sections.tsx` w całości (rejestr pięciu sekcji, `Section`, `sectionEntry` —
to jest kontrakt, którego **nie zmieniasz**), `src/App.tsx` i `src/ui/shell/sections.test.tsx`
(kryterium 3 z T-01: dokładnie jedna sekcja w drzewie, pozostałe cztery **nieobecne** — nie
schowane), `docs/design/DESIGN.md` §6 `empty-state` (pusty ekran to zaproszenie do działania, nie
komunikat o braku danych), `docs/ARCHITECTURE.md` §3 (kolejność i liczba sekcji są częścią
kontraktu), `docs/HARNESS-QUEUE.md` Q-5 (skąd wzięła się ta decyzja i co odrzucono).

## Kto to robi

- **Agent:** `react-ui`
- **Druga opinia:** Codex — pisze Claude Code (D3). Recenzent nie zatwierdza i nie blokuje;
  niedostępny recenzent to `exit 0` z notatką.
- **Artefakty biegu:** `runs/T-25/`

**Czym testujemy.** W repo **nie ma** `jsdom` ani `@testing-library/react`, a `package.json`
i `vite.config.ts` są na liście `DENIED` w `checks/quick-scope.sh`. `vitest` biegnie w środowisku
`node`, więc to, co widzi użytkownik, sprawdzamy przez `renderToStaticMarkup` z `react-dom/server`,
a logikę — czystymi funkcjami wołanymi wprost. Powłoka jest **sterowana**: mapa ekranów wchodzi
propsem, więc żadne kryterium nie potrzebuje istniejącego pliku sekcji.

## Co to zadanie posiada

- `src/ui/screens.ts` — cała mechanika odkrywania. Dwie rzeczy, celowo rozdzielone:
  - `screensFrom(modules: Record<string, unknown>): ScreenMap` — **czysta** funkcja mapująca
    surowy wynik globa na mapę `id → komponent`. Wszystko, co da się pomylić, jest tutaj i da
    się to przetestować bez dotykania dysku.
  - `discoverScreens(): ScreenMap` — `screensFrom(import.meta.glob('../sections/*/index.tsx',
    { eager: true }))`. Jedna linia, żaden warunek.
- `src/App.tsx` — przyjmuje `screens?: ScreenMap`, domyślnie stałą modułową policzoną **raz**
  przez `discoverScreens()`. Renderuje ekran, gdy jest; `EmptyState` ze zdaniem z rejestru, gdy
  go nie ma. Nadal **jeden** `<main>` i **jeden** wpis — bez pętli po `SECTIONS`, bez `hidden`,
  bez `display:none`.
- `src/ui/shell/sections.test.tsx` — istniejący test T-01. Masz go w OWNS **wyłącznie** na
  wypadek, gdyby nowy props wymagał w nim korekty; jego asercji nie wolno osłabić (§7).
- Pliki testowe wymienione przy `check:`.

Trzy rozstrzygnięcia, żeby implementacja nie zgadywała:

1. **Rejestr zostaje bez zmian.** `src/ui/sections.tsx` nie dostaje pola `component` i nie jest
   w OWNS tego zadania. Powiązanie id → komponent wynika ze **ścieżki pliku**, nie z wpisu.
2. **Nieznany katalog jest pomijany, nie jest błędem.** `src/sections/quantum/index.tsx` nie ma
   swojego id w `SECTIONS`, więc wypada z mapy w ciszy. Rzucanie wyjątkiem przy odkrywaniu
   zabiera całe okno za cudzy plik.
3. **Moduł bez użytecznego eksportu też jest pomijany**, a sekcja pokazuje pusty ekran. Ekran,
   który się nie renderuje, ma kosztować jedną sekcję, nie całą aplikację (niezmiennik 5 w duchu,
   po stronie frontu).

## Niezmienniki

- **13 — jeden fakt, jedno miejsce.** Zdanie pustego ekranu przychodzi z `sectionEntry(id).empty`.
  Skopiowany literał w `App.tsx` rozjedzie się z rejestrem przy pierwszej zmianie brzmienia.
- **17 — atrapa w powłoce zostaje w niej na zawsze.** Nie renderuj „zaślepki ekranu" dla sekcji,
  która nie ma jeszcze `index.tsx`. Pusty ekran z rejestru jest prawdziwą odpowiedzią, atrapa
  udająca ekran nie jest.
- **20 — test sprawdza zachowanie, nie obecność stringa.** „W dokumencie jest `data-section`"
  przechodzi na dzisiejszej powłoce, która nie montuje niczego.
- **23 — polityka ma jedno ciało.** Wzorzec globa występuje **raz**, w `discoverScreens()`.
  Drugi wzorzec w teście albo w `App.tsx` to drugie miejsce do rozjechania się.
- **14 — zero żargonu.** W tekście widocznym dla użytkownika nie ma słów `mount`, `route`,
  `registry` ani `glob`.

## Kryteria akceptacji

Bramka odrzuca czerwień pochodzącą z braku modułu (`NOT_A_REAL_RED`, AGENTS.md §2a p. 5), więc
najpierw `src/ui/screens.ts` ze stubami rzucającymi `new Error('not implemented')` i dopiero
potem pliki testów — wtedy `before` pada w czasie wykonania, a nie przy rozwiązywaniu importu.

## AC-1 Sekcja z ekranem pokazuje ten ekran, a pozostałe cztery nie są w drzewie
check: npx --no-install vitest run src/ui/shell/screen-mount.test.tsx

`<App section="agents" screens={{ agents: () => <p data-probe="agents-screen">…</p> }} />`
renderuje `data-probe="agents-screen"`. W tym samym dokumencie identyfikatory czterech
pozostałych sekcji występują **zero razy** — nie są schowane, nie ma ich.

*Słaba asercja:* `expect(html).toContain('agents-screen')`. Przechodzi na powłoce, która montuje
wszystkie pięć ekranów i chowa cztery CSS-em — czyli na „always-mounted route stack", przez który
poprzedni prototyp renderował 142 elementy niosące tekst przy suficie 60 [raport 03 §4.1]. Dyskryminuje:
policzenie **pozostałych czterech identyfikatorów do zera** plus asercja, że w wyjściu nie ma
`hidden` ani `display:none`.

## AC-2 Sekcja bez ekranu pokazuje zdanie z rejestru, a nie puste miejsce
check: npx --no-install vitest run src/ui/shell/screen-fallback.test.tsx

`<App section="memory" screens={{}} />` renderuje **dokładnie** `sectionEntry('memory').empty`,
czytane z rejestru w teście, nie wpisane w test literałem. Dla wszystkich pięciu sekcji naraz:
puste `screens` daje pięć różnych zdań, każde równe swojemu wpisowi.

*Słaba asercja:* `expect(html.length).toBeGreaterThan(0)` albo porównanie z wklejonym w test
zdaniem. Pierwsza przechodzi na pustym `<main>`, druga przestaje cokolwiek znaczyć w dniu, w którym
ktoś poprawi brzmienie w rejestrze — a wtedy rozjazd jest właśnie tym, co miało być złapane.
Dyskryminuje: **porównanie z `sectionEntry(id).empty`** w pętli po pięciu identyfikatorach.

## AC-3 Mapowanie ścieżek odrzuca to, czego nie zna, i nie wywraca się na tym
check: npx --no-install vitest run src/ui/screens-from.test.ts

`screensFrom` dostaje ręcznie zbudowany rekord z czterema wpisami: poprawnym
(`../sections/run/index.tsx`), nieznanym identyfikatorem (`../sections/quantum/index.tsx`),
ścieżką, która nie pasuje do wzorca (`../sections/run/rail/panel.tsx`) i modułem **bez**
użytecznego eksportu. Wynik zawiera **wyłącznie** `run`, wywołanie nie rzuca, a kolejność wpisów
w wejściu nie zmienia wyniku.

*Słaba asercja:* `expect(Object.keys(map)).toContain('run')`. Przechodzi w implementacji, która
przepuszcza również `quantum` i moduł bez eksportu — a wtedy pierwszy literówkowy katalog wywraca
okno. Dyskryminuje: **równość zbioru kluczy** z jednoelementowym zbiorem i asercja, że wartość pod
`run` jest wywoływalna.

## AC-4 To, co powłoka odkryła, zgadza się z tym, co naprawdę leży na dysku
check: npx --no-install vitest run src/ui/screens-discovery.test.ts

Test liczy oczekiwany zbiór **niezależnie** — chodzi po `src/sections/` przez `node:fs`, bierze
katalogi zawierające `index.tsx`, przecina z identyfikatorami z `SECTIONS` — i porównuje
z kluczami `discoverScreens()`. Zbiory muszą być równe.

Uczciwie o sile tego kryterium: dziś obie strony są **puste**, bo żadna sekcja nie ma jeszcze
`index.tsx`, więc kryterium przechodzi trywialnie. Czerwone w warstwie `before` jest dlatego, że
`discoverScreens` rzuca `not implemented` — to jest prawdziwa czerwień, nie brak modułu. Wartość
tego kryterium jest **odroczona i automatyczna**: w dniu, w którym pierwsza sekcja doda swój
`index.tsx`, literówka we wzorcu globa rozjedzie oba zbiory i to kryterium ją złapie. Bez niego
zły wzorzec daje na zawsze pustą mapę, czyli **dokładnie ten obraz, który to zadanie usuwa** —
zielono i pusto.

*Słaba asercja:* `expect(typeof discoverScreens()).toBe('object')`. Przechodzi na funkcji
`() => ({})`, czyli na dokładnie tej awarii, o którą chodzi.

## AC-5 Zepsuty ekran kosztuje jedną sekcję, nie całe okno
check: npx --no-install vitest run src/ui/shell/screen-malformed.test.tsx

`screens` z wpisem, którego wartość nie jest funkcją komponentu (`{ skills: 42 as never }`):
`<App section="skills" …>` renderuje pusty ekran ze zdaniem z rejestru i **nie rzuca**. Powłoka
dla pozostałych sekcji zachowuje się bez zmian.

*Słaba asercja:* `expect(() => render()).not.toThrow()`. Przechodzi na implementacji, która łapie
wyjątek i renderuje pustego `<main>` bez zdania — użytkownik widzi wtedy biały prostokąt i nie wie,
czy aplikacja jest zepsuta, czy pusta. Dyskryminuje: **obecność zdania z rejestru** w tym samym
dokumencie.

## Świadomie poza zakresem

- **Tworzenie `index.tsx` dla którejkolwiek sekcji.** To robi zadanie, które tę sekcję buduje,
  we własnym poddrzewie. Tutaj powstaje wyłącznie mechanizm.
- **Dowód end-to-end, że prawdziwy ekran się montuje.** Należy do **pierwszego** zadania sekcji
  w kolejności (T-08): jego kryterium widzi swój `index.tsx` i swoją treść w dokumencie. Tutaj
  nie ma czego zamontować, a atrapa sekcji zostałaby w repo na zawsze (niezmiennik 17).
- **Zmiana `src/ui/sections.tsx`.** Rejestr jest kontraktem i zostaje bez zmian — to jest sedno
  wybranego rozstrzygnięcia, nie przeoczenie.
- **Leniwe ładowanie ekranów (`React.lazy`, dzielenie paczki).** Pięć sekcji desktopowej aplikacji
  to nie jest problem, który dzieli paczkę. Optymalizacja bez zmierzonej skargi.
- **Granice błędów (`ErrorBoundary`).** AC-5 pilnuje modułu, którego nie da się użyć. Wyjątek
  rzucony w środku **renderowania** działającego ekranu to inna rzecz i inne zadanie.
- **Router, adresy URL, historia.** T8 §6.2 rozstrzyga to wprost: sekcja jest `type Section`
  w stanie interfejsu i tak zostaje.

<!-- OWNS
src/App.tsx
src/ui/screens.ts
src/ui/shell/sections.test.tsx
src/ui/shell/screen-mount.test.tsx
src/ui/shell/screen-fallback.test.tsx
src/ui/shell/screen-malformed.test.tsx
src/ui/screens-from.test.ts
src/ui/screens-discovery.test.ts
-->
