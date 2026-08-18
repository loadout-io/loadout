#!/usr/bin/env bash
# ~0,1 s, bez kompilacji. Zbiór kluczy KAŻDEGO `invoke('<nazwa>', { … })` kontra lista parametrów
# tej komendy przeczytana z `src-tauri/src/ipc.rs`.
#
# INCYDENT, PO KTÓRYM TO POWSTAŁO. Zmierzone 2026-08-17 na wyladowanym trunku:
#
#   src-tauri/src/ipc.rs:653   pub async fn run_workflow(state, file_name, how_many_at_once,
#                                                        lines: Channel<Vec<Line>>)
#   src/sections/run/io.ts:68  invoke<void>('run_workflow', { fileName, howManyAtOnce })
#
# Dwa klucze z trzech. Tauri dopasowuje argumenty PO NAZWIE i deserializuje je, ZANIM wejdzie
# w ciało komendy, więc Start był odrzucany przy KAŻDYM kliknięciu, a jedyne, co widział człowiek,
# brzmiało „Loadout could not start that run" — zdanie, które nie nazywa przyczyny.
#
# DLACZEGO NIC TEGO NIE ŁAPAŁO, i to jest sedno. `T-30 AC-4` został SPECJALNIE utwardzony
# przeciw dryfowi kluczy, poprawnie cytuje regułę i poprawnie wskazuje `ipc.rs` jako źródło
# nazw — a potem przepisuje z tego źródła DWIE nazwy z trzech do literału w teście:
#   expect({ fileName: carried['fileName'], howManyAtOnce: carried['howManyAtOnce'] }) …
# Rzut na dwa ręcznie wpisane klucze jest strukturalnie niewidzący na brakujący trzeci.
# Niezmiennik 28 mówi, co wtedy zrobić: reguła, która była promptem i komentarzem i mimo
# poprawnego brzmienia zawiodła, ma się stać sprawdzeniem. To jest to sprawdzenie.
#
# SKĄD BIERZE SIĘ „PRAWIDŁOWY KLUCZ" — i nie jest to nasza konwencja, tylko cudzy kod:
#   tauri-macros 2.6.3, src/command/wrapper.rs:505-507 — `argument_case` domyślnie `Camel`,
#     więc `file_name` staje się na drucie `fileName`. `rename_all = "snake_case"` to zmienia
#     i dlatego ten skrypt czyta atrybut, a nie zakłada domyślnej.
#   tauri 2.11.5, src/ipc/channel.rs:300 — `Channel` NIE jest wstrzykiwany. Deserializuje
#     stringa spod SWOJEGO klucza, więc brak klucza `lines` to odmowa, nie pusty kanał.
#   tauri 2.11.5 — klucz ignorują wyłącznie: State, AppHandle, Webview, WebviewWindow, Window,
#     CommandScope, GlobalScope, Request. Ta lista jest zamknięta i stoi niżej w INJECTED.
#
# ZAKRES JEST WĄSKI Z PREMEDYTACJĄ, dokładnie jak w `checks/quick-wired.sh`: sądzimy wyłącznie
# wywołania, w których nazwa komendy jest literałem stringa, a argumenty literałem obiektu
# o statycznych kluczach. `invoke('put_note_to_use', args)` (identyfikator) i literał ze
# spreadem albo z kluczem liczonym są POMIJANE bez słowa — grep nie odpowiada na pytanie, co
# jest w `args`, a sprawdzenie, które zgaduje, hałasuje.
#
# DWA ROZSZERZENIA POZA LITERĘ AC-8, oba świadome i oba opisane, żeby nie były niespodzianką.
# (1) Wywołanie BEZ drugiego argumentu (`invoke('stop_run')`) też jest sądzone: skasowanie
# `{ … }` jest skrajnym przypadkiem brakującego klucza, a nie wyjściem poza zakres — gdyby było
# przemilczane, obejściem tego sprawdzenia byłoby usunięcie literału. (2) Nazwa komendy, której
# w `ipc.rs` nie ma, jest zgłaszana zamiast przemilczana: milczenie znaczyłoby, że przemianowanie
# komendy po stronie Rusta gasi to sprawdzenie, czyli że da się je wyłączyć bez kasowania.
# Sprawdzenie, które hałasuje, jest
# obchodzone, a nie naprawiane. Zmierzone 2026-08-18 na gałęzi T-38: 18 komend w `ipc.rs`,
# 14 wywołań w zakresie (9 z literałem obiektu, 5 bezargumentowych), 2 pominięte jako
# nieliterałowe (`putToUse`, `stopUsing` — oba oddają `args`), 0 fałszywych trafień. Kontrola:
# `grep -rnoE "invoke(<[^(]*>)?\(['\"]" src/` bez testów daje 16, czyli 14 + 2, co do sztuki.
# Pliki `*.test.ts(x)` są poza zakresem, bo kryteria ZASADZAJĄ w nich złe klucze
# jako kontrolę negatywną — sądzenie ich zamieniłoby czerwień testu w czerwień bramki.
#
# CZEGO TO SPRAWDZENIE ŚWIADOMIE NIE WIDZI (żeby następny czytelnik nie musiał tego gerpować):
# wywołania budującego obiekt argumentów w zmiennej. Zamknięcie tej dziury wymaga typów, a nie
# gerpa — `checks/quick-types.sh` sądzi to samo drzewo w trybie strict i to jest właściwe
# miejsce. Tutaj kończy się mandat.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

