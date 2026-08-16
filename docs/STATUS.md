# Stan budowy — 2026-08-16, 13:45

Ten plik jest **żywy**. Aktualizuje go orchestrator po każdym lądowaniu. Prawdą o zadaniu jest
`tasks/<ID>.md`; tutaj jest wyłącznie to, czego z plików zadań nie widać: co już stoi w trunku,
co stanęło i dlaczego.

## Liczby

| | |
|---|---|
| wylądowane | **15 z 26** |
| kosztowało | **$317** (zmierzone z `runs/<ID>/*.jsonl`, bez recenzji i napraw) |
| trunk | zielony: `verify.sh full` 11/11, `scripts/ci.sh` z 11 strażnikami |
| produkt | 3 261 linii Rusta w 10 plikach, 21 plików testowych, 11 plików TS |

## Gdzie co jest

**Wylądowane (15):** `S-1 S-2 T-01 T-02 T-03 T-04 T-05 T-11 T-12 T-13 T-14 T-16 T-18 T-21 T-25`

**W locie (2):** `T-06` (implementacja — **kontrakt certyfikowany, zwis AC-2 zniknął**),
`T-19` (faza kontraktu). Pisarz i recenzent `claude` — Codex bez kredytów do 2026-08-20.

Szerokość fali to teraz **dwa**, i nie jest to wybór: wszystko poza T-19 przechodzi przez
T-06 → T-07 → T-08. Wylądowanie T-06 otwiera od razu trzy zadania (`T-07 T-17 T-20`).

**Uwaga recenzenta, której bramka nie widzi.** Zarówno T-14, jak i T-18 dostały uwagę tej samej
klasy: kryterium sprawdza **obecność** kontrolki albo ścieżki, a nie jej **zachowanie**.
W T-14 nagłówkowy `＋ Create` ma handler, ale każdy test używa `renderToStaticMarkup`, który
nigdy nie odpala `onClick`. W T-18 uzasadnienie AC-6 opisuje scenariusz, którego żaden test nie
przechodzi. Obie przeszły bramkę i obie wylądowały zgodnie z kontraktem (jedna runda naprawcza,
potem decyduje bramka) — ale to jest lista dla przeglądu cross-vendor po 2026-08-20.

**Czekają na cudze zadania (8):** `T-07 T-08 T-09 T-15 T-17 T-20 T-22 T-23 T-24`.
Uwaga: `T-07`, `T-17` i `T-20` czekają **wyłącznie na T-06** — jego wylądowanie otwiera trzy
zadania naraz i dlatego poszło pierwsze.

**Zablokowane brakiem kredytów Codeksa (2):** `S-3 T-10`. Kredyty wracają **2026-08-20**.
Wtedy też `docs/PLAN.md` §6a każe zrobić przegląd cross-vendor wszystkiego, co powstało
w trybie same-vendor — a to jest całość.

## T-06 — zdiagnozowane 2026-08-16, hipoteza o SQLite była błędna

**To nie był zamek SQLite. To zakleszczenie kanału tokio, i nie ma go nawet blisko bazy.**

`store_append_only.rs` robi to, co zrobiłby każdy wołający: bierze uchwyt pisarza
(`let writer = store.writer()`), pisze trzy zdarzenia, po czym woła `store.close().await`.
`Store::close(self)` upuszcza **swój** klon nadawcy i czeka na zadanie pisarza — a to zadanie
kończy się dopiero, kiedy `inbox.recv()` zwróci `None`, czyli po upadku **ostatniego** nadawcy.
Zmienna `writer` żyje do końca funkcji testowej. Kanał nigdy się nie zamyka, `recv()` wisi,
`close()` wisi, kryterium zjada dwa budżety po 420 s.

Czyli: **defekt produktu, nie testu.** API, w którym `close()` zawiesza się na zawsze, kiedy
wołający trzyma uchwyt, jest dokładnie tą klasą cichej awarii, przed którą stoi to repo. Test
ma rację i nie wolno go rozluźniać; naprawa należy do `store/`: zamknięcie ma być jawnym
zleceniem (`Job::Close`), po którym pętla kończy pracę niezależnie od żywych klonów, a spóźnieni
nadawcy dostają `WriterGone`. To jest ta sama decyzja, którą i tak musiałaby podjąć implementacja.

