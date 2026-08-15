# S-2 — Do `--max-turns` and `--max-budget-usd` really stop a run?

**Status:** answered, measured
**Date:** 2026-08-15
**CLI:** `claude --version` → `2.1.233 (Claude Code)`
**Raw probe artifacts:** `runs/S-2/s2-control.jsonl`, `runs/S-2/s2-turns.jsonl`, `runs/S-2/s2-budget.jsonl`

---

## 0. The answer in one paragraph

**Both flags are accepted by the parser and both really stop the run.** Against a control that took
two turns and cost $0,0194 on the identical prompt, `--max-turns 1` ended at
`subtype:"error_max_turns"`, `terminal_reason:"max_turns"` and `--max-budget-usd 0.001` ended at
`subtype:"error_max_budget_usd"`, `terminal_reason:"budget_exhausted"`. Neither capped run ever
produced the final assistant message the control produced; both exited `1` where the control exited
`0`. So `decision: use-both` and `agent-field: turns+budget` — the agent editor may carry both
fields, and each has a real mechanism behind it (invariant 16 is satisfied, not dodged).

**The one thing that would have made this answer wrong:** `num_turns` does **not** tell the capped
run from the uncapped one. The control reported `num_turns: 2`; the run under `--max-turns 1` also
reported `num_turns: 2`. Read §2.

---

## 1. Level 1 and 2 — the T1-versus-T4 dispute, settled and set aside

**Advertised in `--help`?** Split, and this is the whole of the dispute [ARCHITECTURE §11]:

```
$ claude --help | grep -E -- '--max-turns|--max-budget-usd'
  --max-budget-usd <amount>             Maximum dollar amount to spend on API
                                        calls (only works with --print)
```

`--max-budget-usd` is documented, `--max-turns` is not — exactly what the T1 fact-check said, and
it means T4's reading ("not in `--help`" → "not a CLI flag") was sound about the help text and wrong
about the CLI. `--help` is a documentation surface, not a parser dump.

**Accepted by the parser?** Both, by T1 §3.2's method (pass the flag with no value; `argument
missing` means the parser knows the token, `unknown option` means it does not):

```
$ claude -p --max-turns 2>&1 | head -2
error: option '--max-turns <turns>' argument missing

$ claude -p --max-budget-usd 2>&1 | head -2
error: option '--max-budget-usd <amount>' argument missing
```

Both `argument missing`. That settles level 2 and settles nothing else: a parser that knows a token
and a run that obeys it are different claims, and in a log they look the same.

## 2. Level 3 — the control run, and the number that lies

Three runs, same directory (`/tmp/s2-work`), same minute, same prompt, same model. The prompt is
chosen so the model *must* take two turns: it calls a tool, then has to speak again carrying the
tool's output.

```bash
P="Use the Bash tool to run: echo hello. Then tell me exactly what it printed."
COMMON=(-p "$P" --output-format stream-json --verbose --model haiku
        --strict-mcp-config --setting-sources "" --allowedTools Bash
        --permission-mode bypassPermissions)