command -v python3 >/dev/null 2>&1 || { echo "python3 is not on PATH" >&2; exit 2; }

# Drzewo bez frontu jest możliwe (i było takie przed T-01). Cisza jest tu poprawną odpowiedzią.
if [ ! -d src ]; then
  echo "invoke-args: no src/ yet, no call to compare"
  exit 0
fi

# Front BEZ wyroczni to już awaria NASZEGO układu, nie sądzonego kodu: nie ma z czym porównywać,
# a „przeszło" znaczyłoby wtedy „nie sprawdziłem". Exit 2, tak jak w full-clippy.sh przy
# rozłączonej polityce — pisarz ma nie iść na polowanie po kodzie, w którym nie ma czego znaleźć.
if [ ! -f src-tauri/src/ipc.rs ]; then
  echo "src/ exists but src-tauri/src/ipc.rs does not, so the argument names have no oracle" >&2
  echo "detail: this check compares invoke() keys against the command signatures in that file" >&2
  exit 2
fi

exec python3 - <<'PY'
import os
import re
import sys

IPC = "src-tauri/src/ipc.rs"
FRONT = "src"

# Typy, które Tauri WSTRZYKUJE: ich `CommandArg::from_command` nie sięga po `command.key`, więc
# nie odpowiada im żaden klucz na drucie. Lista jest zamknięta i pochodzi z tauri 2.11.5
# (`impl … CommandArg for …`). `Channel` jej NIE należy i to jest cały incydent.
INJECTED = {
    "State",
    "AppHandle",
    "Webview",
    "WebviewWindow",
    "Window",
    "CommandScope",
    "GlobalScope",
    "Request",
}


def read(path):
    with open(path, encoding="utf-8", errors="replace") as handle:
        return handle.read()


def balanced(text, start, opener, closer):
    """Zwraca (wnętrze, indeks_za_domknięciem) dla nawiasu stojącego pod `start`."""
    depth = 0
    i = start
    while i < len(text):
        char = text[i]
        if char == opener:
            depth += 1
        elif char == closer:
            depth -= 1
            if depth == 0:
                return text[start + 1 : i], i + 1
        i += 1
    return None, None


def split_top(text):
    """Dzieli po przecinkach na poziomie zero, znając <>, (), [] i strzałkę `->`."""
    parts = []
    depth = 0
    current = []
    i = 0
    while i < len(text):
        pair = text[i : i + 2]
        if pair == "->":
            current.append(pair)
            i += 2
            continue
        char = text[i]
        if char in "<([":
            depth += 1
        elif char in ">)]":
            depth -= 1
        if char == "," and depth == 0:
            parts.append("".join(current))
            current = []
        else:
            current.append(char)
        i += 1
    parts.append("".join(current))
    return [p.strip() for p in parts if p.strip()]


def base_type(text):
    """`&'a tauri::State<'_, AppState>` → `State`. Nazwa bazowa, bez ścieżki i bez generyków."""
    text = text.strip().lstrip("&").strip()
    text = re.sub(r"^'[A-Za-z_]\w*\s*", "", text).strip()
    text = re.sub(r"^mut\s+", "", text).strip()
    head = re.split(r"[<\s,]", text, maxsplit=1)[0]
    return head.split("::")[-1].strip()


