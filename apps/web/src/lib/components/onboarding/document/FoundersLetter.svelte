<!--
  The letter — the first screen anyone sees after their box is theirs.

  ITS OWN SURFACE. Onboarding used to open with this as the first chapter of a
  long scroll, and a scroll is a reading grammar being used for a working
  session. The reading gets a screen so it can be read.

  WRITTEN FOR SOMEONE WHO HAS ALREADY DECIDED. An earlier version reused the
  /about letter, which is good writing aimed at a different reader — one who is
  choosing. This one has chosen, paid, and plugged in a box that knows nothing
  about them. Two jobs: confirm they chose well, and set the frame for the fact
  that in four minutes it will ask for their iMessages.

  THE ORDER IS THE ARGUMENT:

    who I am        a person, reachable, before any claim is made
    the mission     five words
    tomorrow        one concrete object at a concrete time — the beauty, shown
    home vs cloud   the distinction, in prose
    the figures     the same distinction, at a glance
    the limit       what we cannot do yet, and why you are paying
    the rest        what stays yours regardless
    the double blind  why that holds even if we are not good people
    what happens next  three steps, so the button is not a cliff

  The figures sit AFTER the paragraph that names the distinction, not before it.
  A diagram that arrives cold has to be decoded; one that arrives after the idea
  is simply recognised, which is the difference between studying a picture and
  seeing it.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	let { onbegin, reduced = false }: { onbegin: () => void; reduced?: boolean } = $props();

	// Up here because they are the likeliest thing on this page to rot, and a
	// reachable founder is the claim the page rests on — a dead link here costs
	// more than a typo anywhere else in onboarding.
	const EMAIL = "adam@virtues.com";
	const X_URL = "https://x.com/adamjaces";
	const INSTAGRAM_URL = "https://www.instagram.com/aajaces/";
	// The long version of the argument the figures make in a glance.
	const MANIFESTO_URL = "https://virtues.com/library/manifesto";
</script>

