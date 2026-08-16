# T-26 — Cztery sekcje dostają ekran: koniec z kartami, które nic nie montują

Aplikacja ma pięć zakładek i **jedna** z nich będzie miała ekran (`run`, z T-08). Pozostałe cztery
pokazują zdanie z rejestru — „Workflows you build will be listed here." — mimo że komponenty tych
sekcji **już wylądowały i są zielone**: lista workflow i płótno (T-13, T-14), formularz agenta
(T-11), karta recenzji umiejętności (T-19), a pamięć dowozi T-17. Nic ich nie montuje, bo nikt
nie napisał `src/sections/<id>/index.tsx`, bo żadne kryterium o to nie poprosiło.

To jest cicha porażka w najczystszej postaci i warto nazwać jej mechanizm, bo powtórzy się przy
każdej następnej sekcji. Testy komponentowe wołają komponent **wprost**, więc przechodzą bez
powłoki. Bramka jest zielona. `scripts/ci.sh` jest zielone. Recenzent czyta diff jednej sekcji
i też nie ma jak tego zobaczyć. Jedynym miejscem, w którym to widać, jest uruchomiona aplikacja —
a jej nie ogląda żaden automat.

Zapis, który to spowodował, stoi w `tasks/T-08.md` przy AC-8 i jest dziś nieprawdą:

> „Pozostałe sekcje dostają to za darmo: ta sama ścieżka, ten sam wzorzec, jeden `index.tsx`
> w poddrzewie, które zadanie już posiada."

Za darmo nie dostały. `workflows` nie ma nawet właściciela swojego poddrzewa: T-13 posiada
`canvas` i `step-panel`, T-14 posiada `list`, a `src/sections/workflows/` nie posiada nikt.

**Read first:**
`docs/mockup/index.html` — ekrany `data-screen="flowlist"`, `"agents"`, `"skills"`, `"memory"`.
To jest umowa o wyglądzie i to z niej bierzesz kształt: nagłówek `<h1>` z podpisem liczbowym,
akcja główna po prawej, pod spodem lista kafelków. ·
`docs/design/DESIGN.md` §5 (przestrzeń i kształt), §6 (`tile`, `chip`, `button-*`, `empty-state`) ·
`docs/ARCHITECTURE.md` §7 (sufit gęstości — ekran listy ma się w nim zmieścić), §9 (umiejętności) ·
`tasks/T-25.md` (konwencja odkrywania: powłoka szuka `src/sections/<id>/index.tsx` globem) ·
`AGENTS.md` niezmienniki 13, 16, 17, 23.

## Kto to robi

- **Agent:** `react-ui`
- **Druga opinia:** inny vendor niż pisarz (D3).
- **Artefakty biegu:** `runs/T-26/`

## Co to zadanie posiada

- Cztery ekrany: `src/sections/{workflows,agents,skills,memory}/index.tsx`.
- Cztery pliki testów wymienione przy `check:`.

Ekran jest **cienki**. Składa to, co już istnieje, i dokłada wyłącznie to, czego brakuje między
komponentem a sekcją: nagłówek z podpisem, akcję główną i listę. Druga implementacja czegokolwiek,
co ma już swój komponent, jest drugim miejscem prawdy (niezmiennik 23) — a te komponenty są
wylądowane i mają własne kryteria.

## Niezmienniki

- **13 — jeden fakt, jedno miejsce.** Zdanie pustego ekranu sekcji przychodzi wyłącznie
  z `sectionEntry(id).empty`. Ekran, który ma własną pustkę (lista bez elementów), pokazuje
  **swoją**, a nie przepisuje tamtej.
- **16 — kontrolka bez handlera nie wchodzi do repo.** Nagłówkowe „＋" z makiety ma robić to,
  co obiecuje, albo nie ma go być. poprzedni prototyp ma trzy martwe przyciski.
