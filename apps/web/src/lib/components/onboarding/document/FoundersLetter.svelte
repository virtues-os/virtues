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
  why. (The mono label register survives only as the headword of a margin
  note and the heads of the ledger.)

  THE MARGIN (2026-09-08). Two things left the column for the margin: the
  definition of subsidiarity, which used to be a clause inside the first
  sentence, and the ledger, which used to sit between paragraphs as an
  exhibit. Both are annotations ON the letter rather than sentences OF it,
  and an editorial margin is the form that says so — the reader's eye can
  take them or leave them without the sentence breaking stride. On a narrow
  window there is no margin, so the notes fold back into the column as
  indented asides; the prose is identical either way.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";

	// THE SHELL IS THE ROUTE'S. This used to own its own `.ob-wrap`/`.ob-sheet`
	// and mount the progress strip itself, which made the strip a different
	// element on every screen — so it animated in with the content beneath it.
	// The route now mounts one header above one animated slot, and this is only
	// the leaf that goes in the slot.
	// `beginLabel` names what the one button does where the letter is read:
	// "Begin setup" as Setup's preface, "Close the letter" when re-read.
	let { onbegin, beginLabel = "Begin setup" }: { onbegin: () => void; beginLabel?: string } = $props();

	// Up here because they are the likeliest thing on this page to rot, and a
	// reachable founder is the claim the page rests on — a dead link here costs
	// more than a typo anywhere else in onboarding.
	const EMAIL = "adam@virtues.com";
	const X_URL = "https://x.com/adamjaces";
	const INSTAGRAM_URL = "https://www.instagram.com/aajaces/";

	// The signature writes itself the first time it comes into view.
	let sigEl = $state<HTMLElement | null>(null);
	let written = $state(false);
	onMount(() => {
		if (!sigEl || !("IntersectionObserver" in window)) {
			written = true;
			return;
		}
		const io = new IntersectionObserver(
			(entries) => {
				if (entries.some((e) => e.isIntersecting)) {
					written = true;
					io.disconnect();
				}
			},
			{ threshold: 0.9 },
		);
		io.observe(sigEl);
		return () => io.disconnect();
	});
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


		<!-- TWO COLUMNS, ONE ORDER. The prose and the margin are separate
		     containers so that, on a wide window, the notes flow down their own
		     column — each starting no higher than the paragraph it annotates and
		     sliding down past the note above it, the way marginalia actually
		     stack. (Locking each note to its paragraph's row opened a hole in the
		     prose wherever a note outran a short paragraph.) On a narrow window both containers
		     dissolve (display: contents) and `order` interleaves the notes back
		     under their paragraphs, so the reading order is the same either way. -->
		<div class="body">
			<div class="prose">

				<!-- THE PREMISE. Two claims: capability (you can hold this yourself)
				     and title (it is your property — "yours by right", the
				     private-property register without the legalism of "entitled").
				     The against-triple names what extraction becomes; the FOR side
				     is deliberately not stated here — the rest of the letter (the
				     ledger beside it, the wiki, the asks) IS the for side, shown
				     rather than listed. The triple chains into vice-is-repetitive,
				     which explains it. -->
				<p class="p2">
					The data of your life is yours by right, and yours to hold. It's the most
					intimate thing you have, and today it's turned against you: into ads,
					algorithms, and addictions. Vice is repetitive and profitable, which is why so
					much is arranged to produce it. Virtue asks for harder things: attention,
					memory, honesty, intimacy.
				</p>

				<!-- THE GIFT. What holding the record BUYS, straight after the
				     premise that it is yours to hold. The image is the WIKI OF YOUR
				     LIFE (the product's own word for it, 2026-09-28; it was
				     "wikipedia"), and the list escalates from logged facts (went,
				     spoke) through a pattern (the places you keep going back to) to
				     meaning (the stories that matter most). "The places you go when
				     you're happy" went the same day: nothing in the record infers
				     mood (day_summary says "no mood"), and the claim rules delete a
				     line the build can't back. "Thin at first … a record compounds.
				     Give it time, then ask:" went too, as fragments narrating a
				     motif; the paragraph now says plainly that you can ask it. The
				     daily-page line ("a page will be waiting for you") moved to the
				     reveal's door, where tomorrow is real. The asks below are this
				     paragraph's proof, and the letter ends on them — a closing
				     essay-paragraph was a second summit, cut 2026-08-24 (its lines
				     are banked in agents/build/voice.md). -->
				<p class="p3">
					Every day, Virtues writes the wiki of your life: where you went, who you spoke
					with, the places you keep going back to, the stories that matter most. You can
					ask your assistant things like:
				</p>
			</div>

			<div class="margin">

				<!-- THE LEDGER, in the margin beside the paragraph it draws from.
				     Drawn as an account: one rule across, one rule down, a debit
				     column and a credit column. Not a diagram — a page from a book
				     of accounts, a form with five centuries of editorial standing
				     and no AI-slop associations. The entries pair one-to-one ACROSS
				     the rule, each pair the same raw material bent opposite ways:
				     ads↔self-knowledge (they study you to sell; you study yourself
				     for yourself), algorithms↔memory (their machine processes your
				     past to shape you; yours, to remind you), addictions↔virtue.
				     One ink for both columns — the words carry the judgment.
				     The ∴ thesis line that used to follow it was cut 2026-09-08:
				     with the ledger off to the side there is no longer a stack of
				     premises for a therefore-sign to conclude. -->
				<figure
					class="note ledger"
					role="img"
					aria-label="The account of your life, as a table. In their cloud: ads, algorithms, addictions. In your home: self-knowledge, memory, virtue."
				>
					<p class="head">your life's data</p>
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
			</div>
		</div>

		<!-- THE ONLY PLACE THE PRODUCT SPEAKS FOR ITSELF.
		     Three questions, three time horizons — today, yesterday, years —
		     each needing a different stream of the record (the body, the
		     ambient moment, the message history). Every ask must be
		     unanswerable without the record: a question any bare model handles
		     ("how do I become a better writer?") is a question this list
		     cannot afford. It ends on the emotional deep cut, which the
		     reveal's "oldest thing it found" line later pays off. (The
		     practical one, "What am I still paying for that I never use?",
		     was cut for length 2026-09-28.)

		     Set as a block rather than bullets — these are things you would say
		     out loud, and a bulleted list turns speech into a feature grid. Full
		     ink, because they are the payoff of the page rather than an aside. -->
		<ul class="asks">
			<li>Why do I have a migraine today?</li>
			<li>What was the name of the woman I met at the dog park yesterday?</li>
			<li>Who have I lost touch with that I used to talk to every day?</li>
		</ul>

		<div class="body">
			<!-- THE FOUNDER'S LAST LINE. After the asks the letter has finished
			     arguing; what is left is why the person who wrote it started, in
			     one sentence that glosses its own hard word. It used to open the
			     letter ("I'm Adam Jace, and I started Virtues because I believe in
			     digital subsidiarity") with the definition in the margin; the
			     letter now opens on its premise and the name is on the signature
			     (2026-09-28). "I built the thing I wished existed. If it's useful
			     to you, reach out :)" was cut the same day; the contact pills
			     below say it. The grievance close ("Technology has
			     exploited you long enough…") went to the bank in
			     agents/build/voice.md on 2026-09-08; Herbert carries the grievance
			     now, below. -->
			<p class="close">
				I started Virtues because I believe in digital subsidiarity: nothing is more
				local than your own life.
			</p>
		</div>

		<!-- THE LAST WORD IS BORROWED. Herbert says the grievance from sixty years
		     off, which is the point: it is not new and not ours, and a letter
		     that ends on someone else's sentence is a letter confident enough not
		     to need the last word. Set off as a quotation, not a paragraph — the
		     asks are the reader's voice, the close is the founder's, this is a
		     third. -->
		<blockquote class="quote">
			<p>
				Once men turned their thinking over to machines in the hope that this would set
				them free. But that only permitted other men with machines to enslave them.
			</p>
			<footer>Frank Herbert, Dune</footer>
		</blockquote>

		<div class="sign">
			<!-- MASKED, NOT INVERTED. An earlier version inverted black ink assuming
			     a dark ground, which would have painted white on white for the eight
			     LIGHT themes — starting with oxford, the one a new box actually
			     opens on. Masking paints the ink in whatever the theme's foreground
			     is, correct on all sixteen with no list to maintain. -->
			<div class="sig" class:written bind:this={sigEl} role="img" aria-label="Adam Jace"><span class="ink"></span></div>
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

		<!-- The letter is Setup's preface (setup-plan.md), so it ends on one
		     button. Its P.S. about the subscription moved to the Subscription
		     step, with the question it answers. -->
		<div class="exit">
			<button class="ob-btn" onclick={onbegin}>
				{beginLabel}
				<Icon icon="ri:arrow-right-line" width="16" />
			</button>
		</div>
	</div>
</div>

<style>
	/* The one way on, under a rule where the letter ends, in the theme's
	   primary: the same object as every step's way forward. */
	.exit {
		margin-top: 2.5rem;
		padding-top: 2.25rem;
		border-top: 1px solid var(--color-border);
	}
	.exit :global(.ob-btn) {
		margin-top: 0;
		background: var(--color-primary);
		color: var(--color-background);
	}

	/* THE WHOLE TYPE SYSTEM. Three sizes (body, the margin's note, the sign-off's
	   small), and every rule below refers to these rather than inventing its own — which is the specific failure the previous
	   version accumulated one defensible exception at a time. */
	.letter {
		--t-body: 1.0625rem;
		--t-note: 0.9375rem;
		--t-small: 13px;
		/* The margin column and the gutter between it and the prose. */
		--m-width: 15rem;
		--m-gutter: 2.5rem;
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
		/* No word left alone on a paragraph's last line ("like:"). */
		text-wrap: pretty;
	}

	/* ── the margin ────────────────────────────────────────────────────── */

	/* IN THE COLUMN (no margin yet). Both containers dissolve and their
	   children become one flex column, `order` putting each note straight
	   under the paragraph it annotates. */
	.prose,
	.margin {
		display: contents;
	}

	.p2 { order: 1; }
	.ledger { order: 2; }
	.p3 { order: 3; }


	/* The note register: the prose's serif one step down, the margin's ink one
	   step lighter. Headword in the same lowercase mono as the ledger heads, so
	   the two notes read as one apparatus. In the column, an indented aside
	   with a hairline — the same device the asks use, so the letter has one
	   way of saying "beside the text". */
	.note {
		margin: 0;
		padding-left: 1.3rem;
		border-left: 1px solid var(--color-border);
		font-family: var(--font-serif, Georgia, serif);
		font-size: var(--t-note);
		line-height: 1.5;
		color: var(--color-foreground-muted);
		transition: color 0.15s ease;
	}

	.note p {
		margin: 0;
	}

	.note .head {
		margin-bottom: 0.35rem;
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 12px;
		letter-spacing: 0.06em;
		color: var(--color-foreground-subtle);
	}

	/* The ledger, set as a booktabs table: three horizontal rules (above the
	   heads, below the heads, under the last row) and NO vertical rules —
	   columns are separated by whitespace, which is the whole discipline of
	   the form. Serif entries in the note's face; mono only for the heads. */
	.table {
		max-width: 22rem;
		border-top: 1px solid var(--color-border);
		border-bottom: 1px solid var(--color-border);
	}

	/* Equal halves — the heads are a true parallel (in their cloud / in your
	   home), so the geometry gets to be one too. */
	.row {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1rem;
	}

	.row p {
		margin: 0;
	}

	.row.heads {
		padding: 0.55rem 0 0.5rem;
		border-bottom: 1px solid var(--color-border);
		margin-bottom: 0.45rem;
	}

	.row:last-child {
		padding-bottom: 0.55rem;
	}

	/* Lowercase mono, gentle tracking: an annotation's whisper, not a column
	   header's shout. */
	.where {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 12px;
		letter-spacing: 0.06em;
		color: var(--color-foreground-subtle);
	}

	/* One ink for both columns — in print, the judgment is carried by the
	   words (addictions vs virtue), never by graying a side out. The earlier
	   muted-left read as a disabled state, not an opinion. */
	.entry {
		line-height: 1.8;
		color: var(--color-foreground);
	}

	/* THE MARGIN PROPER. Opens only when the space can hold the sheet, a
	   gutter, and a 15rem margin column with room to spare on both sides:
	   38 + 2.5 + 15, doubled for symmetry, plus the wrap's padding. Below that
	   the notes stay in the column — a margin that squeezes the prose is worse
	   than no margin. The column does NOT move: it stays where the sheet
	   centers it, and the margin hangs off its right edge into the space that
	   was already empty. Marginalia are an addition to a page, not a change
	   to where the page sits.
	   THE SPACE, NOT THE WINDOW: whoever sets the letter names a `letter`
	   container (the stage, the letter page, Setup's tab in the app). Keyed
	   to the window, the notes opened in a pane beside the sidebar as if the
	   pane were the window, and hung off the sheet's edge. */
	@container letter (min-width: 76rem) {
		.body {
			display: grid;
			grid-template-columns: minmax(0, 1fr) var(--m-width);
			column-gap: var(--m-gutter);
			align-items: start;
			/* Hangs the margin past the sheet's edge. */
			margin-right: calc(-1 * (var(--m-width) + var(--m-gutter)));
		}

		.prose {
			display: flex;
			flex-direction: column;
			gap: 1.15rem;
		}

		/* Notes stack with real air between them — a margin is read in
		   glances, and two notes close together read as one. */
		.margin {
			display: flex;
			flex-direction: column;
			gap: 3.5rem;
		}

		.note {
			padding-left: 0;
			border-left: 0;
		}


		.table {
			max-width: none;
		}
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

	/* The close carries on the letter after the asks, so it keeps the body's
	   rhythm rather than starting a new block. */
	.asks + .body {
		margin-top: 1.5rem;
	}

	/* A QUOTATION, SET AS THE APP SETS ONE. The rule down its left edge is
	   the same one a `>` quote gets in any page (`--md-quote-border`), so the
	   borrowed voice reads as quoted, where an indent alone read as the
	   letter wandering. A step quieter than the prose, in the same roman
	   serif (never italic), the attribution in small type beneath. */
	.quote {
		margin: 2.5rem 0 0;
		padding: 0 0 0 1.25rem;
		border-left: var(--md-quote-border, 3px solid var(--color-border));
		max-width: 30rem;
		font-family: var(--font-serif, Georgia, serif);
		font-size: var(--t-body);
		line-height: 1.7;
		color: var(--color-foreground-muted);
	}

	.quote p {
		margin: 0;
		text-wrap: pretty;
	}

	.quote footer {
		margin-top: 0.6rem;
		font-family: var(--font-sans, system-ui, sans-serif);
		font-size: var(--t-small);
		line-height: 1.6;
		letter-spacing: 0.01em;
		color: var(--color-foreground-subtle);
	}

	.quote footer::before {
		content: "— ";
	}

	/* ── sign-off ──────────────────────────────────────────────────────── */

	.sign {
		margin-top: 2.75rem;
	}

	/* THE PEN, as two nested masks rather than two layers of one. The
	   outer is a soft-edged gradient slid across left to right, so the name
	   appears as if written: the edge is a pen's width of fade, not a wipe's
	   hard line, and the easing slows into the last stroke. The inner is the
	   ink. One element with both layers and `mask-composite: intersect`
	   drew a solid block beside the name in the Mac app's WebKit, which
	   added the layers instead of intersecting them (2026-09-28); nesting
	   intersects by construction, the way Welcome's drawing does it. */
	.sig {
		height: 4.6rem;
		width: 16.9rem;
		opacity: 0.85;
		-webkit-mask-image: linear-gradient(90deg, black 44%, transparent 56%);
		mask-image: linear-gradient(90deg, black 44%, transparent 56%);
		-webkit-mask-repeat: no-repeat;
		mask-repeat: no-repeat;
		-webkit-mask-size: 230% 100%;
		mask-size: 230% 100%;
		-webkit-mask-position: 100% 0;
		mask-position: 100% 0;
		transition:
			-webkit-mask-position 1.8s cubic-bezier(0.5, 0.1, 0.3, 1) 150ms,
			mask-position 1.8s cubic-bezier(0.5, 0.1, 0.3, 1) 150ms;
	}
	.sig.written {
		-webkit-mask-position: 0 0;
		mask-position: 0 0;
	}
	.sig .ink {
		display: block;
		width: 100%;
		height: 100%;
		background-color: var(--color-foreground);
		-webkit-mask: url("/images/adam_signature.png") left center / contain no-repeat;
		mask: url("/images/adam_signature.png") left center / contain no-repeat;
	}
	@media (prefers-reduced-motion: reduce) {
		.sig {
			transition: none;
		}
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

</style>
