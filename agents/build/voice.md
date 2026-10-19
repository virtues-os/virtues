# Voice

> Three things, none of them a voice: the **claim rules** every surface obeys,
> the **register for in-app copy**, and the **bank** of lines written for the
> letter and kept for the website, the film, and the colophon.
>
> **There is no house voice, and this file no longer claims one.** Until
> 2026-09-21 the header announced "the one voice for every surface where
> Virtues speaks: a perceptive friend who has read the data and refuses to
> flatter you." That line was a description of how the founder's letter should
> feel, written during its August rewrite, and it was promoted into a style by
> accident. Walk the surfaces and nobody named Virtues ever speaks: the letter
> is Adam's, signed and first person; the day page and the articles are the
> record read back, governed by [wiki-editor.md](wiki-editor.md) and the day
> prompt; the life document is the person's own words; the assistant is a
> named character whose line lives in `agent/prompt.rs`; the website is the
> company's, and outside this repo. Everything else is copy. A stylized voice
> over all of that gave UI strings the letter's length and cleverness, and
> nothing else.
>
> What survives is discipline about claims, one plain register for the
> product's furniture, and a file of good lines.

## Claim rules

Settled during the 2026-08 letter rewrite; they apply to every sentence on
every surface — the letter, copy, the manual, the assistant's turns. These are
not tone. They are about what a sentence is allowed to assert.

- **Headings are titles, not sentences** — no trailing periods.
- **Artifacts, not features.** Name what a person would point at (the record,
  the page, the wiki, the portrait), never the modules we would (pipelines,
  models, slots). **"Applets" is the exception, and it is settled**: it is the
  shipping, user-facing name for the thing — it is what the room is called in
  the sidebar and what `/applets` serves. It was renamed to "Routines" on
  2026-09-16 on the strength of this rule and renamed straight back the same
  day. Do not "fix" it again.
- **Categories, not names**, when naming an adversary — a category cannot be
  argued with; a name invites an argument about the name.
- **"Cannot", not "will not"** wherever it is true — incapability by
  construction is the one claim no cloud company can copy.
- **"Data" vs "the record":** their word when it is being weaponized (ads,
  algorithms, addictions extract *data*); our word when it is being kept (you
  hold *the record*). The letter uses both, on purpose, on opposite sides.
- **The ∴ mark is logic before it is a logo.** When it appears in prose it must
  actually mean *therefore* — premises above, conclusion after.
- **Show the architecture, don't assert the virtue.** "Stays on your box. We
  can't read it." — never "your privacy matters to us."

Two rules belong to the letter alone, because it is signed by a person:

- **One command per document.** The letter permits itself exactly one
  imperative. Everything else states, shows, or asks.
- **No corporate "we"** in a document signed by a person.

## UI copy

Settled 2026-09-14 (the Billing pass — "I don't like much of your current copy
and tone and voice") and 2026-09-21 (Paul Henry's copy principles, the voice
rules of which are folded in here; his scope and process lists stay his).
In-app prose — settings pages, hints under controls, empty states, status and
error text, applet descriptions, pairing screens, Atlas emails — is the
product's furniture: read mid-task, on the way to something else. The tell that
it has drifted: a settings page narrating its own design history, or a hint
that reads like a paragraph of the letter.

**Register.** Warm and plain, and concise. Second person; a sparing, confident
"we" where Virtues itself acts ("We can't read it"). Sentences medium and
conversational — one turn each, about twenty words. Keep the reason: the page
still says *why*, in plain words, not as a story. No flourish, no performed
enthusiasm. The stance is Apple, Cursor, virtues.com.

**Clear, then short, then characterful — in that order.** Nielsen Norman's
ranking, and it settles what yields when they pull against each other: a few
extra words are right when they buy understanding, and character is the thing
that goes first, not clarity. "Concise" here means *nothing spare*, never
*fewer words than the meaning needs*. An agent told only to be brief cuts the
reason and leaves a label.

**Tone varies; the register doesn't.** Apple's rule, and the right one: the
words are the same everywhere, the temperature follows the situation. Light
where something went right, straight where something went wrong, never cute at
either end. A finished backup may sound pleased. A failed payment says what
happened and what to do.

**Mechanics** — Apple's, with two overrides:

- Active, verb-first. **Sentence case everywhere** — headings, buttons, labels,
  menu items. Apple asks you to pick one style per element type and hold it;
  we hold one for all of them, and the SPA already does. Contractions on.
