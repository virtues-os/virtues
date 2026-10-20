---
paths:
  - "**/*.rs"
---

# Do not swallow a query error

Loaded because you are in Rust. Every instance below shipped and went unnoticed
for weeks, because a swallowed error shows up as an ordinary number instead
of an error.

`.ok()`, `.unwrap_or(0)` and `.unwrap_or_default()` directly on a `fetch_*`
result turn a broken query into a plausible number, and nothing ever surfaces.
Each of these happened in this codebase: it is why sleep read "0.0 hours", why every
date-scoped search returned nothing, why resting heart rate was a hardcoded
62.0, and why the box reported zero paired devices on every box forever. Use
`?`. If you genuinely mean "absent is fine", say so in a comment naming what
absence means.