claude "${COMMON[@]}" < /dev/null > /tmp/s2-control.jsonl                     # exit 0
claude "${COMMON[@]}" --max-turns 1 < /dev/null > /tmp/s2-turns.jsonl         # exit 1
claude "${COMMON[@]}" --max-budget-usd 0.001 < /dev/null > /tmp/s2-budget.jsonl  # exit 1
```

The `result` event of each, fields quoted verbatim from the jsonl:

| run | `subtype` | `is_error` | `terminal_reason` | `num_turns` | `total_cost_usd` | `stop_reason` |
|---|---|---|---|---|---|---|
| control | `success` | `false` | `completed` | **2** | **0.019432800000000004** | `end_turn` |
| `--max-turns 1` | `error_max_turns` | `true` | `max_turns` | **2** | 0.0037404000000000005 | `tool_use` |
| `--max-budget-usd 0.001` | `error_max_budget_usd` | `true` | `budget_exhausted` | **1** | 0.008782100000000001 | `tool_use` |

The control satisfies both preconditions TASK.md sets: `num_turns: 2` (so a cap of one turn has
something to bite on) and $0,0194 > the $0,001 cap (so the budget probe is not measuring a run that
was under budget anyway). The costs are written with the full IEEE-754 tail the CLI emits; they are
copied, not rounded.

**`num_turns` is not the observable.** A run capped at one turn reported `num_turns: 2` — the same
number the uncapped control reported. Whatever that counter counts, it is not "turns the flag
allowed", and a supervisor that decided enforcement by comparing `num_turns` against the cap would
have concluded `--max-turns` does nothing. This is precisely the silent failure this spike was
written to catch, arriving from the opposite direction to the one TASK.md predicted: not a false
`yes` from a one-turn prompt, but a false `no` from trusting the turn counter. Recorded literally
per invariant 5 rather than reconciled into a tidier story.

**What actually differs is the shape of the stream.** The control emitted
`assistant(thinking) → assistant(tool_use) → user(tool_result) → assistant(thinking) →
assistant(text) → result`. The `--max-turns 1` run stopped after the tool result came back, with no
second assistant message and `"result": null`. The `--max-budget-usd` run stopped one step earlier
still — after `assistant(tool_use)`, before the tool result. In both, the user's question was never
answered. That is enforcement, and `terminal_reason` is where you read it.

**Two `result` subtypes nobody here had seen, written down exactly as they arrived:**

- `"subtype":"error_max_turns"` with `"terminal_reason":"max_turns"` — this is the answer to T1's
  open question 3 ("SDK docs name that subtype; not reproduced locally"). Reproduced locally,
  2026-08-15, on 2.1.233.
- `"subtype":"error_max_budget_usd"` with `"terminal_reason":"budget_exhausted"` — named in no
  document we hold. New.

T1 §8.5's mapping already handles both without a change: `subtype.starts_with("error_max")` →
`FinishReason::LimitReached`, reached only after `is_error` and `terminal_reason` have been read.
Both new `terminal_reason` values (`max_turns`, `budget_exhausted`) need to be recognised there
alongside `cancelled`; neither may be rounded to `api_error`.

## 3. The budget cap is a stopping rule, not a spending ceiling

`--max-budget-usd 0.001` produced a run that spent **0.0087821** — 8,8× the number passed to it.
The cap is evaluated between steps, so the step already in flight completes and is billed. The flag
answers "stop when this much has been spent", not "never spend more than this".

For this spike that changes nothing: the run demonstrably stopped because of the flag, which is what
`enforced` records. It matters downstream, and TASK.md puts it there deliberately: summing cost
across a workflow, and treating a number as a spend guarantee, is T-21's problem. Anyone quoting
`enforced: yes` as account protection is quoting it wrong.

## 4. Consequence for T-11 and the agent editor

`max-turns.enforced: yes` and `max-budget-usd.enforced: yes`, so — stating the invariant-16 sentence
explicitly, in the direction the measurement actually went — **the agent editor may carry a
"how many turns" field and a spend-cap field, because both have a mechanism behind them.** Had
either come back `no`, the corresponding field would not exist; `agent-field: turns+budget` is the
machine-readable form of that sentence and T-11 copies it as is.

Three things T-11 should not infer from the green result:

- **Wall-clock killing stays.** These flags are per-CLI-invocation, live on one vendor, and Codex
  has neither [T3 §7.2]. The limit a user means by "don't grind forever" is still the one Loadout
  enforces itself by killing the process group [T4 §3.3, invariant 6]. `use-both` is "also", not
  "instead".
- **A capped run is a failed run.** Exit 1, `is_error: true`, `result: null` — the user's question
  came back unanswered. The UI has to say the run hit its limit, not show an empty successful reply.
- **The spend field's label must not promise a ceiling.** See §3.

## 5. What this did not measure

Per TASK.md: wall-clock limits (T-03 + T-21), Codex (no equivalent flags), budget as account
protection (T-21), and re-probing on the next CLI release — required by T1 risk 2, and the reason
`cli:` and `date:` are in the block below. This answer is true of `2.1.233` on 2026-08-15 and of
nothing else; both flags reached this state undocumented or half-documented, and the same release
process can take them away.

---

```answer
cli: claude 2.1.233
date: 2026-08-15
control-turns: 2
control-cost: 0.019432800000000004
max-turns.accepted: yes
max-turns.enforced: yes
max-turns.evidence: |
  subtype=error_max_turns terminal_reason=max_turns is_error=true stop_reason=tool_use
  num_turns=2 total_cost_usd=0.0037404000000000005
  control on the same prompt: subtype=success terminal_reason=completed num_turns=2
  total_cost_usd=0.019432800000000004 — the capped run emitted no final assistant message
  and exited 1; note num_turns=2 under a cap of 1, so num_turns does not discriminate
max-budget-usd.accepted: yes
max-budget-usd.enforced: yes
max-budget-usd.evidence: |
  subtype=error_max_budget_usd terminal_reason=budget_exhausted is_error=true
  stop_reason=tool_use num_turns=1 total_cost_usd=0.008782100000000001
  control on the same prompt: subtype=success terminal_reason=completed num_turns=2
  total_cost_usd=0.019432800000000004 — cap was 0.001 and the run still spent 0.0087821,
  so the flag stops the run between steps, it does not bound the spend
decision: use-both
agent-field: turns+budget
```