- **17 — UI nie rysuje relacji, których nie ma w danych.** Podpis liczbowy („5 saved") pokazuje to,
  co naprawdę jest w magazynie; przy zerze mówi zero, a nie znika.
- **23 — polityka w jednym rdzeniu.** Ekran nie zna Tauri i nie woła IPC sam: bierze store, który
  już istnieje (`createWorkflowStore`, `createAgentsStore`, …), tak jak robi to reszta frontu.

## Kryteria akceptacji

Każde kryterium ma **dwie połowy** i to nie jest ozdoba, tylko konsekwencja stanu repo
(zmierzone 2026-08-16, przed napisaniem tego kontraktu): w całym drzewie Rusta jest **zero**
`#[tauri::command]`, a `src/ipc.ts` nie istnieje — warstwę IPC dopiero buduje T-07. Żaden ekran
nie ma więc dziś skąd wziąć prawdziwych danych i nie będzie miał do czasu, aż powstaną adaptery
sekcji. Kryterium wymagające „dwóch agentów w magazynie" przez prawdziwe odkrywanie byłoby
kryterium niewykonalnym, a niewykonalne kryterium pali bieg i kończy się rundą naprawczą, która
nie ma czego naprawić.

Więc: **(a) montaż** dowodzimy przez prawdziwe odkrywanie, bez wstrzykiwania czegokolwiek —
i to jest ta połowa, dla której to zadanie istnieje. **(b) treść** dowodzimy renderując ekran
wprost, z magazynem zasianym przez atrapę `Io` (magazyny są już tak zbudowane: `createAgentsStore(io)`).
Ekran przyjmuje więc opcjonalny `store`, dokładnie tak jak powłoka przyjmuje opcjonalne `screens`:
bez propsu bierze swój prawdziwy, z propsem ten z testu.

## AC-1 Sekcja Workflows montuje się naprawdę i pokazuje listę, nie zdanie z rejestru
check: npx --no-install vitest run src/sections/workflows/mounted.test.tsx

**(a)** `renderToStaticMarkup(<App section="workflows" />)` — **bez** propsu `screens`, czyli przez
prawdziwe odkrywanie z `src/ui/screens.ts`. W dokumencie jest `No workflows yet.` (zdanie **listy**,
wpisane w teście ręcznie, nie zaimportowane) i dokładnie jeden `data-create`; **nie ma**
`sectionEntry('workflows').empty`.

**Kontrola przeciw pustej asercji:** `renderToStaticMarkup(<App section="workflows" screens={{}} />)`
**musi** zawierać `sectionEntry('workflows').empty`. Bez niej „nie ma tego zdania" przechodzi także
wtedy, gdy powłoka przestała je w ogóle renderować — czyli gdy zepsuto pustkę zamiast zamontować ekran.

**(b)** Ten sam ekran wprost, z magazynem zasianym dwoma workflow: obie nazwy w dokumencie,
`No workflows yet.` **znika**. Bez tej połowy ekran, który zawsze rysuje pustkę, przechodzi (a).

*Słaba asercja:* samo `not.toContain(sectionEntry('workflows').empty)` — przechodzi na `index.tsx`
eksportującym pusty `<div/>`. Druga słaba: asercja o braku `data-empty`, bo lista **też** go używa
przy zerze workflow, więc kryterium mierzyłoby co innego, niż mówi.

## AC-2 Sekcja Agents montuje się i przy zerze dalej zaprasza
check: npx --no-install vitest run src/sections/agents/mounted.test.tsx

**(a)** Jak wyżej, dla `agents`: ekran jest zamontowany przez prawdziwe odkrywanie, w dokumencie
jest własne zdanie pustego ekranu **agentów** (nie zdanie z rejestru) i dokładnie jedna kontrolka
dodawania — pusty stan z makiety zaprasza, a nie tylko informuje. `sectionEntry('agents').empty`
nie występuje. Ta sama kontrola z `screens={{}}`.

**(b)** Ekran wprost, z magazynem zasianym dwoma agentami: obie nazwy w dokumencie, każda ze swoim
vendorem, i **ta sama jedna** kontrolka dodawania, co przy zerze.

*Słaba asercja:* sprawdzenie tylko przypadku z dwoma agentami. Przechodzi na ekranie, który przy
zerze renderuje pustkę bez wyjścia — czyli w jedynym stanie, jaki użytkownik widzi przy pierwszym
uruchomieniu, bo IPC jeszcze nie istnieje. Dyskryminuje: oba przypadki i równa liczba kontrolek.

## AC-3 Sekcja Skills montuje się i pokazuje stan rozmieszczenia, nie same nazwy
check: npx --no-install vitest run src/sections/skills/mounted.test.tsx

**(a)** Jak wyżej, dla `skills`, z tą samą kontrolą.

**(b)** Ekran wprost, z dwoma umiejętnościami o **różnym** stanie rozmieszczenia: obie nazwy
i przy każdej znacznik mówiący, dla ilu vendorów jest rozmieszczona — wyliczony ze stanu, nie
wpisany na sztywno. Asercja, że te dwa znaczniki **różnią się**: umiejętność gotowa dla obu
vendorów nie ma prawa wyglądać jak ta czekająca na sprawdzenie.

*Słaba asercja:* lista samych nazw. Gubi całą różnicę, dla której ta sekcja istnieje
(`docs/ARCHITECTURE.md` §9), a T-18 zbudował silnik rozmieszczania właśnie po to.

## AC-4 Sekcja Memory montuje się i trzyma dwie strefy osobno
check: npx --no-install vitest run src/sections/memory/mounted.test.tsx

**(a)** Jak wyżej, dla `memory`, z tą samą kontrolą.

**(b)** Ekran wprost, z jedną notatką zaproponowaną i jedną używaną: obie w dokumencie, w **osobnych**
strefach, a zaproponowana niesie swój znacznik i swoje dwie akcje. Rozdział stref jest tu całym
kryterium, bo to on jest produktem: notatka zaproponowana nie wchodzi do promptu, dopóki człowiek
jej nie promuje (T-17 AC-1 i AC-2), a ekran wyświetlający obie w jednym worku kasuje jedyną widoczną
różnicę między tym, co agent zaproponował, a tym, co człowiek zatwierdził.

*Słaba asercja:* obie notatki w dokumencie, bez pytania o strefy. Przechodzi na ekranie renderującym
jedną płaską listę — czyli na tym, który tę sekcję unieważnia.

## Świadomie poza zakresem

- **Mechanizm odkrywania** — T-25. Tutaj nie zmienia się ani `src/ui/screens.ts`, ani `src/App.tsx`.
  Cztery nowe pliki trafiają do globa same, bo taka jest konwencja.
- **Sekcja `run`** — T-08, razem z paskiem loadoutu i widokiem pracy.
- **Wciąganie umiejętności z linku i karta recenzji** — T-19, wylądowane. Ekran pokazuje wynik,
  nie buduje drugiego przepływu wciągania.
- **Płótno i panel kroku** — T-13. Sekcja Workflows montuje **listę**; przejście z listy na płótno
  domyka T-15, które ma na to własne kryterium.
- **Pasek kart workspace'ów** — T-24.
- **Sufit gęstości jako pomiar** — T-22 dokłada sprawdzenie; tutaj obowiązuje jako reguła
  projektowa, nie jako nowy check.

<!-- OWNS
src/sections/workflows/index.tsx
src/sections/workflows/mounted.test.tsx
src/sections/agents/index.tsx
src/sections/agents/mounted.test.tsx
src/sections/skills/index.tsx
src/sections/skills/mounted.test.tsx
src/sections/memory/index.tsx
src/sections/memory/mounted.test.tsx
-->
