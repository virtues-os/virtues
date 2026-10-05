//! The box's API version: one integer a UI can compare against.
//!
//! **Why an integer and not the release version.** A UI runs against boxes
//! older and newer than itself (phones update themselves; boxes update when
//! they update), and "does this box have what I need" is a capability
//! question. A release tag cannot answer it: prereleases, local builds and
//! `dev` all order badly, and two builds of one tag can differ. So the box
//! carries one number, Tailscale's `CapabilityVersion` pattern, served in
//! `/health` as `api_version`. A box that predates it reports nothing, which
//! every UI reads as **0**.
//!
//! **The rule that makes skew safe: the API only grows.** New request fields
//! are optional; response fields are added, never removed or retyped; renames
//! keep the old name as an alias. So a newer box never breaks an older UI, and
//! this number only has to answer the other direction: whether a UI may use
//! something new. A UI keeps its floor as `MIN_BOX_API`
//! (`apps/web/src/lib/boxApi.ts`) and shows "Your server needs an update"
//! below it. See `agents/plan/local-ui-plan.md`.
//!
//! **Raise it** when the box gains an endpoint, field or behavior a UI may
//! come to depend on, and add a row. Never lower it; never reuse a number.
//!
//! | v | change |
//! |---|---|
//! | 1 | baseline: the API as of 2026-09-29, the first box to report a version |
pub const API_VERSION: u32 = 1;