- **Over 25 words, split it.** GOV.UK's trigger, and unlike "about twenty" it
  is testable. Twenty is the target; twenty-five is the line where you stop
  and break the sentence in two.
- **A button names its outcome, in the person's intent.** Verb plus object,
  never the mechanism and never a bare acknowledgement: "Write my chapters",
  not "Submit"; "Delete 3 pages", not "OK".
- Fragments (titles, hints under a control, chips) take no trailing period.
  Full sentences keep theirs.
- Serial comma. No exclamation points. Capitalize the first word after a colon
  when a full sentence follows.
- **Hyphens, not em dashes, in UI strings** — even where the em dash would be
  correct. The letter, the manual, and this workshop keep theirs.
- **"Computer", not "Mac"**, on any surface a PC user can reach: the SPA, the
  box's own screens, the manual. Vague beats wrong. The Mac desktop app may
  say "Mac"; it runs nowhere else.
- **One name per thing, the same on every screen.** *Recovery phrase*, *Server
  ID*, *applet*, *Standing*, *Balance*, *Wallet activity*, *on-device*,
  *sidecar*, *face*, *pairing*, *relay*. Vendors in the vendor's own
  capitalization: Stripe, Postgres, Radxa, Qualcomm, Anthropic, OpenAI,
  Google, Ollama, LM Studio.
- American spelling, in UI and in comments.

### The sentence shapes

**A passive sentence is a missing subject. Name it, or hand the sentence to
the person.** This is the first rule because it is the one our own copy keeps
breaking, and because in this product the missing subject is never innocent.
"Your account is written." Written by whom? Answer *the server* and you have
contradicted the title of the page it sits on, "In your own words". Answer
*you* and the sentence is both true and active. The passive was the copy
dodging the product's central claim about who authored the thing.

So: where the claim is that a thing is theirs, **the person is the actor**.
Where the server actually did something, including failing, **name the
server**. A sweep on 2026-09-21 found roughly one string in ten passive across
the SPA and the API's error text; these four are the worked examples, all from
the interview's close.

| Was | Is |
|---|---|
| What you said is arranged in two places. Both are yours: the machine never rewrites them, and anything to add or correct is done on the page. | These are your words, in two places. Nothing rewrites them but you, and you can edit either page whenever you like. |
| Your partition of the life, a page each. | Every chapter you named, a page each. |
| Not written this time. The document is safe. | Your server couldn't write the chapters. Everything you said is safe. |
| No page yet — it is written when the interview is closed. | No page yet. Finish the interview to create it. |

Note what the last two do: one names the server because the server is what
failed, and the other hands the verb to the person. "The machine" is not one
of the two names (see the Words list); it was the passive wearing a noun.

The rest, in order of how much of our copy they change:

- **The reader is the subject, not the software.** "Turn on file sharing to
  reach the box from your laptop", never "Virtues allows you to…" or "This
  setting lets you…". *Allow*, *lets you*, *enable*, *capability*, and
  *functionality* are all signs the sentence has the wrong subject.
- **No "we" in a failure.** "Couldn't reach the box", never "We're having
  trouble reaching the box" — who is *we* to someone whose server is in their
  own house? The one "we" that stays is the claim about the company itself:
  "We can't read it."
- **An error is two parts: what happened, then how to fix it.** Both present,
  or it isn't finished. "Choose a password of at least eight characters" beats
  "That password is too short"; instruct rather than scold. Put the message
  beside the thing that failed. No *oops*, no *uh-oh*, no bare "Invalid
  input". If words can't rescue an error most people will hit, the interaction
  is wrong, not the sentence.
- **Say the consequence before anything irreversible**, in the same breath as
  the action, not after it. The interview's close writes the document once and
  retires the room, and said so nowhere until it was over. A person cannot
  consent to a door they did not know was one-way.
- **A setting says what it does when it's on.** The person infers the off
  case. Add a sentence under the label only when the label can't carry it, and
  link to a setting rather than describing where it lives.
- **An empty state** points at the next action and holds nothing that matters,
  because it disappears. **Possessives are sparing** — "Projects", not "Your
  projects" — and the perspective doesn't switch mid-screen.