def lower_camel(name):
    """Odpowiednik `heck::ToLowerCamelCase` dla identyfikatorów snake_case z Rusta."""
    parts = [p for p in name.split("_") if p]
    if not parts:
        return ""
    head = parts[0].lower()
    return head + "".join(p[:1].upper() + p[1:].lower() for p in parts[1:])


def snake(name):
    return name.lower()


COMMAND_ATTR = re.compile(r"#\[\s*(?:tauri\s*::\s*)?command\s*(?:\(([^)]*)\))?\s*\]")
FN_HEAD = re.compile(r"\bfn\s+([A-Za-z_]\w*)\s*\(")


def commands_from_ipc(text):
    """nazwa komendy → (zbiór kluczy na drucie, uporządkowana lista kluczy)."""
    table = {}
    for attr in COMMAND_ATTR.finditer(text):
        options = attr.group(1) or ""
        to_key = snake if '"snake_case"' in options else lower_camel
        renamed = re.search(r'rename\s*=\s*"([^"]+)"', options)
        head = FN_HEAD.search(text, attr.end())
        if head is None:
            continue
        params, _ = balanced(text, head.end() - 1, "(", ")")
        if params is None:
            continue
        keys = []
        for raw in split_top(params):
            raw = re.sub(r"^#\[[^\]]*\]\s*", "", raw).strip()
            if ":" not in raw:
                continue
            ident, _, ty = raw.partition(":")
            ident = re.sub(r"^mut\s+", "", ident.strip()).strip()
            if not ident or ident == "_":
                continue
            if base_type(ty) in INJECTED:
                continue
            keys.append(to_key(ident))
        name = renamed.group(1) if renamed else head.group(1)
        table[name] = keys
    return table


def blank_comments(text):
    """Komentarze zamienione na spacje, długość zachowana. Stringi zostają — niosą nazwy komend."""
    out = list(text)
    i = 0
    n = len(text)
    while i < n:
        char = text[i]
        if char in "'\"`":
            quote = char
            i += 1
            while i < n:
                if text[i] == "\\":
                    i += 2
                    continue
                if text[i] == quote:
                    i += 1
                    break
                i += 1
            continue
        if char == "/" and i + 1 < n and text[i + 1] == "/":
            while i < n and text[i] != "\n":
                out[i] = " "
                i += 1
            continue
        if char == "/" and i + 1 < n and text[i + 1] == "*":
            while i < n and text[i : i + 2] != "*/":
                if text[i] != "\n":
                    out[i] = " "
                i += 1
            for j in range(i, min(i + 2, n)):
                out[j] = " "
            i += 2
            continue
        i += 1
    return "".join(out)


def skip_space(text, i):
    while i < len(text) and text[i] in " \t\r\n":
        i += 1
    return i


def read_generics(text, i):
    """Przeskakuje `<…>` po nazwie `invoke`, jeśli tam stoi. Zwraca indeks albo None."""
    if i >= len(text) or text[i] != "<":
        return i
    depth = 0
    while i < len(text):
        if text[i] == "<":
            depth += 1
        elif text[i] == ">":
            depth -= 1
            if depth == 0:
                return i + 1
        elif text[i] in ";{}":
            return None
        i += 1
    return None


IDENT = re.compile(r"[A-Za-z_$][A-Za-z0-9_$]*")


def object_keys(text, start):
    """Klucze literału obiektu stojącego pod `start`. None = literał poza zakresem sprawdzenia."""
    body, _ = balanced(text, start, "{", "}")
    if body is None:
        return None
    keys = []
    i = 0
    n = len(body)
    expect_key = True
    depth = 0
    while i < n:
        char = body[i]
        if expect_key and depth == 0:
            i = skip_space(body, i)
            if i >= n:
                break
            char = body[i]
            if char == "}":
                break
            if char in "[.":
                # klucz liczony albo spread — nie wiemy, co wniesie. Poza zakresem.
                return None
            if char in "'\"":
                quote = char
                j = i + 1
                buf = []
                while j < n and body[j] != quote:
                    if body[j] == "\\":
                        return None
                    buf.append(body[j])
                    j += 1
                keys.append("".join(buf))
                i = j + 1
                expect_key = False
                continue
            if char == "`":
                return None
            match = IDENT.match(body, i)
            if match is None:
                return None
            keys.append(match.group(0))
            i = match.end()
            expect_key = False
            continue
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
        elif char in "'\"`":
            quote = char
            i += 1
            while i < n:
                if body[i] == "\\":
                    i += 2
                    continue
                if body[i] == quote:
                    break
                i += 1
        elif char == "," and depth == 0:
            expect_key = True
        i += 1
    return keys


