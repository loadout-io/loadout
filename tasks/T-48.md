# T-48 — Sekcje, listy i inspektory

Piąte zadanie fali. Powłoka, ekran Run i marka są w nowym języku; **cztery sekcje listowe nie
są w nim ani trochę**. Zmierzone 2026-08-19 na wylądowanym `main`:

| co | stan |
|---|---|
| `rounded-sq` w Agents, Skills, Memory, Workflows | **60 wystąpień w 17 plikach** |
| jakikolwiek promień z pasma (`sm`/`md`/`lg`/`pill`) w tych sekcjach | **0** |
| alias barwy `*-wash` w tych sekcjach | **8 wystąpień** |
| promień `.tile`, `.node`, `.chip`, `.empty .ic`, `.side` w makiecie | **brak w ogóle** |
| `data-empty` w Workflows | na **opakowaniu**, nie na zdaniu — inaczej niż w trzech pozostałych |

`rounded-sq` i `bg-*-wash` to **aliasy**, które T-45 utrzymał przy życiu wyłącznie po to, żeby
migracja była addytywna (`--radius-sq: var(--radius-sm)`, `--color-attend-wash:
var(--color-attend-soft)`). T-50 je kasuje — a nie da się skasować aliasu, którego woła
sześćdziesiąt osiem miejsc. To zadanie jest tym, co T-50 musi zastać zrobione.

## Dwa kryteria z planu tej fali NIE weszły, i to jest zmierzone

**„Etykieta pola nie jest w wersalikach"** jest zielone od T-45: wersaliki przeniosły się na
`.text-eyebrow`, a `text-label` ich nie nosi. W żadnej z pięciu sekcji nie ma dziś ani jednego
`uppercase`. Kryterium, które przechodzi przed napisaniem linijki, nie jest kryterium.

