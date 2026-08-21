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
		<h1 class="ob-h1 hero">A small correction to technology.</h1>

		<!-- A face carries an argument about trust in a way a paragraph cannot,
		     which is exactly the job of this screen. -->
		<div class="film" role="img" aria-label="A short film from the founder — coming soon">
			<div class="film-inner">
				<Icon icon="ri:play-circle-line" width="30" />
				<span>A word from the founder</span>
			</div>
		</div>

		<div class="body">
			<!-- A BELIEF, THEN A QUESTION, in one breath. The list is CATEGORIES
			     rather than names: a category cannot be argued with, and a name
			     invites an argument about that name — which is the one thing this
			     paragraph cannot afford. The last item is the exception that
			     proves it: it describes rather than names, so the reader supplies
			     the face themselves, which lands harder and will still read the
			     same in ten years. -->
			<p>
				I'm Adam Jace, and I started Virtues because I believe in subsidiarity — the old
				idea that a thing belongs at the most local level able to hold it. Nothing is more
				local than your own life. So ask it plainly. Who would you rather have holding the
				whole account of yours: a government, a political party, big tech, an advertising
				company, disordered men? Or you.
			</p>

			<!-- The turn from the question to the product, and the only place the
			     name gets explained. "Vice is repetitive and profitable" is the
			     load-bearing line: it says why an entire industry is arranged the
			     way it is without accusing anyone of malice, and it makes the name
			     an argument rather than a mood. -->
			<p>
				That is why I built Virtues: to put your digital life somewhere it cannot be turned
				into advertising, or a feed, or a habit you did not choose. Vice is repetitive and
				profitable, which is why so much is arranged to produce it. Virtue asks for harder
				things — attention, memory, honesty, intimacy.
			</p>

			<!-- SHOWN, NOT ASSERTED. Everything above is an argument, and a page of
			     pure argument is cold. A concrete object at a concrete time is a
			     thing you can picture, and "every day" is a promise you can check
			     rather than an IOU. -->
			<p>
				Every day, a page will be waiting for you: yesterday, written down — where you went,
				who you spoke with, what you were working on. Thin at first, having only just met
				you. But give it a week, and it becomes Jarvis. You can ask it things like:
			</p>
		</div>

		<!-- THE ONLY PLACE THE PRODUCT SPEAKS FOR ITSELF.
		     Five questions carry more than five paragraphs of capability claims,
		     and the RANGE is the argument: a symptom, a stranger's name, a craft,
		     a song, a fast. No single app holds all five today, and nothing but
		     your own record could.

		     Set as a block rather than bullets — these are things you would say
		     out loud, and a bulleted list turns speech into a feature grid. Full
		     ink, because they are the payoff of the page rather than an aside. -->
		<ul class="asks">
			<li>Why do I have a migraine today?</li>
			<li>What was the name of the woman I met at the dog park yesterday?</li>
			<li>How do I become a better writer?</li>
			<li>What song was playing at the bar yesterday that made me nostalgic?</li>
		</ul>

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
