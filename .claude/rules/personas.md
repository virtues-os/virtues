---
paths:
  - "apps/web/src/**"
  - "apps/desktop/src/**"
  - "docs/**"
  - "virtues-core/prompts/**"
  - "virtues-core/src/agent/**"
  - "applets/**/SKILL.md"
---

# Check the change against the five personas

Loaded because you are editing something a person sees or reads: the app,
the manual, or a prompt that shapes what the assistant says. The personas
and their tiers are in CLAUDE.md under "Who Virtues is for" and in full in
[`agents/build/personas.md`](../../agents/build/personas.md).

Before you finish the change, answer these in your report to the owner:

1. **Which persona does it serve?** If you can't name one, ask whether the
   change belongs in the product.
2. **What does it do to the primaries?** For the narrative seeker, check
   whether answers are still written in careful prose. For the quantified
   self, check whether their own numbers and charts are still in the reply
   or on the page. The change must not make either default worse.
3. **Does it add a streak, count, badge, overdue alarm or game mechanic?**
   If yes, take it out. Someone in recovery uses the same pages.
4. **Does it send work off the box?** If yes, the surface shows that it did.
   The claims allowed are in
   [privacy-boundary.md](../../agents/build/privacy-boundary.md).
5. **Is it polished enough to show someone?** That is the tech prosumer's
   bar. A rough version of a primary feature still comes before a polished
   extra.