<div class="letter" class:still={reduced}>
	<div class="sheet">
		<p class="kicker">Virtues Onboarding</p>
		<h1>A small correction, to technology.</h1>

		<!-- A face carries an argument about trust in a way a paragraph cannot,
		     which is exactly the job of this screen. -->
		<div class="film" role="img" aria-label="A short film from the founder — coming soon">
			<div class="film-inner">
				<Icon icon="ri:play-circle-line" width="30" />
				<span>A word from the founder</span>
			</div>
		</div>

		<div class="body">
			<!-- A BELIEF, THEN A QUESTION. The first version opened by naming
			     the founder and pointing at three social accounts, which told a
			     new owner who was speaking and nothing about why. The reason came
			     four paragraphs later, buried under architecture.

			     The list is deliberately categories and one allusion rather than
			     names. Categories cannot be argued with; a name invites an
			     argument about that name, which is the one thing this paragraph
			     cannot afford. The last item does the work of the name and lets
			     the reader supply it, which lands harder and will still read the
			     same in ten years.

			     "and … and …" rather than commas inside that item, or the list
			     reads as six answers instead of four — money, an island and a
			     disordered life would each look like their own candidate. The
			     repeated conjunction binds them into one man, and "disordered" is
			     the older, better word for it: a life whose loves are out of
			     order, which is precisely the objection. -->
			<p>
				I'm Adam Jace, and I started Virtues because I believe in subsidiarity: the old idea
				that a thing belongs at the most local level capable of holding it. Nothing is more
				local than your own life.
			</p>

			<p class="challenge">
				So ask it plainly. Who would you rather have holding the whole account of yours — a
				government, a political party, an advertising company, a man with money and an
				island and a disordered life? Or you.
			</p>

			<p class="reachable">You can reach me directly. I answer these myself.</p>

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

			<p class="mission">Personal intelligence, privately.</p>

			<!-- SHOWN, NOT ASSERTED. Nearly every other beat here is a negation,
			     and a page of pure defence is cold. One concrete object at a
			     concrete time is a thing you can picture. Tomorrow rather than
			     some eventual capability, because a promise you can check within a
			     day is not an IOU. -->
			<p>
				Tomorrow morning a page will be waiting for you: yesterday, written down — where you
				went, who you spoke with, what you were working on. It will be thin at first, having
				only just met you. Give it a month and it will know your weeks better than you do;
				give it three years and you will be able to ask about an ordinary Tuesday and
				receive an answer.
			</p>

			<p>
				That is only possible if the record of your life sits somewhere you control. Almost
				everything else you use keeps it in the other place — on machines you cannot enter,
				belonging to companies whose interest in you is not the same as your interest in
				yourself. The difference is not how carefully the data is guarded. It is whose house
				it is kept in.
			</p>
		</div>

		<div class="paths">
			<div class="path now">
				<!-- THE SAME HOUSE IN BOTH FIGURES. That is the device: not two
				     worlds, but one home and two fates for what is inside it. An
				     earlier version drew five grey slabs and a fan of lines, which
				     nobody could read as "the cloud" — so the cloud is a cloud,
				     both things are labelled, and the moving parts drop to three.

				     Here everything leaves. The dots rise, cross the roof, and fade
				     into something with no inside to see. The house is left empty,
				     which is the argument. -->
				<svg
					class="fig"
					viewBox="0 0 240 158"
					role="img"
					aria-label="Your life leaving your home for servers you cannot see"
				>
					<path
						class="cloud solid"
						d="M74 56c0-11 9-20 20-20 4-11 15-18 27-16 9 1 17 7 21 15 12-2 23 7 23 19 0 11-9 20-20 20H92c-10 0-18-8-18-18z"
					/>
					<text class="tag" x="120" y="14">their servers</text>

					<g class="leaving">
						<circle cx="120" cy="112" r="3.5" />
						<circle cx="110" cy="112" r="3" />
						<circle cx="130" cy="112" r="3" />
					</g>

					<path class="home" d="M84 120 L120 96 L156 120 L156 142 L84 142 Z" />
					<text class="tag home-tag" x="120" y="154">your home</text>
				</svg>

				<p class="path-head">As it is</p>
				<p>
					Built to be watched, rather than to watch with you. Your attention sold, your
					habits shaped by people you will never meet, and the record of your life held on
					machines you cannot see, audit, or leave.
				</p>
			</div>

			<div class="path ideal">
				<!-- SEALED. Same house, same cloud, and nothing between them.
				     An earlier version drew a thread carrying a question up and
				     back, on the reasoning that the box borrows models it cannot
				     host. But this figure is about where your LIFE is kept, not
				     about every packet — the record does not leave, and drawing a
				     line out of the house says the opposite of the only thing this
				     picture exists to say. The borrowing is stated in words two
				     paragraphs down, where it can be explained rather than
				     misread. The cloud stays, faint and unattached: the same two
				     objects as the left, with the connection simply absent. -->
				<svg
					class="fig"
					viewBox="0 0 240 158"
					role="img"
					aria-label="Your life staying home; only questions leave"
				>
					<path
						class="cloud hollow"
						d="M74 56c0-11 9-20 20-20 4-11 15-18 27-16 9 1 17 7 21 15 12-2 23 7 23 19 0 11-9 20-20 20H92c-10 0-18-8-18-18z"
					/>
					<text class="tag" x="120" y="14">their servers</text>

					<path class="home held" d="M84 120 L120 96 L156 120 L156 142 L84 142 Z" />
					<g class="kept">
						<circle cx="120" cy="130" r="3.5" />
						<circle cx="103" cy="134" r="2.5" />
						<circle cx="137" cy="134" r="2.5" />
					</g>
					<text class="tag home-tag" x="120" y="154">your home</text>
				</svg>

				<p class="path-head">As it ought to be</p>
				<p>
					The whole of it in your house and open to your inspection — every process, every
					log, every byte. Intelligence that serves who you are trying to become, and lets
					you look honestly at your own life without the looking being sold.
				</p>
			</div>
		</div>

		<!-- The door out for anyone the figures just convinced, placed where the
		     appetite is rather than at the end. -->
		<p class="further">
			<a href={MANIFESTO_URL} target="_blank" rel="noreferrer">
				Our case for subsidiarity in technology
				<Icon icon="ri:external-link-line" width="13" />
			</a>
		</p>

		<div class="body">
			<!-- THE LIMITATION, and it is the most credible thing on the page.
			     Saying what Virtues cannot do on the day someone paid is what no
			     competitor will do, and it disarms the subscription: it stops being
			     a compromise to defend and becomes the consequence of a fact stated
			     first. -->
			<p>
				There is one thing worth saying plainly on the day you have paid for this, and it is
				what Virtues cannot yet do.
				<strong
					>No intelligence you can run in your own house is good enough to be trusted with
					your life</strong
				>
				— not honestly, not yet — and anyone who tells you otherwise is either selling something
				or has not tried it. So your box borrows better models than it can host, and that is
				what the subscription buys. When the hardware in your home is equal to the work, the
				subscription ends. It is the only part of Virtues that is not already yours.
			</p>

			<p>
				The rest already is. Your record lives on the box in your home and does not leave
				it. The models are shown only what a question requires, under terms that keep
				nothing: not stored, not trained on, not logged.
			</p>

			<p>And on our side, we have built it so that we cannot put you back together.</p>
		</div>

		<!-- Two columns, nothing between them. The gap IS the argument, which is
		     why it is drawn rather than described. -->
		<figure class="blind">
			<svg viewBox="0 0 520 142" role="img" aria-label="Billing and the app share no identifier">
				<g class="col">
					<rect x="1" y="1" width="228" height="108" rx="10" />
					<text class="col-title" x="20" y="30">Billing</text>
					<text class="col-item" x="20" y="54">your name</text>
					<text class="col-item" x="20" y="74">your card</text>
					<text class="col-item" x="20" y="94">what you pay</text>
				</g>

				<g class="col">
					<rect x="291" y="1" width="228" height="108" rx="10" />
					<text class="col-title" x="310" y="30">The app</text>
					<text class="col-item" x="310" y="54">an account key</text>
					<text class="col-item" x="310" y="74">nothing else</text>
					<text class="col-item" x="310" y="94">no name, no card</text>
				</g>

				<line class="cut" x1="260" y1="16" x2="260" y2="94" />
				<text class="gap" x="260" y="132">no shared identifier</text>
			</svg>
			<figcaption>
				Joining the two returns an empty table. That holds whether we are honest, careless,
				breached, subpoenaed, acquired, or dead.
			</figcaption>
		</figure>

		<!-- Three steps, so the button is a path rather than a cliff — including
		     the honest admission that the last one takes a while. -->
		<div class="next">
			<p class="next-head">What happens next</p>
			<ol>
				<li>
					<span>Connect this Mac.</span> It already holds years of your messages, and it
					needs nothing but your permission.
				</li>
				<li>
					<span>Add whatever else you want it to know.</span> Your phone, your calendar, your
					mail. Any of it, in any order, whenever.
				</li>
				<li>
					<span>Tell it who you are.</span> Fourteen questions that nothing else can answer
					for you. It takes a while, and it saves as you go.
				</li>
			</ol>
		</div>

		<div class="sign">
			<div class="sig" role="img" aria-label="Adam Jace"></div>
			<div class="role">Founder, Virtues</div>
		</div>

		<button class="begin" onclick={onbegin}>
			Begin
			<Icon icon="ri:arrow-right-line" width="16" />
		</button>
	</div>
