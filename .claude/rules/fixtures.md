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
prompt. Each is public. Data from the owner's real record has leaked into files like these
eight different ways, and every time it took a history rewrite to get it back
out. The reason given each time was one of "it's just a sample", "it's
anonymized", "it's shaped like a real day" or "the owner said it was fine".
None of those reasons makes the data invented.

## The rules

1. **Invent every example here, for this file.** Never copy, trim, translate,
   paraphrase or "anonymize" anything from the owner's record: no message,
   transcript line, email, chat turn, page, day article, wiki note, calendar
   title or memory. If a sentence reads naturally because a real person
   wrote it, it is a leak.
2. **Never build from a real day, box, tester or week.** No fixture, seed, demo
   or prompt example is "based on", "shaped like" or "inspired by" a real
   person's data, and no comment, doc or commit message says one is. A prompt
   example that describes a life describes an invented one. The owner's
   habits, family, health, places and plans in a prompt, even paraphrased with
   the names changed, leak the owner's persona.
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
6. **Describe the failure class instead of the incident.** A comment or doc that needs
   an example of what went wrong describes the shape ("a seed carried real
   timings shifted two weeks"), never the data.
7. **Nobody's consent makes real data acceptable in the repo.** No commit, comment or doc may say the
   owner approved real data in a tracked file.

## How this is enforced

No hook or CI job checks fixtures for real data, so you have to: read every value
you add against the rules above before committing. When unsure whether
something is invented, replace it with something that plainly is.
