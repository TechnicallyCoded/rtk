# RTK Auto-Savings Rules

These rules guide RTK changes that are based on observed token waste.

## Core Principle

Optimize recurring command classes, not one-off bad invocations. If a command was too broad, RTK should help the user notice and narrow it. It should not add brittle filters that only hide the specific output from that one command.

## Decision Rules

- Prefer general-purpose filters over repository-specific, filename-specific, branch-specific, or log-line-specific behavior.
- Preserve enough signal for the user to decide the next command. Token savings are not useful if the output no longer supports action.
- Treat large output as a UX problem first. Summarize counts, top files, representative lines, and next-step suggestions before printing raw matches.
- Add verbose escape hatches when default output becomes lossy. Examples: `rg-verbose`, `grep-verbose`, or explicit flags that restore the previous fuller behavior.
- Refuse or truncate wasteful search output when it exceeds a safe threshold, and explain how to continue: narrow the path, narrow the pattern, add file globs, or use the verbose command.
- Do not special-case generated artifacts by exact path unless the rule is broadly applicable. Prefer general detection for huge single-line files, minified bundles, lockfiles, model/tokenizer files, binary-ish text, and logs.
- Rank optimization work by token impact, not annoyance. High raw output, high emitted output, frequent usage, and low savings should drive priorities.
- Keep passthrough behavior honest. If RTK cannot safely summarize a command, say so and record enough stats to make future support obvious.
- Avoid hiding failures. Errors, warnings, failed tests, and non-zero exits should remain prominent even when output is compact.
- Make summaries composable for agents. Output should be concise, stable, grep-friendly where practical, and avoid decorative bulk.

## Search Command Policy

- Default `rg` and `grep` behavior should avoid dumping a wasteful number of matching lines.
- For excessive matches, print a compact summary with total matches, matching files, top files by match count, and a few representative lines.
- Include guidance: use a narrower pattern or path first; use `rg-verbose` or `grep-verbose` only when full output is genuinely needed.
- Do not optimize for a specific accidental broad search. Optimize for the class of broad searches.

## Analytics Policy

- Add stats that reveal where tokens are spent, not only where tokens are saved.
- Track and display raw input tokens, emitted output tokens, saved tokens, savings percentage, count, and command group.
- Provide project-scoped views when possible so optimization decisions can be made from the relevant workspace.
