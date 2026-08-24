# Jak to się spina

Mapa harnessu: kto kogo woła, czym się wymieniają i co znaczy każdy kod wyjścia.
Reguły wiążące są w [`AGENTS.md`](../AGENTS.md); tutaj jest wyłącznie okablowanie.

## Graf wywołań

```
  ship-task.sh <ID>                         graf biegu zapisany w kodzie
    ├─ worktree.sh <gałąź>                  → stdout: JEDNA linia, ścieżka worktree
    │    port: <git-dir worktree>/loadout-port   (nigdy <drzewo>/.port)
    ├─ cp tasks/<ID>.md → <wt>/TASK.md, commit jako PIERWSZY commit gałęzi
    ├─ verify.sh before      (pre-flight; exit 2 = brak kontraktu → stop przed wydatkiem)
    ├─ pisarz: faza kontraktowa (tylko pliki z linii `check:`)   prompt na STDIN
    ├─ verify.sh before      (egzekwowane; musi być czerwono z właściwego powodu)
    ├─ pisarz: implementacja                                     prompt na STDIN
    ├─ verify.sh full        → exit 2 = NASZA konfiguracja → stop, bez rundy naprawczej
    ├─ review.sh --agent A --reviewer B   → runs/review.json (schema-bound) + stdout
    ├─ repair.sh --agent A --reviewer B   ← runs/last.json (`failed`) + runs/review.json
    │    planista (recenzent, read-only) → runs/repair-plan.txt → pisarz wykonuje
    └─ verify.sh full        → kod wyjścia ship-task.sh

  verify.sh [before|quick|full] [--only AC-n] [--report] [--ids] [--jobs N]
    ├─ harness/snapshot.sh   refs/snapshots/<epoch>, bufor 40, drzewa NIE rusza
    └─ exec python3 harness/gate.py "$@"
         ├─ checks/<before|quick|full>-<id>.sh   ranga <= poziom → `bash <ścieżka>`
         ├─ TASK.md `## AC-n` + `check:`         kryteria, szerokość <= 2
         └─ runs/last.json                       paragon, tmp + os.replace

  integrate.sh <gałąź>...    bramka RAZ na trunku (exit 2 = brak TASK.md, jedziemy),
                             potem merge --no-ff + verify.sh full po KAŻDEJ gałęzi
  scripts/ci.sh [rust|web|full]   to woła CI. NIE verify.sh full: na trunku nie ma TASK.md
  .claude/hooks/stop-gate.sh      Stop hook → verify.sh full; exit 2 blokuje model
  harness/guards.sh               sadzi naruszenie w każdym checks/*.sh, wymaga czerwieni
```

## Kody wyjścia — identyczne w całym harnessie

| Kod | Znaczenie | Kto go produkuje |
|---|---|---|
| `0` | przeszło | wszystkie |
| `1` | sprawdzenie padło — uczciwa porażka kodu | gate.py, ci.sh, integrate.sh, guards.sh |
| `2` | **my** jesteśmy źle skonfigurowani — nigdy mylone z 1 | wszystkie |
| `3` | przerwane (SIGINT/SIGTERM) albo sufit czasu | gate.py, ci.sh, ship-task.sh, guards.sh |

Kod `2` to: brak sprawdzeń, brak `## AC-n` w TASK.md, defekt kontraktu zadania, brak
narzędzia (`prettier`, `cargo`, `python3`), brudne drzewo tam, gdzie ma być czyste.
Sprawdzenie projektowe, które wyszło dwójką, **przewraca cały poziom na 2**, a nie na 1:
„nie umiem sprawdzić" to inna wiadomość niż „jest źle", i tylko jedna z nich jest o kodzie.
`review.sh` jest jedynym skryptem, który kończy się **zawsze zerem** poza własnym kodem 2 —
niedostępny recenzent to fakt o świecie, nie werdykt o zmianie.

## Pliki

**`verify.sh`** — piętnaście linii: punkt przywracania, potem `exec python3 harness/gate.py`.
Nie decyduje o niczym, żeby nie było drugiego miejsca, w którym „zielone" znaczy co innego.

**`harness/gate.py`** — bramka. Odkrywa sprawdzenia z DWÓCH źródeł (`checks/<poziom>-<id>.sh`
po randze, `## AC-n` + `check:` z TASK.md), biegnie dwiema falami (projektowe do końca, potem
kryteria przy szerokości ≤ 2), pilnuje limitu na SPRAWDZENIE (`start_new_session` + `killpg`
SIGTERM→SIGKILL, jeden wydrukowany retry), egzekwuje regułę dowodu (`DEFAULT_EXPECT`) i
`NOT_A_REAL_RED`, w `before` ODWRACA wyłącznie kryteria, i zapisuje paragon.
`CHECK_TIMEOUT_OVERRIDE` (zimne cargo) mieszka tutaj, w oracle'u — bieg nie podnosi sobie limitu.
Każda diagnostyka kompilatora Rusta jest `NOT_A_REAL_RED`: test musi dojść do runtime i paść
na zachowaniu, nie zatrzymać wspólny cel integracyjny przed pierwszą asercją.

**`harness/snapshot.sh`** — `git stash create` → `refs/snapshots/<epoch>`, bufor pierścieniowy 40.
Nie dotyka drzewa, indeksu ani stasha. Odzysk: `git checkout refs/snapshots/<ts> -- <plik>`.

**`harness/guards.sh`** — dla każdego `checks/{before,quick,full}-*.sh`: zasadź prawdziwe
naruszenie, wymagaj czerwonego, przywróć, wymagaj zielonego. Check bez funkcji `guard_<id>`
to twarda porażka. Odmawia (2) na brudnym drzewie, bo pominięty guard czyta się jak zdany.

**`harness/review-schema.json`** — `verdict ∈ {concern, none}`, `findings` `maxItems: 6`,
`additionalProperties: false`. Nie ma wariantu „pass": recenzent strukturalnie nie ma czego
zatwierdzić. Ten sam plik jest `--output-schema` dla codeksa i wzorcem dla walidatora review.sh.

**`worktree.sh`** — wycina `../loadout-<nazwa>`, port z `cksum(nazwa) % 80 + 5300` (nigdy z
liczby żywych worktree), `node_modules` jako klon APFS, `target/` jako symlink do wspólnego
cache'u, zaufanie workspace'u dla obu vendorów. Na stdout leci jedna linia: ścieżka.

**`ship-task.sh`** — cały graf biegu w kodzie, nie w promptcie: model, który dostaje sekwencję
w promptcie, pomija etap, kiedy uzna go za zbędny. Pisarz jest wołany dwa razy (kontrakt, potem
implementacja), bo inaczej `verify.sh before` nie jest egzekwowalne, tylko poproszone.

**`review.sh` / `repair.sh`** — druga opinia i DOKŁADNIE jedna runda poprawek. Domyślnie
cross-vendor; przy same-vendor recenzent dostaje inny model i jawną rolę. Prompt zawsze STDIN-em
(niezmiennik 9), agent w WŁASNEJ grupie procesów, zamiatanej po stoperze (niezmiennik 6).
`repair.sh` czyta `runs/last.json` (klucz `failed`) i `runs/review.json`, i kończy kodem bramki.

**`integrate.sh`** — merge `--no-ff` po jednej gałęzi, pełna bramka po każdej. Konflikt na
`TASK.md` rozwiązuje kopia z trunka; każdy inny zostaje człowiekowi. Czerwień zostawia merge
na miejscu i drukuje `git reset --hard HEAD~1`.

**`scripts/ci.sh`** — jedyne źródło prawdy o tym, co CI uznaje za zielone. Dwie funkcje pasów,
`full` woła obie, więc `full == rust ∪ web` przez konstrukcję. Workflow nie wymienia ani jednego
kroku. CI **nie** woła `verify.sh full`: na trunku nie ma TASK.md, a `full` bez kryteriów
trafiłaby w strażnika pustki (exit 2) — zielone `full` znaczy „praca zrobiona", więc bez
kryteriów nie ma czego twierdzić.

Strażnik pustki jest świadomie **zależny od poziomu**: odmawiają `before` (nie ma czego odwracać)
i `full`. `verify.sh quick` na trunku działa normalnie i robi higienę — fmt, clippy, typy, zakres,
słownictwo, tokeny są sensowne same z siebie, a ich zielone nigdy nie twierdziło, że zadanie jest
skończone. Bez tego rozróżnienia człowiek pracujący poza worktree zadania nie miał żadnej szybkiej
pętli.

**`.claude/hooks/stop-gate.sh` + `.claude/settings.json`** — model nie kończy tury na czerwonym
drzewie. `BLOCK_CAP=3`, licznik w `$(git rev-parse --git-dir)/stop-gate-blocks` (nie w `.git/`,
bo w worktree to plik), honoruje `stop_hook_active`, a na kodzie 2 ustępuje człowiekowi.

**`checks/`** — jedno sprawdzenie na plik; nazwa pliku steruje poziomem. `_cargo-serialize.sh`
jest SOURCE'owane, nie odkrywane (niezmiennik 26: jeden ciężki cargo naraz). `tsconfig.strict.json`
i obie listy słownictwa leżą tutaj, poza zasięgiem biegu — bieg nie rozluźnia własnej bramki.
