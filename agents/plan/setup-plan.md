# Setup — what is left

Setup is one flow at `routes/(onboarding)/setup/[[step]]` (steps in
`lib/components/setup/steps/`): Welcome, the letter, Account, Server, Wi-Fi,
Names, Subscription, then Connections, Timeline and Interview (each
skippable), then the ∴ and the app. On an unpaired iPhone (bbb70792) and Mac
(7df58cf9) the first half runs from the app's own copy; a second device joins
through the connect page (c436001b); once someone is in the app, a skipped step
opens as a tab (9329c6d2). A server goes by "Virtues NNNN" before it has an
owner and "<name>'s server" after (0135ba1f). Shipped in PR #100.

How the flow works is in [onboarding.md](../build/onboarding.md#setup). Its four laws are there too. Delete this file when the list below is empty.

## Open

- **Cleanup (slice 4).** `apps/web/src-tauri/ui/connect.html` is still ~2,500
  lines. It is what Windows, Linux and Android open unpaired, what every
  recovery screen is, and where a joining device goes (Setup's first half only
  sets up new servers). Target: the join path moves into Setup, recovery ("can't
  reach your server", "doesn't recognize this device") becomes small screens in
  the baked app, and one compiled-in page stays only for when the baked app
  itself fails to load. Delete with it the airlock's own sign-in and the
  server's getting-started chat room (`virtues-core/src/api/getting_started.rs`,
  its seed and tools) once nothing reads it.
- **Verify on hardware.** A first real iPhone Bluetooth pair through Setup, and
  a real pair and hand-off to the server's origin on a Mac.
- **Explain Bluetooth before the system prompt.** Today the OS sheet is the
  first mention of Bluetooth anywhere in Setup. One screen in our own words
  before it (from the [establishing-screen specimen](specimens/establishing-screen/README.md)).
- **Where the subscription is offered** — unresolved. Setup makes Subscription
  a required step with no skip; `connect.html` and the open-relay record say
  setup never mentions money and the subscription is offered the first time
  hosted AI is used. Decide once; tracked in [account-plan.md](account-plan.md).
- **Paying outside the US.** US storefront iPhone apps may link out to web
  checkout, which is what Subscription does. Anywhere else needs in-app
  purchase or a regional arrangement. Decide before the iOS app is sold
  outside the US.
- **Tagline** for Welcome. `Hello.svelte` shows "Intelligence, made personal."
  The candidate settled for the establishing screen on 2026-09-15 is "The most
  intimate record of your life, finally yours."
- **Hiding the Setup tile** while steps are still skipped: where "hide" lives,
  and whether it is per device or per server.
- **The music** (`static/onboarding/hello.mp3`) stays out of git: its licence
  grants nothing about publishing the master, and the repo is public. Ship it
  from a private path.

## Not started, needs something first

- **Phone and server in lockstep** (the server's screen following the words
  as they are typed): needs the Bluetooth session to report typed words, a
  wire change.
- **Answering the interview out loud**: needs a transcription route for the
  web app. Until then touch screens point at the keyboard's dictation.
- **Your city as the drawing**: needs an image route and storage, and a
  drawing that gets the city wrong costs more than it delights. Decide before
  building.
