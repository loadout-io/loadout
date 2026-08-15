# S-1 — Can a Claude session be given a subset of skills?

**Status:** answered, measured
**Date:** 2026-08-15
**CLI:** `claude --version` → `2.1.233 (Claude Code)`
**Raw probe artifacts:** `runs/S-1/*.jsonl` (control + M1, M2, M3, M3a, M3b)

---

## 0. The answer in one paragraph

**Yes — a per-session subset is real, and it takes a generated directory plus two flags, not one flag.**
The session saw **54** skills with no mechanism and **18** with the mechanism, and the two skills we
placed in the generated directory were among those 18 while all 38 of the user's own skills were gone.
The mechanism is `--plugin-dir <generated dir>` (which *adds* our skills) combined with
`--setting-sources ""` (which *removes* the user's). Either flag alone fails: `--plugin-dir` on its own
took 54 → 56, i.e. it is an add, not a filter; `--setting-sources ""` on its own took 54 → 16, i.e. it is
a truncation to a floor we do not control. T-13 may render the "Only these" checkbox list [T3 §7.1];
T-18 builds the directory.

**The one caveat that changes UI copy:** 16 skills survive `--setting-sources ""` on this machine
(`deep-research`, `dataviz`, `code-review`, `run`, …). They are not read out of `~/.claude/skills` —
no flag we found drops them short of `--disable-slash-commands`, which drops *everything* to 0. So the
honest UI promise is **"only these, plus the CLI's own bundled skills"**, not "only these, full stop".
The checkbox list governs exactly the 38 skills that did disappear. If T-13 renders the row as an
absolute guarantee, it is lying by 16.

---

## 1. Why the control run is the whole point

The trap this spike exists to catch: run `claude` with an isolation flag inside an empty directory,
watch `system/init` report two skills, write down "subsetting works". You would have measured a
directory that held two skills all along, or a flag that zeroed everything and let two in from
elsewhere. Both produce the same number and neither is a subset.

So: same directory (`/tmp/s1-run`), same minute, same prompt, same model, same
`--strict-mcp-config`, run once without the mechanism and once with it. A subset is
`0 < treatment < control`. `54 → 18` is a subset. `54 → 56` (M3 alone) is an add.
`54 → 0` (M2) is "nothing".

One correction to TASK.md's recipe, worth recording because the next person will hit it: `head -1
control.jsonl` does **not** hold `system/init` on a machine with hooks configured. Line 1 was
`{"type":"system","subtype":"hook_started",...}`; `system/init` was line 7. Scan for
`type == "system" && subtype == "init"` instead of trusting the first line — `head -1` would have
reported `skills: 0` for every run and produced a confident, wrong `not-possible`.

## 2. What each candidate did

All six runs used the identical base command, in `/tmp/s1-run`, on 2026-08-15:

```bash
claude -p "Reply with exactly: OK" --output-format stream-json --verbose \
       --model haiku --strict-mcp-config < /dev/null
```

| # | Added to the base command | `skills` | Reading |
|---|---------------------------|----------|---------|
| — | *(control)* | **54** | the user's full complement |
| M1 | `--setting-sources ""` | 16 | not a subset we chose — a floor the CLI keeps |
| M2 | `--disable-slash-commands` | 0 | "nothing", exactly as its help text says |
| M3 | `--plugin-dir /tmp/s1-only-two` | 54 | plugin loaded, **zero skills registered** — wrong layout |
| M3a | `--plugin-dir /tmp/s1-plugin-a` | 56 | adds our 2 on top of 54 — an add, not a filter |
| **M3b** | `--setting-sources "" --plugin-dir /tmp/s1-plugin-a` | **18** | **16 floor + our 2 — the subset** |
| M4 | `--setting-sources "project"` + `.claude/skills/…` | *not run* | see §5 |

M5 (`--settings '<json>'`) was not run: it is reserved in TASK.md for the case where M3 and M4 both
fail, and M3b succeeded.

Two details from M3 vs M3a that T-18 needs and that no `--help` text states:

- **Layout decides whether a plugin's skills exist at all.** `/tmp/s1-only-two` held `alpha/SKILL.md`
  and `beta/SKILL.md` at its root — exactly the shape TASK.md's setup snippet builds. The plugin loaded
  (it appears in `init.plugins` as `s1-only-two@inline`) and contributed **nothing**. Moving the same
  two files to `skills/alpha/SKILL.md` and `skills/beta/SKILL.md` registered both. The `skills/` level
  is mandatory; a plugin dir that omits it fails silently, with a green-looking `plugins` entry.
- **Skills arrive namespaced by the directory's basename.** They came back as `s1-plugin-a:alpha` and
  `s1-plugin-a:beta`, not `alpha`/`beta`. Whatever T-18 names the generated directory becomes a user-visible
  prefix, and T-13's checkbox labels have to survive it.

## 3. The two questions TASK.md asks to settle next to the number

**Is the mechanism per-session?** Yes. Both flags are per-invocation; nothing was written to
`~/.claude`, and the generated directory lives outside the project. This is what keeps the answer out
of `not-possible` — a mechanism that required rewriting `~/.claude/skills` would be a global mutation
of the user's machine, and the verdict would be `not-possible` however good the number looked.

**Does the generated directory need `.claude-plugin/plugin.json`?** **No** — not for skills, on 2.1.233.
`/tmp/s1-plugin-a` carried no manifest at all and both skills registered, with the plugin named after
the directory basename. T-18 may still want to write one to pin a stable plugin name rather than
inheriting the basename, but it is not a precondition for the subset to work, and this spike measured
no other plugin surface (commands, agents, hooks) where it might be.

## 4. Consequence for T-13 and T-18

- **T-13 (`ui-consequence: only-these`).** The "All skills / Only these" toggle [T3 §7.1] is buildable and
  has a real handler behind it — no invariant-16 control-without-a-handler. The checkbox list must be
  built from the skills that `--setting-sources ""` actually removes; the 16 bundled ones are not
  checkbox material and should not be offered as removable.
- **T-18.** Generate the directory in §layout, pass both flags. Note the `skills/` level and the
  basename-as-namespace, because both are silent failures: the first yields a loaded plugin with no
  skills, the second yields checkbox labels that never match the names the session reports.

## 5. What this spike did not measure

**M4 (`--setting-sources "project"` over a project-local `.claude/skills/{alpha,beta}`) was not run.**
Creating any path containing `.claude` was refused by this environment's permission layer as a sensitive
file, including throwaway directories under `/tmp`. This is a gap in coverage, not a negative result:
M4 may work as well as M3b, and TASK.md explicitly wants both numbers recorded when two mechanisms
work, with the build-or-not decision left to T-18. Re-run M4 in an environment that permits writing a
`.claude` directory before T-18 chooses between them. The answer below therefore reports the mechanism
that was measured, not a claim that it is the only one.

Also out of scope by TASK.md: whether the model *uses* the subset (the observable is `skills` in
`system/init`), and how far the 16-skill floor can be pushed down.

---

```answer
verdict: generated-dir
cli: claude 2.1.233
date: 2026-08-15
control-command: claude -p "Reply with exactly: OK" --output-format stream-json --verbose --model haiku --strict-mcp-config
treatment-command: claude -p "Reply with exactly: OK" --output-format stream-json --verbose --model haiku --strict-mcp-config --setting-sources "" --plugin-dir /tmp/s1-plugin-a
control-skills: 54
treatment-skills: 18
ui-consequence: only-these
init-skills-raw: ["deep-research","s1-plugin-a:alpha","s1-plugin-a:beta","design-sync","dataviz","update-config","verify","debug","code-review","simplify","batch","fewer-permission-prompts","doctor","loop","schedule","claude-api","run","run-skill-generator"]
layout: |
  /tmp/s1-plugin-a/              # basename becomes the skill namespace: s1-plugin-a:alpha
    skills/                      # mandatory level; without it the plugin loads and registers nothing
      alpha/
        SKILL.md                 # front-matter: name, description
      beta/
        SKILL.md
  # no .claude-plugin/plugin.json was present and both skills registered
```
