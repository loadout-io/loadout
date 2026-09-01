#!/usr/bin/env python3
"""Odczepia bieg od tury orchestratora: podwojny fork + setsid, kod wyjscia do <log>.rc.

# Po co to istnieje

`docs/STATUS.md`, 2026-08-19: T-10 i T-54 zostaly ubite na twardym suficie 3600 s tla, oba
**PO** wykonaniu pracy, w fazie recenzji albo poprawek. Bieg odpalony jako dziecko tury umiera
razem z tura. Po `setsid` proces ma wlasna sesje i wlasna grupe procesow, wiec menedzer zadan
nie ma go jak zabrac na granicy tury.

Ten helper byl pisany od nowa **dwa razy** (19.08 i 20.08), bo za kazdym razem zostawal
w scratchpadzie sesji. Stad jest tutaj. Zmierzone 2026-08-20: dziewiec biegow w czterech falach,
zero zgubionych na granicy tury.

# Uzycie, z korzenia repo

    python3 scripts/detach.py <plik-logu> ./ship.sh "<prompt>" --agent claude

Zmienne srodowiska przechodza przez forki, wiec `FROM=`, `LOADOUT_TRUNK=`
i `LOADOUT_CARGO_LOCK_WAIT=` stawia sie przed wywolaniem jak zwykle.

# Dwie rzeczy, ktore trzeba wiedziec przy zabijaniu takiego biegu

1. **Zabijasz GRUPE, nie proces.** `kill -TERM <pid>` na basha zostawia zywego `claude -p`,
   ktory pisze do worktree i pali limit — zmierzone tej nocy przy wycofywaniu T-59. Poprawnie:
   `kill -TERM -<pgid>`, potem `kill -KILL -<pgid>`, a na koncu `kill -0 -<pgid>` musi dac
   ESRCH. To jest niezmiennik 6 zastosowany do wlasnych narzedzi.
2. **`<log>.rc` powstaje tylko przy normalnym zejsciu.** Bieg ubity nie zapisze paragonu, wiec
   czuwanie czekajace na ten plik trzeba domknac recznie (`echo 137 > <log>.rc`).
"""
import os
import subprocess
import sys

log = sys.argv[1]
cmd = sys.argv[2:]
if not cmd:
    sys.exit("uzycie: detach.py <log> <komenda> [argumenty...]")

# Pierwszy fork: rodzic wraca od razu, wiec tura nie czeka.
if os.fork() > 0:
    sys.exit(0)
# Wlasna sesja — odtad zaden sygnal do naszej grupy procesow tu nie dojdzie.
os.setsid()
# Drugi fork: proces przestaje byc liderem sesji, wiec nie odzyska terminala.
if os.fork() > 0:
    os._exit(0)

with open(log, "ab", buffering=0) as out:
    os.dup2(out.fileno(), 1)
    os.dup2(out.fileno(), 2)
    os.dup2(os.open(os.devnull, os.O_RDONLY), 0)
    rc = subprocess.call(cmd)

# Kod wyjscia na dysk, bo nie ma juz rodzica, ktory by go odebral.
with open(log + ".rc", "w", encoding="utf-8") as receipt:
    receipt.write(str(rc) + "\n")
os._exit(rc)
