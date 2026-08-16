# Stan budowy — 2026-08-16, 11:30

Ten plik jest **żywy**. Aktualizuje go orchestrator po każdym lądowaniu. Prawdą o zadaniu jest
`tasks/<ID>.md`; tutaj jest wyłącznie to, czego z plików zadań nie widać: co już stoi w trunku,
co stanęło i dlaczego.

## Liczby

| | |
|---|---|
| wylądowane | **12 z 26** |
| kosztowało | **$271** |
| trunk | zielony: `verify.sh full` 11/11, `scripts/ci.sh` z 11 strażnikami |
| produkt | 3 261 linii Rusta w 10 plikach, 21 plików testowych, 11 plików TS |

## Gdzie co jest

**Wylądowane (12):** `S-1 S-2 T-01 T-02 T-03 T-04 T-05 T-11 T-12 T-16 T-21 T-25`

**Gotowe do wzięcia od razu (3):** `T-13 T-14 T-18` — wszystkie zależności w trunku.

**Czekają na cudze zadania (10):** `T-07 T-08 T-09 T-15 T-17 T-19 T-20 T-22 T-23 T-24`.
Uwaga: `T-07`, `T-17` i `T-20` czekają **wyłącznie na T-06**, który jest odstawiony niżej.
Odblokowanie T-06 otwiera trzy zadania naraz i jest najwyżej punktowaną robotą, jaka została.

**Odstawione na czerwonym (1): `T-06`.** Worktree `../loadout-task-T-06` zostawiony celowo
jako dowód. Diagnoza niżej.

**Zablokowane brakiem kredytów Codeksa (2):** `S-3 T-10`. Kredyty wracają **2026-08-20**.
Wtedy też `docs/PLAN.md` §6a każe zrobić przegląd cross-vendor wszystkiego, co powstało
w trybie same-vendor — a to jest całość.

## T-06 — jedyne otwarte zatrzymanie

Wyszedł kodem **3** (sufit czasu) w warstwie `before`. Sześć kryteriów zeszło w ~1 s, a **`AC-2`
zjadło 840 s i zgłosiło „did not FINISH — it hung or could not start"** (budżet 420 s, wykorzystany
dwa razy).

`AC-2` celowo otwiera **drugie, zapisujące** połączenie `rusqlite` prosto na plik bazy,
z pominięciem naszego API — żeby udowodnić, że wyzwalacze append-only łapią też
`sqlite3 loadout.db` z terminala. To połączenie zawisło.

**Hipoteza, niepotwierdzona:** magazyn trzyma zamek zapisu, którego nie oddaje między
wywołaniami, więc drugi pisarz czeka do skutku (albo do `busy_timeout`). Jeśli to prawda, to
**nie jest defekt testu, tylko produktu** — każde zewnętrzne narzędzie zawiesi się tak samo,
a kryterium zrobiło dokładnie to, po co je napisano.

Czego **nie** sprawdziłem, a trzeba: czy w warstwie `before` szkielet w ogóle otwiera połączenie
(przy `todo!()` powinien panikować natychmiast, nie wisieć). Zacznij od przeczytania
`../loadout-task-T-06/src-tauri/tests/store_append_only.rs` i tego, co faza kontraktu wpisała
do `src-tauri/src/store/`.

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

## Kolizje kręgosłupa — to jest normalne, nie awaria

`src-tauri/src/lib.rs` zbiera `pub mod` od **każdego** zadania tworzącego moduł, więc przy
lądowaniu dwóch takich zadań konflikt jest **pewny**. Zdarzyło się dwa razy przy T-11 i T-12.
Rozwiązanie jest zawsze to samo: **zachowaj obie deklaracje**. Nie wybieraj strony.

To samo dotyczy `engine/mod.rs`, `memory/mod.rs`, `skills/mod.rs`, `drivers/mod.rs`.
`harness/task-spine.py` pilnuje, żeby każde zadanie miało gdzie dopisać swój wiersz.

## Rzecz, która nie zatrzyma bramki i dlatego jest groźna

`T-25` dał mechanizm montowania sekcji (`src/ui/screens.ts`, powłoka szuka
`src/sections/<id>/index.tsx`), ale **żadna sekcja nie ma jeszcze swojego `index.tsx`**.
`npm run dev` na `localhost:5273` pokazuje pięć pustych ekranów i **będzie tak pokazywał**,
dopóki nie wyląduje `T-08` — to on ma `AC-8`, jedyne kryterium, które renderuje `<App>` bez
wstrzykiwania i sprawdza, że zdania pustego ekranu tam **nie ma**.

Kryteria pozostałych sekcji to testy komponentowe wołane wprost na plikach. Przechodzą bez
montażu. Bramka nigdy o tym nie powie.
