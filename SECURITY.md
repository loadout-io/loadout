# Security

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub's
[security advisory](https://github.com/loadout-io/loadout/security/advisories/new) form rather
than by opening a public issue. Include what you did, what happened, and what you expected — a
minimal reproduction is worth more than a severity score.

You can expect an acknowledgement within a few days. If a fix ships, the advisory will credit you
unless you ask otherwise.

## What is in scope

Loadout runs coding agents against your own projects on your own machine, so the interesting
boundaries are:

- **Process supervision** — a cancelled or timed-out run must terminate its whole process group,
  and Loadout verifies the group is dead rather than assuming it.
- **Prompt and secret handling** — prompts and credentials travel through stdin, never through
  command-line arguments, where any local process could read them from the process table.
- **Child environments** — a spawned agent receives an environment rebuilt from an explicit
  allowlist, not an inherited copy of yours.
- **Write scopes** — two steps that would write to the same place are refused before the first
  process starts.
- **Vendor event parsing** — an unknown event from an agent CLI is recorded and ignored; it must
  never crash a run or be executed.

## What is not a vulnerability

Loadout is a tool for running agents you have chosen against code you control. An agent doing
something destructive because you gave it permission to is the product working as designed, not a
security flaw. The same goes for a workflow you wrote that runs a harmful command.

## Supported versions

The most recent release receives fixes. Older versions do not.