**„Pusty ekran ma czynną kontrolkę"** jest zielone w trzech sekcjach (Agents, Skills, Workflows
mają `＋ Create` / `＋ Add a skill` przy zerze i pilnują tego własne testy `mounted.test.tsx`),
a w Memory jest **fałszywe z projektu**: notatki pisze agent, nie człowiek („Agents leave what
they learn here, for the next agent"). Dopisanie tam przycisku po to, żeby kryterium zzieleniało,
byłoby kontrolką bez czynności. Zamiast tego AC-3 pilnuje **znacznika**: `data-empty` ma siedzieć
na elemencie ze SAMYM zdaniem — tak mówi komentarz w `src/App.tsx` i tak robią trzy sekcje
z czterech.

**Żargonu tu nie sądzimy.** `checks/quick-vocabulary.sh` sądzi każdy napis w `src/` przy każdym
biegu bramki, a druga kopia tabeli żargonu w teście to dwa źródła prawdy o tej samej rzeczy
(niezmiennik 23).

## Chip: pigułka z washem swojego stanu

Spec §6, wiersz `chip`: `--radius-pill`, obrys `{stan}-edge`, tło `{stan}-soft`. Trzy chipy, które
dziś istnieją (`skills/review-card.tsx`, `memory/note-row.tsx`, `workflows/editor.tsx`), mają
wash pod aliasem i **kwadratowy narożnik**.

Chipa poznaje się po **pełnym obrysie i wypełnieniu stanu**, nie po nazwie klasy: przycisk
niebezpieczny ma obrys stanu i **nie ma wypełnienia** (spec: `button-danger`), a pasek błędu ma
`border-b`, czyli obrys jednej krawędzi. Dlatego kryterium nie żąda pigułki od wszystkiego, co
niesie barwę stanu — żąda jej od tego, co jest wypełnione i obwiedzione w całości.

## Odstępstwo od planu, świadome i nazwane

Plan tej fali przewidywał **inspektor dwukolumnowy z etykietą wyrównaną do prawej**, na wzór
Ustawień systemowych. Zmierzone: panel ma 330 px, a etykiety w tych formularzach to „Give up
after", „File access", „Runs with" — kolumna etykiet szeroka na 90 px łamie je na dwa wiersze,
a `textarea` z instrukcjami zostaje na 220 px. Etykieta **zostaje nad polem**, tak jak w makiecie,
i to jest decyzja, nie przeoczenie. Dwie kolumny wracają wtedy, kiedy inspektor dostanie
szerokość, w której się mieszczą.

## AC-1 Pasmo promieni i prawdziwe nazwy dochodzą do czterech sekcji
check: npx --no-install vitest run src/sections/radii-band-reaches-the-sections.test.tsx
expect: (\d+) passed

Czytane ze źródeł wszystkich siedemnastu plików, bo pytanie brzmi „czy w kodzie tych sekcji został
choć jeden alias". Asercje: (a) **ani jednego** `rounded-sq` i `rounded-dot`; (b) **ani jednego**
aliasu barwy `*-wash`; (c) każdy promień nazwany w tych plikach należy do pasma
`sm`/`md`/`lg`/`pill` — żadnej wartości arbitralnej, żadnej nazwy spoza pasma; (d) `rounded-md`
i `rounded-pill` występują co najmniej raz każdy, bo te sekcje mają i karty, i chipy;
(e) kontrola przeciw pustemu czytaniu: mniej niż dwanaście plików albo mniej niż dwadzieścia nazw
promieni to błąd testu, nie zieleń.

*Słaba wersja:* asercja, że `rounded-md` gdzieś jest. Przechodzi z sześćdziesięcioma aliasami
obok — czyli na dzisiejszym stanie plus jedna linia.

## AC-2 Chip stanu jest pigułką z washem, a nic nie miesza dwóch stanów
check: npx --no-install vitest run src/sections/state-chip-is-a-pill-with-its-wash.test.tsx
expect: (\d+) passed

Czytane z **wyrenderowanych i zasianych** sekcji (`createXStore(io)` → `load()` → ekran
z `store`), bo chipy pojawiają się dopiero przy danych. Asercje: (a) każdy element o **pełnym
obrysie stanu i wypełnieniu stanu** niesie `rounded-pill`; (b) **żaden** element w tych sekcjach
nie łączy obrysu jednego stanu z wypełnieniem innego — sądzone po wszystkim, także po paskach
i przyciskach; (c) przycisk niosący barwę stanu **nie ma wypełnienia**, bo prominencja należy do
akcentu; (d) istnieje chip neutralny: `rounded-pill` z obrysem `--line` i tekstem `--muted`;
(e) kontrola przeciw przemiataniu po pustym zbiorze: zero przeczytanych chipów to błąd testu.

*Słaba wersja:* sprawdzenie napisu klasy w źródle. Przechodzi na chipie, którego nikt nie montuje.

## AC-3 Znacznik pustego ekranu siedzi na zdaniu, w każdej z pięciu sekcji
check: npx --no-install vitest run src/sections/empty-screen-invites.test.tsx
expect: (\d+) passed

Każda sekcja renderowana przez **prawdziwe odkrywanie** (`<App section="…" />` bez `screens` —
z `screens={{}}` dostaje się kontrolę rejestru, nie sekcję). Asercje: (a) ekran niesie **dokładnie
jeden** `data-empty`; (b) treść oznaczonego elementu to **samo zdanie sekcji**: bez glifu `◇`, bez
zdania zapraszającego i bez etykiety przycisku; (c) zaproszenie i kontrolka stoją POZA oznaczonym
elementem, ale w tym samym ekranie; (d) nic z tego nie jest pustym napisem — zdanie ma co najmniej
dziesięć znaków; (e) w żadnym pustym ekranie nie ma widocznego `undefined`, `null`, `n/a`
ani `not reported`.

*Słaba wersja:* asercja na obecność `data-empty`. Przechodzi dziś, kiedy Workflows trzyma znacznik
na opakowaniu i treścią oznaczonego elementu jest „◇ zdanie zdanie ＋ Create".

## AC-4 Pole jest studnią pod swoją etykietą, w obu źródłach
check: npx --no-install vitest run src/sections/field-is-a-well-under-its-label.test.tsx
expect: (\d+) passed

Asercje: (a) w **wyrenderowanym** formularzu agenta każda etykieta z `for` stoi PRZED swoją
kontrolką i wskazuje na `id`, które naprawdę istnieje; (b) każda taka kontrolka niesie `bg-well`,
`border-line-strong` i `rounded-sm`; (c) każda niesie widoczny pierścień skupienia
(`focus-visible` z `--accent-ring`) — bez tego formularz jest nieobsługiwalny z klawiatury,
a to jest sekcja, w której wpisuje się instrukcje agenta; (d) to samo, czytane ze **źródeł**
pozostałych czterech sekcji: żadna kontrolka pod etykietą nie ma własnego promienia ani własnego
tła spoza pasma; (e) makieta mówi to samo, czytane z reguły `.fld input`; (f) kontrola przeciw
pustemu czytaniu: mniej niż dziewięć par etykieta-kontrolka w formularzu agenta to błąd testu.

*Słaba wersja:* asercja na `bg-well` w źródle formularza Agents. Cztery pozostałe sekcje mogą
wtedy zostać na kwadratach z aliasu.

<!-- OWNS
docs/mockup/index.html
docs/design/DESIGN.md
docs/superpowers/specs/2026-08-19-quiet-glass-design.md
src/sections/agents/index.tsx
src/sections/agents/agent-form.tsx
src/sections/agents/more-settings.tsx
src/sections/skills/index.tsx
src/sections/skills/review-card.tsx
src/sections/memory/index.tsx
src/sections/memory/note-row.tsx
src/sections/memory/passed-row.tsx
src/sections/memory/forced-choice.tsx
src/sections/workflows/index.tsx
src/sections/workflows/editor.tsx
src/sections/workflows/canvas/canvas.tsx
src/sections/workflows/canvas/problems.tsx
src/sections/workflows/canvas/tile.tsx
src/sections/workflows/list/tile.tsx
src/sections/workflows/list/workflow-list.tsx
src/sections/workflows/step-panel/panel.tsx
src/sections/workflows/step-panel/checkpoint-panel.tsx
src/sections/radii-band-reaches-the-sections.test.tsx
src/sections/state-chip-is-a-pill-with-its-wash.test.tsx
src/sections/empty-screen-invites.test.tsx
src/sections/field-is-a-well-under-its-label.test.tsx
-->
