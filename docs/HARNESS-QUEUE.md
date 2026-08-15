# Kolejka poprawek harnessu

Zmiany gotowe do naniesienia, których **nie wolno nanieść w trakcie biegu**. Nanieść przy
pierwszym postoju pętli, potem skasować wpis.

Zasada, która tę kolejkę tworzy: bash czyta skrypt **przyrostowo**, po offsetach bajtowych.
Edycja pliku, który właśnie się wykonuje, przesuwa wszystko za kursorem i proces wykonuje
śmieci — składniowo poprawne, semantycznie losowe. Zdarzyło się trzy razy 2026-08-15,
za każdym razem po moim własnym ostrzeżeniu.

Reguła nadrzędna dla wszystkich pozycji: **jeśli da się to zrobić skryptem, hakiem albo
uprawnieniami, nie robimy tego promptem.** Prompt jest miękki (bieg może go zignorować i nikt
tego nie zauważy), rośnie w nieskończoność i kosztuje tokeny w każdym biegu. Skrypt jest
twardy, deterministyczny i kosztuje raz.

---

## Q-1 — `ship-task.sh` ma odpalać się z przypiętej kopii

**Klasa:** ta poprawka kasuje powód istnienia całej tej kolejki.

Na samą górę `ship-task.sh`, zaraz za `set -euo pipefail`:

```bash
# Bash czyta ten plik przyrostowo, po offsetach bajtowych. Edycja w trakcie biegu przesuwa
# wszystko za kursorem i proces wykonuje smieci. Zdarzylo sie trzy razy 2026-08-15.
# Kopia jest niezmienna, wiec orchestrator moze naprawiac harness, kiedy petla chodzi.
if [ -z "${LOADOUT_PINNED:-}" ]; then
  snap="$(mktemp -t ship-task)"
  cat "$0" > "$snap"
  export LOADOUT_PINNED=1
  exec bash "$snap" "$@"
fi
```

Do sprawdzenia przy nanoszeniu: czy `$0` w kopii nie jest nigdzie używane do wyliczania
katalogu repo. Jeśli jest — najpierw wyliczyć `ROOT`, potem `exec`, i przekazać przez zmienną.

To samo dotyczy `scripts/build-loop.sh`, z tego samego powodu.

---

## Q-2 — wyrzucić akapit o formatterze z promptu kontraktu

Nieaktualny od `d586ad9`. Formatowanie robi hak `PostToolUse`, bieg nie ma już czego
pamiętać. Do usunięcia (ok. wiersz 296):

```
Before you finish, run the formatter: npm run fmt for the frontend and cargo fmt for Rust.
quick-fmt is part of the gate, and a formatting diff is the cheapest possible way to turn an
otherwise green task red. Measured on T-01: seventeen checks, one failure, prettier.
```

---

## Q-3 — przeterminowany muteks cargo to kod 2, nigdy 1

**Klasa:** fałszywa czerwień. Uzbraja się dokładnie wtedy, gdy zaczniemy puszczać zadania
równolegle przez Workflow, czyli w następnej fazie planu.

`checks/quick-clippy.sh:33`, `checks/full-clippy.sh:32`, `checks/full-test.sh:33` mają dziś:

```bash
cargo_serialize || exit 1
```

Kiedy `cargo_serialize` nie doczeka się muteksu w 300 s, sprawdzenie **nie uruchomiło niczego**
— a mimo to melduje **1**, czyli „twój kod jest zepsuty". `gate.py` sam nazywa to błędem
w komentarzu, który ta linia łamie: *„2 wygrywa z 1 i z 3: skoro jedno sprawdzenie nie umiało
się wykonać, werdykt poziomu nie jest twierdzeniem o kodzie"*. Na poziomie sprawdzenia
`gate.py` zna tylko dwie kategorie — `2` to `misconfigured`, wszystko inne niezerowe to
`failed` — więc jedyną poprawną odpowiedzią jest **2**.

Zamiana we wszystkich trzech plikach:

```bash
cargo_serialize || exit 2
```

Zmierzone 2026-08-15 22:00: `scripts/ci.sh` odpalony w trakcie T-03 czekał na muteks pełne
300 s, po czym strażnik `quick-clippy` napisał „RED WITH THE VIOLATION GONE (exit 1) — the
guard proves nothing". Strażnik zadziałał, sprawdzenie nie.

Przy nanoszeniu dopisać strażnika: zająć `$TMPDIR/loadout-cargo.lock` przez `mkdir`, odpalić
sprawdzenie z `LOADOUT_CARGO_LOCK_WAIT=2` i wymagać **2**, nie 1.

Uboczny wniosek, którego NIE naprawiamy teraz: dopóki muteks jest zajęty, `scripts/ci.sh`
nie da się wykonać do końca. To nie jest defekt — to ten sam niezmiennik 26 robiący swoje.
Odpalać `ci.sh` przy postoju pętli albo w tle.

---

## Q-4 — podciągnięcie trunka ma być PRZED naprawą, nie po niej

**Klasa:** poprawka `01deb45` zrobiona w połowie. Sama znalazła swoją drugą połowę.

Dzisiejsza kolejność w `ship-task.sh`:

```
bramka → druga opinia → repair.sh → merge main → bramka
```

Czyli agent naprawiający pracuje przeciwko **nieaktualnej kopii harnessu**, a dopiero po nim
podciągamy trunk i sądzimy go nową bramką. To jest dokładnie ta klasa fałszywych zatrzymań,
którą `01deb45` miał skasować — trzy z pierwszych czterech postojów pętli 2026-08-15.
Naprawa może trafić w stare sprawdzenie i przewrócić się na nowym, i nikt nie zrozumie dlaczego.

Agent naprawiający jest przy tym tym, który **najbardziej** potrzebuje aktualnej bramki:
jego jedynym zadaniem jest odpowiedzieć na to, co bramka powiedziała.

Docelowo:

```
bramka → druga opinia → merge main → repair.sh → bramka
```

Powód, dla którego merge stoi tam, gdzie stoi, jest w komentarzu: *„Moment jest bezpieczny,
bo żaden agent już nie pracuje"*. Po drugiej opinii jest równie bezpieczny — recenzent
skończył, naprawiacz jeszcze nie wystartował. Warunek jest spełniony w obu miejscach,
więc nic nie stoi na przeszkodzie.

Efekt uboczny, dla którego warto to zrobić od razu: hak formatujący (`d586ad9`) trafi do
worktree **przed** rundą naprawczą, więc naprawiacz przestanie móc wywrócić zadanie
przecinkiem. Zmierzone na T-03: `quick-fmt` czerwony na pliku testowym, w gałęzi wyciętej
przed hakiem.

---

## Czego świadomie NIE mechanizujemy

**„Jedna komenda na wywołanie Bash".** Kusi, żeby zrobić z tego hak `PreToolUse`, ale hak
odmawiający też kosztuje turę — dokładnie tę samą, którą kosztuje odmowa uprawnień. Zysku
zero, a dochodzi ryzyko fałszywej odmowy na poprawnym łańcuchu. Zostaje promptem, bo to
zachowanie, a nie stan, który da się wykryć i naprawić.

**Asercja, że hak formatujący jest podpięty.** Kusi jako jednolinijkowe sprawdzenie, ale
`.claude/**` jest dla biegu zabronione do zapisu — jedynym, kto może go odpiąć, jest
orchestrator. Sprawdzenie pilnowałoby wyłącznie mnie, a MANIFEST kazałby dopisać wpis.
Ceremonia większa niż ryzyko.