</div>

<style>
	.letter {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 4rem 1.5rem;
	}

	.sheet {
		width: 100%;
		max-width: 42rem;
		animation: rise 0.7s cubic-bezier(0.2, 0.7, 0.2, 1) both;
	}

	.still .sheet {
		animation: none;
	}

	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	.kicker {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 11px;
		letter-spacing: 0.18em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
		margin: 0 0 0.75rem;
	}

	h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: clamp(2rem, 4vw, 2.75rem);
		line-height: 1.05;
		letter-spacing: -0.02em;
		margin: 0;
		text-wrap: balance;
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
		font-size: 13px;
	}

	.body {
		margin-top: 2rem;
		display: flex;
		flex-direction: column;
		gap: 1.1rem;
		font-size: 1.0625rem;
		line-height: 1.65;
		color: var(--color-foreground-muted);
	}

	.body p {
		margin: 0;
	}

	.body strong {
		color: var(--color-foreground);
		font-weight: 600;
	}

	/* The question is the turn the paragraph above sets up, so it gets the
	   serif and the room to be read as one. */
	.challenge {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.15rem !important;
		line-height: 1.45;
		color: var(--color-foreground);
		margin: 0.3rem 0 !important;
	}

	.reachable {
		font-size: 14px !important;
		color: var(--color-foreground-subtle);
		margin: 0.4rem 0 -0.4rem !important;
	}

	.mission {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.25rem;
		color: var(--color-foreground);
		margin: 0.4rem 0 !important;
	}

	.contacts {
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
		font-size: 13px;
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

	/* ── the two paths ─────────────────────────────────────────────────── */

	.paths {
		margin: 2.5rem 0 0;
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 2.5rem;
	}

	@media (max-width: 640px) {
		.paths {
			grid-template-columns: 1fr;
			gap: 2rem;
		}
	}

	.path p {
		margin: 0;
		font-size: 14.5px;
		line-height: 1.6;
	}

	/* BOTH HEADINGS IN FULL INK. Dimming the left one alongside its body read as
	   a rendering fault rather than a judgement — a pale serif looks like
	   something that failed to load. The contrast lives in the figures and the
	   body copy, where it is legible as intent. */
	.path-head {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.35rem !important;
		line-height: 1.15;
		color: var(--color-foreground);
		margin: 1.1rem 0 0.7rem !important;
		padding-bottom: 0.7rem;
		border-bottom: 1px solid var(--color-border);
	}

	.ideal .path-head {
		border-bottom-color: color-mix(in srgb, var(--color-primary) 55%, transparent);
	}

	.now p:not(.path-head) {
		color: var(--color-foreground-subtle);
	}

	.fig {
		width: 100%;
		height: auto;
		overflow: visible;
	}

	.tag {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 8px;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		text-anchor: middle;
		fill: var(--color-foreground-subtle);
	}

	/* Solid where your life ends up; hollow where only a question goes. The fill
	   is the difference between a place that holds you and one that does not. */
	.cloud {
		stroke: var(--color-foreground-subtle);
		stroke-width: 1;
	}

	.solid {
		fill: color-mix(in srgb, var(--color-foreground) 14%, transparent);
	}

	.hollow {
		fill: none;
		stroke-dasharray: 3 4;
		opacity: 0.55;
	}

	.home {
		fill: none;
		stroke: var(--color-foreground-subtle);
		stroke-width: 1.25;
		stroke-linejoin: round;
	}

	/* The kept house is the only filled shape here, and it carries the page's
	   single accent. */
	.held {
		fill: color-mix(in srgb, var(--color-primary) 10%, transparent);
		stroke: var(--color-foreground);
	}

	.leaving circle {
		fill: var(--color-foreground-subtle);
		animation: leave 3.4s ease-in infinite;
	}

	.leaving circle:nth-child(2) {
		animation-delay: -1.1s;
	}

	.leaving circle:nth-child(3) {
		animation-delay: -2.2s;
	}

	/* Up, out, gone. It never returns and never arrives anywhere visible. */
	@keyframes leave {
		0% {
			transform: translateY(0);
			opacity: 0;
		}
		15% {
			opacity: 0.9;
		}
		70% {
			opacity: 0.45;
		}
		100% {
			transform: translateY(-66px);
			opacity: 0;
		}
	}

	.kept circle {
		fill: var(--color-foreground);
		animation: settle 7s ease-in-out infinite;
	}

	.kept circle:nth-child(2) {
		animation-duration: 9s;
	}

	.kept circle:nth-child(3) {
		animation-duration: 8s;
		animation-direction: reverse;
	}

	/* It stirs; it does not travel. */
	@keyframes settle {
		0%,
		100% {
			transform: translate(0, 0);
		}
		50% {
			transform: translate(0, -5px);
		}
	}

	/* An argument made by animation must still be legible standing still. */
	.still .leaving circle,
	.still .kept circle {
		animation: none;
	}

	@media (prefers-reduced-motion: reduce) {
		.leaving circle,
		.kept circle {
			animation: none;
		}
	}

	.further {
		margin: 1.4rem 0 0;
		font-size: 13.5px;
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

	/* ── the double blind ──────────────────────────────────────────────── */

	.blind {
		margin: 2.25rem 0 0;
	}

	.blind svg {
		width: 100%;
		height: auto;
		overflow: visible;
	}

	.col rect {
		fill: color-mix(in srgb, var(--color-foreground) 4%, transparent);
		stroke: var(--color-border);
		stroke-width: 1;
	}

	.col-title {
		fill: var(--color-foreground);
		font-size: 14px;
		font-weight: 600;
	}

	.col-item {
		fill: var(--color-foreground-subtle);
		font-size: 12px;
	}

	/* Dashed and short of both boxes: a join that was never made, rather than a
	   wall between two things that would otherwise touch. */
	.cut {
		stroke: var(--color-foreground-subtle);
		stroke-width: 1;
		stroke-dasharray: 3 5;
		opacity: 0.5;
	}

	.gap {
		fill: var(--color-foreground-subtle);
		font-size: 11px;
		letter-spacing: 0.06em;
		text-anchor: middle;
	}

	figcaption {
		margin-top: 1rem;
		font-size: 13.5px;
		line-height: 1.6;
		color: var(--color-foreground-muted);
	}

	/* ── what happens next ─────────────────────────────────────────────── */

	.next {
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--color-border);
	}

	.next-head {
		margin: 0 0 0.9rem;
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 11px;
		letter-spacing: 0.16em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
	}

	.next ol {
		margin: 0;
		padding: 0 0 0 1.2rem;
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
		font-size: 15px;
		line-height: 1.6;
		color: var(--color-foreground-muted);
	}

	.next li span {
		color: var(--color-foreground);
	}

	.sign {
		margin-top: 2.25rem;
	}

	/* MASKED, NOT INVERTED. An earlier version inverted black ink assuming a dark
	   ground, which would have painted white on white for the eight LIGHT themes
	   — starting with pemberley, the one a new box actually opens on. Masking
	   paints the ink in whatever the theme's foreground is, correct on all
	   sixteen with no list to maintain. */
	.sig {
		height: 3rem;
		width: 11rem;
		background-color: var(--color-foreground);
		opacity: 0.85;
		-webkit-mask: url("/images/adam_signature.png") no-repeat left center / contain;
		mask: url("/images/adam_signature.png") no-repeat left center / contain;
	}

	.role {
		margin-top: 0.35rem;
		font-size: 12.5px;
		color: var(--color-foreground-subtle);
	}

	.begin {
		margin-top: 2.5rem;
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		font: inherit;
		font-size: 15px;
		padding: 0.7rem 1.4rem;
		border-radius: 10px;
		border: 1px solid var(--color-border);
		background: none;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background 0.15s ease;
	}

	.begin:hover {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
	}
</style>
