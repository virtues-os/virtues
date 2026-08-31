<!--
  The letter — the first screen anyone sees after their box is theirs.

  ONE JOB. Make someone believe the box is theirs, and tell them what happens
  next. An earlier version had nine movements, a two-panel figure, a second
  figure, and six body sizes; every piece was individually defensible and the
  whole was a brochure. What survived is what does that one job.

  TWO FEELINGS, one beat each:

    understood     a person wrote this, and names the thing you actually fear
    excited        tomorrow morning, one concrete object at a concrete time

  The architecture argument (where the record lives, what is borrowed, why
  there is a subscription) does NOT live here anymore — the two-marker figure
  and its bridge paragraph were cut 2026-08-21. The letter states the belief
  and the promise; the case is the manifesto's to make, and the film's when it
  exists. A letter that pauses to draw a diagram stops being a letter.

  TWO SIZES, ONE FAMILY. Serif for the headline and the whole body, one small
  size for the sign-off chrome. Nothing else. The old page ran six body sizes
  and the mixture was the reason it felt unresolved before anyone could say
  why. (The mono label register left with the figure; the strip above the
  letter still carries it.)
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	// THE SHELL IS THE ROUTE'S. This used to own its own `.ob-wrap`/`.ob-sheet`
	// and mount the progress strip itself, which made the strip a different
	// element on every screen — so it animated in with the content beneath it.
	// The route now mounts one header above one animated slot, and this is only
	// the leaf that goes in the slot.
	let { onbegin }: { onbegin: () => void } = $props();

	// Up here because they are the likeliest thing on this page to rot, and a
	// reachable founder is the claim the page rests on — a dead link here costs
	// more than a typo anywhere else in onboarding.
	const EMAIL = "adam@virtues.com";
	const X_URL = "https://x.com/adamjaces";
	const INSTAGRAM_URL = "https://www.instagram.com/aajaces/";
</script>

