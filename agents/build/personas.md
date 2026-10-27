# Personas

These are the five people Virtues is designed for. Every surface is reviewed
against them: chat first, and pages, the sidebar, settings and setup the same
way. The tiers decide which persona sets the defaults when two of them want
different things.

## The five

| Tier | Persona | What they want, in their words |
|---|---|---|
| Primary | Narrative seeker | "I want answers about my life written with care, in words." |
| Primary | Quantified self | "I want charts and numbers from my own data, right in the reply." |
| Secondary | Tech prosumer | "I want to find something new and beautifully polished before anyone else, with features worth showing off and posting about." |
| Tertiary | Recovery | "I want a chat that remembers my progress and setbacks, and helps me understand what drives them. Streaks and games don't help me." |
| Tertiary | Data sovereign | "I want to see when work happens on my server and when it leaves it." |

## How to use them

- **The two primaries set the defaults.** What a surface shows first, what
  a reply looks like, and what an empty state offers are decided by the
  narrative seeker and the quantified self. Prose and charts from the
  person's own data are both first-class answers, and a surface that only
  serves one of them is unfinished.
- **The secondary and tertiary personas are served without diluting the
  primaries.** Polish and features worth showing are for the tech prosumer.
  Memory of progress and setbacks, with no streaks, counts, red badges or
  game mechanics, is for recovery. A visible line between work on the box
  and work sent off it is for the data sovereign. Each of these goes where
  it does not crowd out the primary defaults, usually one level down.
- **A design review names all five.** For the surface under review, say what
  each persona wants from it, then rank fixes with the primaries first.
  [`tasks-plan.md`](../plan/tasks-plan.md) has a "What each persona gets"
  table that shows the shape.
- **Recovery is a constraint on everyone's surfaces.** Streaks, overdue
  alarms, nag counts and gamified progress are left out of every page, since
  any of them can land on someone in recovery.
- **The data sovereign's claims come from code.** A label saying where work
  ran must be read off the request path. The claims we may and may not make
  are in [privacy-boundary.md](privacy-boundary.md).

Don't add a sixth persona, rename these, or reorder the tiers. Those are the
owner's decisions; if a design seems to need a new persona, raise it.
