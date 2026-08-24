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
	// The long version, for anyone the figure just convinced.
	const MANIFESTO_URL = "https://virtues.com/library/manifesto";
</script>

<div class="letter">
	<div>
		<h1 class="ob-h1 hero">A small correction to technology</h1>

		<!-- A face carries an argument about trust in a way a paragraph cannot,
		     which is exactly the job of this screen. -->
		<div class="film" role="img" aria-label="A short film from the founder — coming soon">
			<div class="film-inner">
				<Icon icon="ri:play-circle-line" width="30" />
				<span>A word from the founder</span>
			</div>
		</div>

		<div class="body">
			<!-- A BELIEF, THEN A QUESTION, in one breath. The list is three
			     CATEGORIES, no names — a category cannot be argued with, and a
			     name invites an argument about that name, which this paragraph
			     cannot afford. -->
			<p>
				I'm Adam Jace, and I started Virtues because I believe in subsidiarity — the old
				idea that a thing belongs at the most local level able to hold it. Nothing is more
				local than your own life. So ask plainly: who would you rather have holding the
				whole account of your life — a government, a political party, big tech? Or you.
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
				Virtues rests on a simple premise: you are capable of holding the data of your
				life, and it is yours by right. It is the most intimate thing you have, and it can
				be used against you — in ads, algorithms, and addictions. Vice is repetitive and
				profitable, which is why so much is arranged to produce it. Virtue asks for harder
				things — attention, memory, honesty, intimacy.
			</p>

			<!-- WHAT VIRTUES IS — named by its artifacts, never its features. The
			     record, the page, the wiki, the intelligence that has read it: the
			     things a person would point at, not the modules we would. (Chat is
			     implied by the asks below; the portrait introduces itself later in
			     the flow, which is better theater than announcing it here.) The
			     daily page stays the anchor image — a concrete object at a
			     concrete time, a promise you can check rather than an IOU. -->
			<p>
				Virtues is the complete record of a life: yours. Every day it writes a page —
				yesterday, written down: where you went, who you spoke with, what you were working
				on. Around the pages grows a wiki of your world, the people and places and work
				your days are made of; an intelligence reads all of it. Thin at first, having only
				just met you. But a record compounds: give it time and it forms an honest opinion
				of how you actually live — your habits, your rhythms, the story you are in — and
				you can ask:
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
			<!-- THE CLOSE. The asks end on product; the letter must end on the
			     person. The frame is REMEMBERING, not being-known — the system as
			     a mirror and a way to search your own life (anamnesis). "Growing
			     older", not "growing up": the aphorism is about the accumulating
			     life, and "growing up" pointed at childhood — the one season
			     when discovery genuinely outweighs memory. The last sentence
			     inverts the manifesto's diagnosis — technology strips away the
			     part of life that has nothing to do with it — into technology's
			     one proper use. The us/you shift is deliberate: the founder
			     joins the reader for the final clause. -->
			<p>
				The goal is to know yourself more intimately than any company or any model ever
				will — a mirror for everything you had let slip, a way to search your own life.
				Growing older is less about discovering the new than remembering what you already
				knew. That is the only way technology should be used: to give back the part of
				life that has nothing to do with it — to make us more human, and more virtuous.
			</p>
		</div>

		<!-- The long-form case (subsidiarity, where the record lives, what is
		     borrowed) moved out of the letter entirely — it is the manifesto's
		     to make. One quiet pointer for whoever wants it now. -->
		<p class="further">
			<a href={MANIFESTO_URL} target="_blank" rel="noreferrer">
				Our case for subsidiarity in technology
				<Icon icon="ri:external-link-line" width="13" />
			</a>
		</p>

		<div class="sign">
			<!-- MASKED, NOT INVERTED. An earlier version inverted black ink assuming
			     a dark ground, which would have painted white on white for the eight
			     LIGHT themes — starting with pemberley, the one a new box actually
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
			<button class="ob-btn" onclick={onbegin}>
				Begin
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

	/* SAME TYPE AS THE LETTER, muted rather than smaller — a caption's way of
	   staying a caption. */
	.further {
		font-family: var(--font-serif, Georgia, serif);
		font-size: var(--t-body);
		line-height: 1.7;
		color: var(--color-foreground-muted);
		margin: 2.25rem 0 0;
	}

	.further a {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		color: var(--color-foreground-muted);
		text-decoration: none;
		border-bottom: 1px solid var(--color-border);
		padding-bottom: 2px;
	}

	.further a:hover {
		color: var(--color-foreground);
		border-bottom-color: var(--color-foreground-subtle);
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