<div class="letter">
	<div>
		<h1 class="ob-h1 hero">A small correction to technology</h1>

		<!-- THE FILM IS OUT UNTIL IT EXISTS. A face carries an argument about
		     trust in a way a paragraph cannot — but a play button that plays
		     nothing reads as broken, and real users arrive today. Restore this
		     block (and its .film CSS below) when the film ships:

		<div class="film" role="img" aria-label="A short film from the founder">
			<div class="film-inner">
				<Icon icon="ri:play-circle-line" width="30" />
				<span>A word from the founder</span>
			</div>
		</div>
		-->


		<div class="body">
			<!-- A BELIEF, THEN A QUESTION, in one breath. One continuous motion —
			     belief, principle, question — no stage directions ("So ask
			     plainly:" was a command to ask a question the letter asks
			     anyway) and no fragment drumroll: the question's last word is
			     "you?", and ¶2's "yours by right" is its answer. The list is
			     three CATEGORIES, no names — a category cannot be argued with,
			     and a name invites an argument about that name, which this
			     paragraph cannot afford. -->
			<p>
				I'm Adam Jace, and I started Virtues because I believe in subsidiarity — the old
				idea that a thing belongs at the most local level that can hold it. Nothing is more
				local than your own life. So who should hold the whole account of it: a government,
				a political party, big tech, or you?
			</p>

			<!-- THE PREMISE. Two claims: capability (you can hold this yourself)
			     and title (it is your property — "yours by right", the
			     private-property register without the legalism of "entitled").
			     The against-triple names what extraction becomes; the FOR side is
			     deliberately not stated here — the rest of the letter (the page,
			     the wiki, the asks, the mirror) IS the for side, shown rather
			     than listed. The triple chains into vice-is-repetitive, which
			     explains it. -->
			<p>
				Virtues rests on a simple premise: the data of your life is yours by right, and
				yours to hold. It is the most intimate thing you have, and today it is turned
				against you — into ads, algorithms, and addictions. Vice is repetitive and
				profitable, which is why so much is arranged to produce it. Virtue asks for harder
				things — attention, memory, honesty, intimacy.
			</p>

			<!-- THE LEDGER. ¶1 called it "the whole account of your life", so the
			     figure is drawn as an account: one rule across, one rule down,
			     a debit column and a credit column. Not a diagram — a page from
			     a book of accounts, which is a form with five centuries of
			     editorial standing and no AI-slop associations. The entries
			     pair one-to-one ACROSS the rule, each pair the same raw
			     material bent opposite ways: ads↔self-knowledge (they study
			     you to sell; you study yourself for yourself), algorithms↔
			     memory (their machine processes your past to shape you; yours,
			     to remind you), addictions↔virtue (the couplet above, drawn).
			     One ink for both columns — the words carry the judgment. -->
			<figure
				class="ledger"
				role="img"
				aria-label="The account of your life, as a table. In their cloud: ads, algorithms, addictions. In your home: self-knowledge, memory, virtue."
			>
				<p class="origin">your life's data</p>
				<div class="table">
					<div class="row heads">
						<p class="where">in their cloud</p>
						<p class="where">in your home</p>
					</div>
					<div class="row">
						<p class="entry">ads</p>
						<p class="entry">self-knowledge</p>
					</div>
					<div class="row">
						<p class="entry">algorithms</p>
						<p class="entry">memory</p>
					</div>
					<div class="row">
						<p class="entry">addictions</p>
						<p class="entry">virtue</p>
					</div>
				</div>
			</figure>

			<!-- THE THESIS. The one large claim, introduced by the mark itself —
			     ∴ is the therefore-sign, so the logo arrives doing its job as
			     LOGIC: everything above it, the figure included, is premises,
			     and this line is the conclusion. It is also the only sentence
			     in the letter allowed to command. Same ink as the prose — the
			     mark is salient by being a symbol, not by being colored. -->
			<p class="thesis">
				<span class="therefore" aria-hidden="true">∴</span> You must protect your life's
				data to protect your soul.
			</p>

			<!-- THE GIFT. What protecting the record BUYS, right after the thesis
			     commands you to protect it: command, then gift. The image is the
			     WIKIPEDIA OF YOUR LIFE — instantly graspable, browsable, always
			     growing — and the list escalates from logged facts (went, spoke,
			     worked) through an inferred pattern (the places you go when
			     you're happy) to meaning (the stories that matter most), which
			     DEMONSTRATES "a record compounds" instead of asserting it. The
			     daily-page line ("a page will be waiting for you") moved to the
			     reveal's door, where tomorrow is real. The asks below are this
			     paragraph's proof, and the letter ends on them — the ∴ is the
			     one summit; a closing essay-paragraph was a second one, cut
			     2026-08-24 (its lines are banked in agents/build/voice.md). -->
			<p>
				Every day, Virtues writes the wikipedia of your life: where you went, who you
				spoke with, what you were working on, the places you go when you're happy, the
				stories that matter most. Thin at first, having only just met you. But a record
				compounds. Give it time, then ask:
			</p>
		</div>

		<!-- THE ONLY PLACE THE PRODUCT SPEAKS FOR ITSELF.
		     Four questions, four time horizons — today, yesterday, the
		     standing ledger, years — each needing a different stream of the
		     record (the body, the ambient moment, the transactions, the
		     message history). Every ask must be unanswerable without the
		     record: a question any bare model handles ("how do I become a
		     better writer?") is a question this list cannot afford. Third is
		     the practical one so the list still ends on the emotional deep
		     cut, which the reveal's "oldest thing it found" line later pays
		     off.

		     Set as a block rather than bullets — these are things you would say
		     out loud, and a bulleted list turns speech into a feature grid. Full
		     ink, because they are the payoff of the page rather than an aside. -->
		<ul class="asks">
			<li>Why do I have a migraine today?</li>
			<li>What was the name of the woman I met at the dog park yesterday?</li>
			<li>What am I still paying for that I never use?</li>
			<li>Who have I lost touch with that I used to talk to every day?</li>
		</ul>

		<div class="body">
			<!-- THE LAST WORD. Two plain sentences after the asks — the grievance
			     and the turn. The hard words (subsidiarity, the ledger) did
			     their work upstairs; the close is the line anyone could repeat
			     at dinner. The you→us shift between the sentences is the
			     founder stepping in beside the reader for the final clause,
			     which lands on the cadence every cut spared: more human, and
			     more virtuous. (The essay-close and the manifesto link were
			     both cut 2026-08-24 — the essay was a second summit after the
			     ∴, the link an exit ramp to a cloud-era document the ledger
			     contradicts; its lines are banked in agents/build/voice.md.) -->
			<p>
				Technology has exploited you long enough. This is what it was always supposed to
				do: make us more human, and more virtuous.
			</p>
		</div>

		<div class="sign">
			<!-- MASKED, NOT INVERTED. An earlier version inverted black ink assuming
			     a dark ground, which would have painted white on white for the eight
			     LIGHT themes — starting with oxford, the one a new box actually
			     opens on. Masking paints the ink in whatever the theme's foreground
			     is, correct on all sixteen with no list to maintain. -->
			<div class="sig" role="img" aria-label="Adam Jace"></div>
			<p class="role">Founder, Virtues</p>

			<div class="contacts">
				<!-- BUNDLED MARKS, NOT FAVICONS. Fetching icons from x.com and
				     instagram.com would make the page that promises nothing leaves
				     your box reach out to two ad companies and tell them you opened
				     it. The icon registry pre-imports everything, so these cost no
				     network at all. -->
				<a class="pill" href="mailto:{EMAIL}">
					<Icon icon="ri:mail-line" width="15" />
					{EMAIL}
				</a>
				<a class="pill" href={X_URL} target="_blank" rel="noreferrer">
					<Icon icon="ri:twitter-x-fill" width="14" />
					adamjaces
				</a>
				<a class="pill" href={INSTAGRAM_URL} target="_blank" rel="noreferrer">
					<Icon icon="ri:instagram-line" width="15" />
					aajaces
				</a>
			</div>
		</div>

		<div class="exit">
			<!-- The letter is the whole of onboarding now, so its close is the
			     door itself — everything that used to follow lives on Home as
			     getting-started sections. -->
			<button class="ob-btn" onclick={onbegin}>
				Enter Virtues
				<Icon icon="ri:arrow-right-line" width="16" />
			</button>
		</div>
	</div>
</div>

<style>
	/* THE WHOLE TYPE SYSTEM. Three sizes, and every rule below refers to these
	   rather than inventing its own — which is the specific failure the previous
	   version accumulated one defensible exception at a time. */
	.letter {
		--t-body: 1.0625rem;
		--t-small: 13px;
	}

	/* The one screen allowed to be louder than the others: it is the cover, and
	   everything after it is an interior page. */
	.hero {
		font-size: clamp(2rem, 4vw, 2.75rem);
		line-height: 1.05;
		letter-spacing: -0.02em;
	}

	/* Commented out with the film block above — restore together.
	.film {
		margin: 2rem 0 0;
		aspect-ratio: 16 / 9;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: color-mix(in srgb, var(--color-foreground) 3%, transparent);
		display: grid;
		place-items: center;
	}

	.film-inner {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		color: var(--color-foreground-subtle);
		font-size: var(--t-small);
	}
	*/

	/* A LETTER, SO IT IS SET LIKE ONE. Serif throughout at a single size — the
	   old page reserved the serif for two "important" paragraphs, which made the
	   rest look like interface chrome someone had to get past. */
	.body {
		margin-top: 2.25rem;
		display: flex;
		flex-direction: column;
		gap: 1.15rem;
		font-family: var(--font-serif, Georgia, serif);
		font-size: var(--t-body);
		line-height: 1.7;
		color: var(--color-foreground);
	}

	.body p {
		margin: 0;
	}

	/* ── the ledger ────────────────────────────────────────────────────── */

	/* The one figure in the letter, set as a booktabs table: an inset exhibit
	   at well under the measure, three horizontal rules (above the heads,
	   below the heads, under the last row), and NO vertical rules — columns
	   are separated by whitespace, which is the whole discipline of the form.
	   Serif entries in the prose's own face; mono only for the heads. The
	   debit column is muted; each pair reads across its row as an opposition. */
	.ledger {
		margin: 1.5rem 0 1rem;
	}

	.origin {
		margin: 0 0 0.85rem;
		text-align: center;
		font-family: var(--font-serif, Georgia, serif);
		font-size: var(--t-body);
		color: var(--color-foreground);
	}

	.table {
		max-width: 25rem;
		margin: 0 auto;
		border-top: 1px solid var(--color-border);
		border-bottom: 1px solid var(--color-border);
	}

	/* Equal halves — the heads are a true parallel now (in their cloud / in
	   your home), so the geometry gets to be one too. */
	.row {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 2rem;
	}

	.row p {
		margin: 0;
	}

	.row.heads {
		padding: 0.6rem 0 0.55rem;
		border-bottom: 1px solid var(--color-border);
		margin-bottom: 0.55rem;
	}

	.row:last-child {
		padding-bottom: 0.65rem;
	}

	/* Lowercase mono, gentle tracking: an annotation's whisper, not a column
	   header's shout. */
	.where {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 11.5px;
		letter-spacing: 0.06em;
		color: var(--color-foreground-subtle);
	}

	/* One ink for both columns — in print, the judgment is carried by the
	   words (addictions vs virtue), never by graying a side out. The earlier
	   muted-left read as a disabled state, not an opinion. */
	.entry {
		font-family: var(--font-serif, Georgia, serif);
		font-size: var(--t-body);
		line-height: 1.85;
		color: var(--color-foreground);
	}

	/* The thesis line: same serif, same size, same ink — salience comes from
	   the mark and the surrounding air, never from shouting or color. */
	.thesis {
		margin-top: 0.35rem;
	}

	.therefore {
		margin-right: 0.15rem;
	}

	/* Questions someone would say out loud, so they are set as speech: no
	   markers, a hairline to hold them together as one utterance, and the same
	   serif at the same size as the prose they interrupt. Bullets would have
	   made them a feature grid, which is the one thing they must not read as. */
	.asks {
		margin: 1.4rem 0 0;
		padding: 0 0 0 1.3rem;
		border-left: 1px solid var(--color-border);
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		font-family: var(--font-serif, Georgia, serif);
		font-size: var(--t-body);
		line-height: 1.45;
		color: var(--color-foreground);
	}

	.asks li {
		margin: 0;
		text-wrap: pretty;
	}

	/* The bridge paragraph carries on the letter, so it keeps the body's rhythm
	   rather than starting a new block. */
	.asks + .body {
		margin-top: 1.5rem;
	}

	/* ── sign-off ──────────────────────────────────────────────────────── */

	.sign {
		margin-top: 2.75rem;
	}

	.sig {
		height: 4.6rem;
		width: 16.9rem;
		background-color: var(--color-foreground);
		opacity: 0.85;
		-webkit-mask: url("/images/adam_signature.png") no-repeat left center / contain;
		mask: url("/images/adam_signature.png") no-repeat left center / contain;
	}

	.role {
		margin: 0.45rem 0 0;
		font-size: var(--t-small);
		line-height: 1.6;
		color: var(--color-foreground-subtle);
	}

	.contacts {
		margin-top: 0.9rem;
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.pill {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.35rem 0.7rem;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		font-size: var(--t-small);
		color: var(--color-foreground-muted);
		text-decoration: none;
		transition:
			background 0.15s ease,
			color 0.15s ease;
	}

	.pill:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
	}

	/* The signature ends the letter; the button is the way out of it, not the
	   last line of the sign-off. A rule separates the two. */
	.exit {
		margin-top: 3rem;
		padding-top: 2.25rem;
		border-top: 1px solid var(--color-border);
	}

	/* The button itself is .ob-btn; it sits directly under the rule, so it drops
	   the shared top margin. */
	.exit .ob-btn {
		margin-top: 0;
	}

</style>