**Words.** Turn on / turn off, not enable / disable. Choose for menu items,
select for objects. Enter, not type or input. Quit, not exit. Cancel, not kill.
Appears, not displays. After, not once. Whether when there are two outcomes, if
for a condition. Because, not since. Want, not wish. To, not in order to. By,
with, or through, not via. For example, not e.g. And so on, not etc. Rewrite
and/or. *Can* is ability, *may* is permission, *might* is possibility — which
is why **"cannot"** carries so much weight for us, and why it is only ever used
where the incapability is real. GOV.UK's vague-verb list is banned outright:
*deliver*, *impact*, *leverage*, *utilise*, *streamline*. So is schema
vocabulary on a screen — *partition*, *entity*, *primitive*, *provenance*,
*payload*, *endpoint*, *instance*, *surface* — which is the glossary leaking
into the product.

**Don't give the box a mind.** The assistant is a named character and may think,
notice, or wonder. The box, the record, an applet, and the software do not want,
try, believe, or feel. Apple takes a passive sentence over an anthropomorphic
one, and so do we. Define an acronym on first use or don't use it. Humor lives
in examples, if anywhere.

**Claims.** Three rules that run before a line is written or rewritten, because
review catches a bad line and misses a missing one:

- **A line that states a behavior, benefit, or guarantee is checked against
  the shipping build.** If the build doesn't do it, the line is deleted, not
  polished. "Your data stays on the server" was cut the day it stopped being
  true; a cleaner-sounding false claim is still a false claim.
- **A line about where data physically lives trades clarity for accuracy,
  never the reverse.** If the accurate wording is unreadable, leave it standing
  and flag it — do not simplify it into something vaguer.
- **A screen where the app could appear to change or lose the person's data
  says, in one plain line, what happened and that the data is safe.** "I can't
  show them again. Your data is still here." The absent reassurance is the
  thing reviews miss.

**What this section does not govern.** Names — a button label, a section
header, an applet's `display_name` — are identifiers with a human face;
renaming one is a code change with mirrors (sidebar, URL, tests, docs) and
lands as a bundle or not at all. Error codes (`not_linked`, `vault_unreadable`),
log lines, env vars, and column names are contracts, not copy; the human
sentence beside a code is fair game, the code is not. The CLI and installer
print through their own vocabulary (∴ ✓ · ⚠ ✖) and are not covered here.

## The name, reframed (2026-08-24)

Virtues does not mean "use technology to live virtuously." **Virtues change
prudently over time — in both their mores and their requirements — and today,
owning your own record is a virtue in itself.** The name claims that keeping
custody of your life's data is among the virtues *of this age*, the way
temperance or thrift were of theirs. "It is a virtue of our age to own your
tech."

This is the deeper reading and should inform naming, marketing, and any future
"why the name" copy.

## The bank

Lines written for the founder's letter thesis slot, kept for other surfaces.
The letter ran *∴ You must protect your life's data to protect your soul.* until
2026-09-08, when the thesis slot was cut: the ledger moved into the margin and
the close became Herbert's Dune line, so the letter no longer has a summit of
its own. The thesis and the old close join the bank.

| Line | Crux | Likely surface |
|---|---|---|
| ∴ You must protect your life's data to protect your soul. | the thesis, as the letter ran it 2026-08-21 → 09-08 | manifesto, film script |
| Technology has exploited you long enough. This is what it was always supposed to do: make us more human, and more virtuous. | the letter's close 2026-08-24 → 09-08 | about page, film script |
| ∴ No one is free whose inner life is someone else's asset. | freedom / political | website, manifesto |
| ∴ What does it profit a man to gain the whole internet and lose his own story? | scriptural echo (Mark 8:36) | film script, essay |
| ∴ Keeping your own record is the first virtue of the digital age. | the name's argument | website hero, "why the name" |
| ∴ Anything that knows you this well must belong to you. | the intimacy condition | product pages, reveal colophon territory |
| ∴ Your life is not raw material. | anti-extraction, six words | merch-grade; social |
| ∴ The record of a life belongs where the life is lived. | subsidiarity as hearth-truth | packaging, panel ambient |
| ∴ A life unrecorded scatters; a life recorded, and owned, endures. | memory / legacy | the Examen surface, essays |
| ∴ Whoever holds the record of your life holds a hand on your soul. | custody warning | film script |
| We are bringing the cloud home. | the structural program | website hero (the honest home of the corporate "we") |
| Most of a life is lost not to anyone's malice, but to nobody writing it down — and the ordinary days, it turns out, were the beautiful ones. | the beauty of the record | reveal, Examen, film |
| The goal is to know yourself more intimately than any company or model ever will. | the competitive claim | website hero |
| A mirror for everything you had let slip; a way to search your own life. | mirror / search | product pages |
| Growing older is less about discovering new things than remembering what you already knew. | anamnesis | the Examen essay's opening line |
| Technology has one good use: to give back the part of life that has nothing to do with technology — to make us more human, and more virtuous. | the peroration | film script, about page |
| Every day, a page will be waiting for you: yesterday, written down. | the daily gift | ASSIGNED: the reveal's door (the tomorrow-beat), where "tomorrow" is real |

