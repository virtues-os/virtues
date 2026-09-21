---
paths:
  - "apps/web/src/**/*.svelte"
  - "apps/web/src/**/*.ts"
  - "apps/web/src-tauri/ui/*.html"
  - "applets/**/manifest.toml"
  - "applets/**/face/*.html"
  - "virtues-core/src/api/**/*.rs"
  - "virtues-core/src/server/**/*.rs"
  - "services/virtues-atlas/src/email.rs"
  - "apps/mac-source/Sources/*.swift"
  - "apps/web/plugins/reach/ios/*.swift"
  - "docs/**/*.md"
---

# UI copy

This file loads because the file you opened can carry prose a person reads on
a screen: a hint, an empty state, an error banner, an applet description, a
pairing screen, an email, a manual page. The full section is
`agents/build/voice.md` § UI copy. This is the part you must not get wrong.

**The register is not the founder's letter's.** In-app prose is warm, plain,
and concise: second person, a sparing "we" where Virtues acts, sentences of
about twenty words, the reason kept but said plainly. There is no house voice
above this — the letter is Adam's, the wiki has its own law, the assistant has
a persona line in code. A settings page that narrates its own design history
has drifted.

**Four sentence shapes — these change the most lines:**

1. **The reader is the subject, not the software.** "Turn on file sharing to
   reach the box from your laptop", never "Virtues lets you…". *Allow*, *lets
   you*, *enable*, *capability*, *functionality* all mean the subject is wrong.
2. **No "we" in a failure.** "Couldn't reach the box", not "We're having
   trouble reaching the box". The only "we" that stays is the claim about the
   company: "We can't read it."
3. **An error names the fix, not the fault.** "Choose a password of at least
   eight characters", not "That password is too short". No *oops*, no bare
   "Invalid input". Put it beside the thing that failed.
4. **A setting says what it does when on.** The off case is inferred.

**Mechanics.** Active, verb-first, **sentence case everywhere** (headings,
buttons, labels), contractions on. Fragments take no trailing period; full
sentences keep theirs. Serial comma; no exclamation points. **Hyphens, not em
dashes, in UI strings.** **"Computer", not "Mac"** on anything a PC user can
reach (the Mac app may say "Mac"). **One name per thing on every screen** —
recovery phrase, Server ID, applet, Standing, Balance, Wallet activity,
on-device, sidecar, face, pairing, relay; vendors in their own capitalization.
American spelling. Turn on/off not enable/disable; enter not type; choose for
menus, select for objects; after not once; whether for two outcomes; because
not since; to not in order to. **Don't give the box a mind** — the assistant
is a character and may think; the box, the record, and an applet do not want
or try.

**Claims — check these before the line exists:**

1. A line that states a behavior, benefit, or guarantee is verified against
   the shipping build. If the build doesn't do it, delete the line. Never
   polish a false claim.
2. A line about where data physically lives trades clarity for accuracy,
   never the reverse. Unreadable-but-true stays and gets flagged.
3. Where the app could appear to change or lose the person's data, one plain
   line says what happened and that the data is safe.

**Not copy — do not rename in a copy pass.** Button labels, section headers,
and applet `display_name` are identifiers with mirrors (sidebar, URL, tests,
docs); a rename is a code change that lands as a bundle. Error codes
(`not_linked`), log lines, env vars, column names are contracts. The human
sentence beside an error code is fair game; the code is not. CLI and installer
output have their own vocabulary and are not covered here.