CALL = re.compile(r"\binvoke\s*")
NAME = re.compile(r"""\s*(['"])([A-Za-z_]\w*)\1\s*""")


def calls_from_front(root):
    """(plik, linia, komenda, klucze) dla każdego wywołania w zakresie. `keys=None` = bez obiektu."""
    found = []
    skipped = 0
    for base, dirs, names in os.walk(root):
        dirs[:] = [d for d in dirs if d != "node_modules"]
        for name in sorted(names):
            if not name.endswith((".ts", ".tsx")):
                continue
            if ".test." in name or name.endswith(".spec.ts"):
                continue
            path = os.path.join(base, name)
            text = blank_comments(read(path))
            for hit in CALL.finditer(text):
                i = read_generics(text, hit.end())
                if i is None:
                    continue
                if i >= len(text) or text[i] != "(":
                    continue
                after = NAME.match(text, i + 1)
                if after is None:
                    continue
                command = after.group(2)
                line = text.count("\n", 0, hit.start()) + 1
                j = after.end()
                if j < len(text) and text[j] == ")":
                    found.append((path, line, command, []))
                    continue
                if j >= len(text) or text[j] != ",":
                    skipped += 1
                    continue
                j = skip_space(text, j + 1)
                if j < len(text) and text[j] == ")":
                    found.append((path, line, command, []))
                    continue
                if j >= len(text) or text[j] != "{":
                    skipped += 1
                    continue
                keys = object_keys(text, j)
                if keys is None:
                    skipped += 1
                    continue
                found.append((path, line, command, keys))
    return found, skipped


table = commands_from_ipc(read(IPC))

# Parser, który po cichu zwrócił pustą tablicę, przepuściłby KAŻDE wywołanie na porównaniu dwóch
# pustych zbiorów — czyli byłby sprawdzeniem, które nigdy nie świeci (niezmiennik 19). To jest
# awaria naszego układu, nie sądzonego kodu, więc 2, a nie 1.
if not table:
    sys.stderr.write(
        "parsed ZERO #[tauri::command] signatures out of %s, so every key set below would be\n"
        "compared against an empty list and pass on nothing. Refusing to report a clean tree.\n"
        % IPC
    )
    raise SystemExit(2)

calls, skipped = calls_from_front(FRONT)

problems = []
for path, line, command, keys in calls:
    if command not in table:
        problems.append(
            (
                path,
                line,
                command,
                "no #[tauri::command] named %s exists in %s, so this call can never be matched"
                % (command, IPC),
            )
        )
        continue
    expected = table[command]
    missing = [k for k in expected if k not in keys]
    extra = [k for k in keys if k not in expected]
    if not missing and not extra:
        continue
    detail = []
    if missing:
        detail.append("missing key(s): %s" % ", ".join(missing))
    if extra:
        detail.append("unknown key(s): %s" % ", ".join(extra))
    problems.append(
        (
            path,
            line,
            command,
            "%s -- %s takes [%s], this call passes [%s]"
            % (
                "; ".join(detail),
                command,
                ", ".join(expected),
                ", ".join(keys),
            ),
        )
    )

if problems:
    sys.stderr.write(
        "these invoke() calls do not match the command signature in %s:\n" % IPC
    )
    for path, line, command, detail in problems:
        sys.stderr.write("  %s:%d  call to '%s'\n" % (path, line, command))
        sys.stderr.write("      %s\n" % detail)
    sys.stderr.write(
        "\nTauri matches command arguments BY NAME and deserializes them BEFORE it enters the\n"
        "command body, so a wrong key set is not a smaller payload -- it is a rejected call, on\n"
        "every single click, with a message the user never sees. That is how Start shipped broken\n"
        "on 2026-08-17 behind a green criterion that cast the payload onto two hand-typed keys.\n"
        "Argument names live in ONE place: %s. Read them, do not retype them.\n" % IPC
    )
    raise SystemExit(1)

print(
    "invoke-args: %d invoke call(s) in %s judged against %d commands, every key set exact "
    "(%d call(s) out of scope: no object literal)" % (len(calls), FRONT, len(table), skipped)
)
PY