Bramka **nazwała to poprawnie** w paragonie („did not FINISH — it hung or could not start"),
a mimo to wyszła kodem 3 zamiast 1. Powód i naprawa: niżej, w „Co naprawiono w harnessie".

## Co naprawiono w harnessie 2026-08-16 (każdy ze strażnikiem albo z kontrolą negatywną)

Wszystkie wyszły z jednego incydentu — T-06 — i każdy z nich zatrzymałby pętlę znowu.

**`063c7e0` — sufit poziomu liczy retry, który bramka sama przyznaje.** `run_one` po timeoucie
pyta drugi raz (zamierzone), więc oracle autoryzuje do `2*b`. Sufit liczył `b`, poziom miał
460 s przy zjedzonych 840 s i przewracał się na „GATE TOO SLOW" — **za czas, który sam wydał**,
przykrywając jedyną actionable wiadomość. Kod 3 znaczy „przerwane albo maszyna" i wysyła
orchestratora po osierocone procesy; prawdą był defekt kontraktu. Liczenie wyjęte do
`ceiling_for()`, strażnik `hung_check_reads_as_red_not_as_a_slow_gate` w `scripts/ci.sh`.
Sufit **nadal** łapie bramkę wolną bez powodu — sprawdzone kontrolą negatywną.

**`a70af28` — faza kontraktu dostaje JEDNĄ rundę naprawczą.** Strona implementacyjna miała ją
od początku, kontraktowa nie miała żadnej — więc jeden błąd w szkielecie kasował cały bieg
i wymagał ręcznego skasowania worktree. Runda dostaje powody **z paragonu**: bramka rozróżnia
trzy kształty fałszywej czerwieni („did not FINISH", „PASSES before implementation",
„did not RUN") i każdy ma inną, nazwaną naprawę. Przy okazji routing wznowienia przestał
zgadywać z licznika commitów (na T-06 `commit_leftovers` dorobił drugi commit i skrypt kazał
wyrzucić pracę bez ani jednej linii implementacji) — pyta paragon, niezmiennik 20.

**`870b53f` — wznowienie nie płaci trzy razy za ten sam przebieg `before`.** Bez tego T-06
zapłaciłby 3 × 840 s plus zbędne przepisanie kontraktu od zera.

**`efda313` + `60c5f5e` — odcisk asercji, i strażnik dla niego.** Nowa runda otwiera jedną drogę
na skróty: „spraw, żeby padało INACZEJ" da się przeczytać jako „asertuj mniej". Reguła istniała
w promptcie i była miękka (niezmiennik 28). Teraz `assertion_fingerprint` liczy linie niosące
asercje per plik specyfikacji, a **każda** faza po certyfikacji kontraktu — implementacja,
naprawa, naprawa kontraktu — porównuje się z tą bazą. Spadek zatrzymuje bieg i nazywa plik.
Skasowany plik liczy się jako strata wszystkich asercji, bo inaczej najprostsze obejście było
niewidoczne.

**Odświeżenie oracle'a, i dlaczego trzeba było go naprawiać dwa razy.** Poprawka sufitu była dla
T-06 **nieosiągalna**: `worktree.sh` wycina cały katalog roboczy, więc gałąź niesie **własną**
kopię `harness/`, i ta stara kopia oddała 3 tam, gdzie nowa oddaje 1. `ship-task.sh` znał tę
klasę — podciągał trunk, ale dopiero **przed rundą naprawczą**, czyli po dwóch osądach.
Podniesione na start biegu (`refresh_harness_from_trunk`, strażnik pyta i o działanie,
i o kolejność). Druga próba padła znowu, bo ten merge **konfliktuje na `lib.rs`** — czyli
istniejące odświeżenie przed rundą naprawczą też cicho nie działało. Naprawione regułą
`merge=union` w `.gitattributes`: to zapis tego, co i tak robi się ręcznie za każdym razem,
i przy okazji zdejmuje ten konflikt z **każdego drugiego lądowania** w `integrate.sh`.

Trzy fałszywe starty T-06 nauczyły jednej rzeczy o samym harnessie: **naprawa bramki nie działa
wstecz na gałęzie już wycięte**, dopóki nie ma czym jej tam wpuścić.

## Co poszło nie tak przez noc — żeby nie powtórzyć

Sterownik falowy wylądował T-25 i T-16 o 03:03, a potem **stał osiem godzin**. WebStorm zapisał
pliki `.idea/`, `integrate.sh` słusznie odmówił lądowania na brudnym drzewie, a sterownik
odkładał land „na następną rundkę" **444 razy, nie wypisując ani razu, co jest brudne**.

Przyczyną nie był WebStorm, tylko **dwie definicje czystego drzewa**: `checks/quick-scope.sh`
ma `\.idea` na liście `GENERATED` od początku, `git status --porcelain` nie miał o tym pojęcia.
`.gitignore` dostał `.idea/` i `.vscode/` w `d08b6f0`.

Sterownik i `scripts/loop.sh` zostały **usunięte** (`3946181`). Nie odtwarzaj ich bez przeczytania
tamtego komunikatu commita: przez jedną noc trzy razy poprawiałem monitoring po tym, jak skłamał,
i i tak nie złapał jedynej awarii, która się wydarzyła. **Monitoring, który nie diagnozuje, jest
gorszy niż jego brak**, bo wygląda jak nadzór.

## Kolizje kręgosłupa — już rozwiązane regułą, nie ręką

`src-tauri/src/lib.rs` zbiera `pub mod` od **każdego** zadania tworzącego moduł, więc przy
dwóch takich zadaniach konflikt jest **pewny**. Zdarzył się przy T-11 i T-12, a potem zablokował
odświeżanie harnessu na T-06.

**Od 2026-08-16 nie musisz go rozwiązywać ręcznie.** `.gitattributes` niesie
`src-tauri/src/lib.rs merge=union`, czyli zapisaną wprost regułę „zachowaj obie deklaracje,
nigdy nie wybieraj strony". Obowiązuje tak samo przy `integrate.sh` i przy odświeżaniu harnessu
na gałęzi. Zweryfikowane na T-06: po merge'u stoi pięć deklaracji — `engine`, `store` z gałęzi,
`library`, `memory`, `workflow` z trunka. Nic nie zginęło, nic się nie zdublowało.

Jeden haczyk, który kosztował fałszywy start: **reguła musi być w gałęzi**, żeby zadziałała
przy merge'u. Gałąź wycięta przed jej wprowadzeniem potrzebuje jednorazowego wpuszczenia pliku
(`cp .gitattributes` + commit). Wszystkie gałęzie wycinane od teraz dostają go z trunka.

`engine/mod.rs`, `memory/mod.rs`, `skills/mod.rs` i `drivers/mod.rs` **celowo** zostały poza
regułą: mimo nazwy niosą prawdziwy kod, więc union mógłby skleić tam dwie wersje funkcji
zamiast dwóch deklaracji. Uzasadnienie w `docs/HARNESS-QUEUE.md`.
`harness/task-spine.py` dalej pilnuje, żeby każde zadanie miało gdzie dopisać swój wiersz.

## Rzecz, która nie zatrzyma bramki i dlatego jest groźna

`T-25` dał mechanizm montowania sekcji (`src/ui/screens.ts`, powłoka szuka
`src/sections/<id>/index.tsx`), ale **żadna sekcja nie ma jeszcze swojego `index.tsx`**.
`npm run dev` na `localhost:5273` pokazuje pięć pustych ekranów i **będzie tak pokazywał**,
dopóki nie wyląduje `T-08` — to on ma `AC-8`, jedyne kryterium, które renderuje `<App>` bez
wstrzykiwania i sprawdza, że zdania pustego ekranu tam **nie ma**.

Kryteria pozostałych sekcji to testy komponentowe wołane wprost na plikach. Przechodzą bez
montażu. Bramka nigdy o tym nie powie.
