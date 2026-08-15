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

## Czego świadomie NIE mechanizujemy

**„Jedna komenda na wywołanie Bash".** Kusi, żeby zrobić z tego hak `PreToolUse`, ale hak
odmawiający też kosztuje turę — dokładnie tę samą, którą kosztuje odmowa uprawnień. Zysku
zero, a dochodzi ryzyko fałszywej odmowy na poprawnym łańcuchu. Zostaje promptem, bo to
zachowanie, a nie stan, który da się wykryć i naprawić.

**Asercja, że hak formatujący jest podpięty.** Kusi jako jednolinijkowe sprawdzenie, ale
`.claude/**` jest dla biegu zabronione do zapisu — jedynym, kto może go odpiąć, jest
orchestrator. Sprawdzenie pilnowałoby wyłącznie mnie, a MANIFEST kazałby dopisać wpis.
Ceremonia większa niż ryzyko.
