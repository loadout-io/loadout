#!/usr/bin/env bash
# The gate.  ./verify.sh [before|quick|full] [--only AC-n] [--report]
# Pętla wewnętrzna i integrate.sh wołają TO. CI woła `bash scripts/ci.sh full` (AGENTS.md §6):
# na trunku nie ma TASK.md, więc ta bramka trafiłaby tam w strażnika pustki i wyszła 2.
# Dwie bramki, jeden zbiór sprawdzeń — ciała sprawdzeń mieszkają w checks/, nigdzie indziej.
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"

# Najpierw punkt przywracania. Git nie ma haka pre-checkout, więc `git checkout -- .`,
# `reset --hard` i `stash` kasują drzewo robocze bez śladu (reflog notuje tylko commity).
# W repo źródłowym: cztery takie incydenty jednego dnia, dwa zauważone dopiero wtedy, gdy
# "naprawione" sprawdzenie okazało się nienaprawione. Kosztuje milisekundy, niczego nie rusza.
bash harness/snapshot.sh 2>/dev/null || true

# Cała logika mieszka w bramce. Ten plik nie decyduje o niczym — gdyby decydował, byłoby
# drugie miejsce, w którym "zielone" znaczy co innego niż w CI.
exec python3 harness/gate.py "$@"
