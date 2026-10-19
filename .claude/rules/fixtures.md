---
paths:
  - "**/tests/**"
  - "**/test/**"
  - "**/__tests__/**"
  - "**/testdata/**"
  - "**/fixtures/**"
  - "**/*.test.ts"
  - "**/*.spec.ts"
  - "**/*_test.rs"
  - "**/*_test.py"
  - "**/test_*.py"
  - "**/seeds/**"
  - "docs/**"
  - "agents/**"
  - "**/prompts/**"
  - "**/*prompt*"
  - "applets/**/manifest.toml"
---

# Fixtures, examples and prose quote only invented text

This file loads because the file you opened holds example data or prose:
a test, a fixture, a seed, the demo, a manual page, an agents/ document, a
prompt. Each is public. The owner's real record has reached files like these
eight different ways, and every time it took a history rewrite to get it back
out. The reason given each time was one of "it's just a sample", "it's
anonymized", "it's shaped like a real day" or "the owner said it was fine".
None of those makes it fiction.

## The rules

1. **Invent every example here, for this file.** Never copy, trim, translate,
   paraphrase or "anonymize" anything from the owner's record: no message,
   transcript line, email, chat turn, page, day article, wiki note, calendar
   title or memory. A sentence that reads naturally because a real person
   wrote it is the failure, not the goal.
2. **Never build from a real day, box, tester or week.** No fixture, seed, demo
   or prompt example is "based on", "shaped like" or "inspired by" a real
   person's data, and no comment, doc or commit message says one is. A prompt
   example that describes a life describes an invented one. The owner's
   habits, family, health, places and plans in a prompt, even paraphrased with
   the names changed, are the persona leak.
3. **No measured values.** No step count, amount, heart rate, sleep length,
   duration, count of messages, places or days, date of a real event,
   coordinate or timestamp taken from the record. Not rounded, not shifted by
   a few days, not "for illustration". The guard matches real timestamps at
   every whole-day shift, real coordinates to ~11 m, and real (date, value)
   pairs.
4. **The repo carries no demo data.** Don't add a seed, a dump or a generator
   that reads `virtues_real`, `boxcopy` or a box. A future demo is built from
   pure invention, off this repo, and reviewed before anything of it lands.
5. **Use the reserved fiction.** Phones `+1512555xxxx`, mail `@example.com`,
   the stock cast (`Nick`, `David Okafor`), invented places. A real
   public place (a landmark, a city) is fine; one the owner frequents is not.
6. **Write the failure class, not the incident.** A comment or doc that needs
   an example of what went wrong describes the shape ("a seed carried real
   timings shifted two weeks"), never the data.
7. **No one can consent in the repo.** No commit, comment or doc may say the
   owner approved real data in a tracked file.

## What enforces it

The git hooks run `tools/privacy-guard.py` (the owner's record, by hash) and
`tools/check-pii.py` (contact patterns, file types) on every commit, commit
message and push, and `.github/workflows/privacy.yml` runs check-pii on every
push in CI. A refusal names the kind and the line, never the real value. Fix
it by replacing the content with fiction. Never skip a hook; a generic false
hit is accepted by hash by the owner (`tools/privacy-guard.py allow --last`).
