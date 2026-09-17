---
paths:
  - "**/*.rs"
---

# Do not swallow a query error

Loaded because you are in Rust. Every instance below shipped and was believed
for weeks, because a swallowed error does not look like an error — it looks
like a number.

`.ok()`, `.unwrap_or(0)` and `.unwrap_or_default()` directly on a `fetch_*`
result turn a broken query into a plausible number, and nothing ever surfaces.
That is not hypothetical: it is why sleep read "0.0 hours", why every
date-scoped search returned nothing, why resting heart rate was a hardcoded
62.0, and why the box reported zero paired devices on every box forever. Use
`?`. If you genuinely mean "absent is fine", say so in a comment naming what
absence means.