## Quotables (2026-08-24, the voices exercise)

Written in the styles of the letter's ancestors; original phrasings unless
flagged. The letter itself is finished — these are for the website, the film,
the manifesto rewrite, and in-product epigraphs.

**The keepers (Adam-loved):**

- *The trouble with data is not that it is collected, but that it is collected
  by everyone except its owner.* — (Chestertonian) — website problem-section;
  press-ready aphorism.
- *This day, honestly seen, is material enough for virtue. Write to yourself,
  for yourself — no other reader was ever needed.* — (Marcus Aurelius; "to
  yourself" = his own title, Ta eis heauton) — strongest candidate for an
  IN-PRODUCT epigraph: the reveal colophon or the day-page empty state.
- *The line between good and evil runs through every human heart — and
  strangers have been mapping your half of it for profit.* — (first clause is
  REAL Solzhenitsyn, Gulag Archipelago; the turn is ours. If used publicly,
  frame as allusion or credit him) — film script.
- *The most revolutionary act available to an ordinary man is an accurate
  record of his own life, because every power now in existence would prefer he
  didn't keep one.* — (Orwellian, original) — website hero or the manifesto
  rewrite's opening.

**Worth keeping warm:** A man's life is the one estate he should refuse to
rent (Chesterton) · the devils harvest the unnoticed hours / nothing dismays
them like a man who keeps accounts (Screwtape) · the homely house and the lit
window are worth all the towers of the wise (Tolkien) · virtue is a habit;
habit is built of particular acts; particular acts are forgotten unless
recorded (Aquinas) · rest begins where a man sits down with his own life and
reads it without lying (Augustine) · call your days to account each evening;
the man who audits his life owns it (Seneca) · all of man's misery comes from
his inability to sit quietly in his own room — and an industry arose to keep
him anywhere else; sit down in your room, your life is in there (Pascal — his
room IS the box, possibly the best undiscovered frame) · no one — least of
all himself — was keeping the minutes (Thoreau) · bring your days home; the
scale of care is the household (Wendell Berry) · own the house (Bond) · be
attentive, be intelligent, be reasonable, be responsible (Lonergan's actual
transcendental precepts — could structure the Examen) · a fragment of Adam's:
"the moderns have built a machine that makes predictable vice out of a man's
…" (unfinished, keep the scent).

Banked film-script material (spoken-to-camera register, too hot for print):
"evil dwells in men pretending to be good" · the extraction litany (taxes,
data, time, energy) · "you have no idea what people do with your data right
now, and I can promise you it's not good things" · "disordered men" (survives
in speech; cut from the letter's list 2026-08-24).

## Epigraphs (added 2026-09-04)

Quotations for the colophon slot the page grammar defines
([design-grammar.md](design-grammar.md) §2): one per page, one per day, set in
the serif with the attribution in the margin. Provenance matters here more
than anywhere — a line shown as a quotation must be one, or be marked as ours.

| Line | Provenance | Likely surface |
|---|---|---|
| The trouble with data is not that it is collected, but that it is collected by everyone except its owner. | ours, Chestertonian in shape | website problem section; press-ready aphorism |
| This day, honestly seen, is material enough for virtue. Write to yourself, for yourself — no other reader was ever needed. | ours, after Marcus Aurelius (*Ta eis heauton*, "to himself", is his own title) | **in-product**: the reveal colophon, the day page's empty state, Getting Started's foot |
| The line between good and evil runs through every human heart — and strangers have been mapping your half of it for profit. | first clause is Solzhenitsyn, *The Gulag Archipelago*; the turn is ours. **In public, frame as allusion or credit him** | film script |
| The most revolutionary act available to an ordinary man is an accurate record of his own life, because every power now in existence would prefer he didn't keep one. | ours, Orwellian in shape | website hero; the manifesto's opening |
