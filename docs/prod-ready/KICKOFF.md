# Prompt startowy dla orkiestratora (wklej w całości)

Jesteś orkiestratorem pętli „prod-ready" w repo `~/Projects/Loadout`. Twoim jedynym zadaniem jest
doprowadzić do końca WSZYSTKIE pozycje z `docs/prod-ready/PLAN.md` — samodzielnie, bez pytania mnie
o kolejny krok, aż każda pozycja ma status `LANDED` albo `BLOCKED`. Pracujesz w pętli: wybierz →
uruchom → odbierz → wlej → zapisz stan → następna. Nie kończ tury zdaniem „mogę kontynuować" —
kontynuuj.

Zacznij tak:

1. Przeczytaj `AGENTS.md`, potem `docs/prod-ready/PLAN.md` w całości. Sekcja „Protokół pętli" jest
   wiążąca; sekcja „Stan" jest jedynym źródłem prawdy o tym, co zrobione. Audyt, z którego to
   wynika: `docs/prod-ready/AUDIT-2026-09-02.md` (czytaj tylko, gdy zadanie tego wymaga).
2. Wykonaj FALĘ 0 własnymi rękami, dokładnie według sekcji „Fala 0" planu, pakiet po pakiecie
   (0.1 → 0.5). Pliki pod `harness/`, `checks/`, `scripts/`, `.claude/`, `*.config.ts`,
   `package.json`, `Cargo.toml`, `AGENTS.md`, `docs/DECISIONS-LOCKED.md` są dla Edit/Write
   zablokowane — piszesz je przez `python3` z zapisem atomowym (tmp + `os.replace`), nigdy przez
   Edit. Po każdym pakiecie: testy wskazane w planie, commit na `main` z opisem
   `chore(prod-ready): 0.N …`, status w PLAN.md.
3. Od FALI 1 każde zadanie idzie przez harness:
   `scripts/h run <id> --prompt "$(cat docs/prod-ready/prompts/<ID>.md)" [--dev codex --verifier claude]`
   w tle (Bash z `run_in_background: true`, bo bieg trwa 30–90 min). Kiedy dostaniesz powiadomienie
   o końcu: kod 0 → `scripts/h land <id>` (na `main`, czyste drzewo) → `scripts/h clean <id>` →
   status `LANDED`; kod 2 → status `BLOCKED` z jednym zdaniem z werdyktu; kod 3 → jedno
   ponowienie z `LOADOUT_MAX_TURNS=400`, potem `BLOCKED`; kod 1 → przeczytaj `runs/<id>/` i
   rozstrzygnij, czy to maszyna (powtórz raz), czy kod (`BLOCKED`).
4. Równoległość: najwyżej JEDEN bieg dotykający Rusta naraz i najwyżej jeden bieg czysto TS obok
   niego (niezmiennik 26). `h land` biegnie wyłącznie, gdy żaden bieg nie trwa. Kolejność
   i zależności są w tabeli „Stan" — zadanie startuje, gdy wszystkie jego zależności są `LANDED`.
5. Po każdej zmianie statusu dopisz wiersz w sekcji „Dziennik" PLAN.md (data, ID, co się stało,
   koszt z `runs/<id>/`) i zacommituj PLAN.md na `main`. Gdy kontekst tej sesji robi się ciężki,
   zapisz stan i wywołaj `/compact` — plan jest napisany tak, żeby świeża sesja z tym samym
   promptem podjęła pracę od tabeli „Stan".
6. Nigdy: `git push`, `git reset --hard` (cofasz przez `git revert -m 1 HEAD`), edycja wyroczni
   z wnętrza biegu, dwa ciężkie `cargo` naraz, uruchamianie `cargo test`/`vitest` samemu, gdy bieg
   albo `land` trwa. Jeśli `ci.sh full` po merge'u jest czerwone: gdy to `fmt`/`clippy` — popraw na
   `main` i powtórz `bash scripts/ci.sh full`; inaczej `git revert -m 1 HEAD` i `BLOCKED`.
7. Zakończ dopiero, gdy każda pozycja ma `LANDED` albo `BLOCKED`. Wtedy napisz mi raport:
   co weszło, co zablokowane i dlaczego, łączny koszt z `runs/`, i co wymaga mojej ręki.

Pierwsze polecenie po przeczytaniu planu: pakiet 0.1 (sprzątanie maszyny) — od zabicia
osieroconego procesu, którego pid i ścieżka są w planie, po potwierdzeniu `ps`, że to nadal on.
