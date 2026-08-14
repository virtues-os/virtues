<!--
  The letter — the first screen anyone sees after their box is theirs.

  ITS OWN SURFACE, deliberately. Onboarding used to open with this as the first
  chapter of a long scroll, and a scroll is a reading grammar being used for a
  working session: read, then act, then wait, then read again, with no signal
  about which mode you are in. The reading gets a screen so it can be read.

  WRITTEN FOR SOMEONE WHO HAS ALREADY DECIDED. The first version of this page
  reused the /about letter, which is good writing aimed at a different person —
  someone choosing. This one has chosen, paid, and plugged in a box that knows
  nothing about them. They are not curious yet; they are checking whether they
  were had. So the page does two jobs: confirm they chose well, and set the
  frame for the fact that in four minutes it will ask for their iMessages.

  THE LIMITATION IS THE CENTREPIECE, not a caveat. Saying what Virtues CANNOT do
  on the day someone paid is the most credible thing available here, and no
  competitor will say it. It also disarms the subscription entirely: it stops
  being a compromise to defend and becomes the consequence of a fact stated
  first. Everything below it is read as true because of it.

  THE DOUBLE BLIND IS A DIAGRAM because the claim is architectural. Prose can
  only ASSERT that two records share no key; two columns with nothing between
  them shows it. The sentence under it is kept exactly as written — "honest,
  careless, breached, subpoenaed, acquired, or dead" is the whole argument in
  one line, and it survives because it names failure modes a promise cannot.

  NO CAPABILITY TEASE. Nothing here about what you will be able to ask it. The
  box knows nothing yet, so every example would be an IOU; that material belongs
  at the END of onboarding where it is a payoff.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	let { onbegin, reduced = false }: { onbegin: () => void; reduced?: boolean } = $props();

	// Up here because they are the most likely thing on this page to be wrong,
	// and a reachable founder is the claim the page rests on — a dead link here
	// costs more than a typo anywhere else in onboarding.
	const EMAIL = 'adam@virtues.com';
	const X_URL = 'https://x.com/adamjaces';
	const INSTAGRAM_URL = 'https://www.instagram.com/aajaces/';
</script>

<div class="letter" class:still={reduced}>
	<div class="sheet">
		<p class="kicker">Virtues Onboarding</p>
		<h1>A small correction, to technology.</h1>

		<!-- Placeholder. A face carries an argument about trust in a way a
		     paragraph cannot, which is exactly the job of this screen. -->
		<div class="film" role="img" aria-label="A short film from the founder — coming soon">
			<div class="film-inner">
				<Icon icon="ri:play-circle-line" width="30" />
				<span>A word from the founder</span>
			</div>
		</div>

		<div class="body">
			<p>
				I am Adam Jace; I started Virtues. The accounts below are mine, and I answer them
				myself.
			</p>

			<!-- BUNDLED MARKS, NOT FAVICONS. Pulling icons from x.com and
			     instagram.com would make this page — the one promising nothing
			     leaves your box — fetch from two ad companies and tell them you
			     opened it. The icon registry pre-imports everything for exactly
			     this reason, so the pills cost no network at all. -->
			<div class="contacts">
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

			<!-- SHOWN, NOT ASSERTED. Every other beat on this page is a negation —
			     what we cannot do, do not see, cannot join — and a page of pure
			     defence is cold. One concrete object at a concrete time does what
			     no list of features would: it is a thing you can picture. Kept to
			     tomorrow morning rather than some eventual capability, because a
			     promise you can check within a day is not an IOU. -->
			<p>
				Tomorrow morning a page will be waiting for you: yesterday, written down — where you
				went, who you spoke with, what you were working on. It will be thin at first, having
				only just met you. Give it a month and it will know your weeks better than you do;
				give it three years and you will be able to ask about an ordinary Tuesday and
				receive an answer.
			</p>

			<p>
				There is one thing worth saying plainly on the day you have paid for this, and it is
				what Virtues cannot yet do.
				<strong
					>No intelligence you can run in your own house is good enough to be trusted with
					your life</strong
				> — not honestly, not yet — and anyone who tells you otherwise is either selling
				something or has not tried it. So your box borrows better models than it can host,
				and that is what the subscription buys. When the hardware in your home is equal to
				the work, the subscription ends. It is the only part of Virtues that is not already
				yours.
			</p>

			<p>
				The rest already is. Your record — messages, calendar, places, health — lives on the
				box in your home and does not leave it. The models are shown only what a question
				requires, under terms that keep nothing: not stored, not trained on, not logged.
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

				<!-- The severed join: a dashed line that stops short of both sides. -->
				<line class="cut" x1="260" y1="16" x2="260" y2="94" />
				<text class="gap" x="260" y="132">no shared identifier</text>
			</svg>
			<figcaption>
				Joining the two returns an empty table. That holds whether we are honest, careless,
				breached, subpoenaed, acquired, or dead.
			</figcaption>
		</figure>

		<!-- THE SHAPE OF THE NEXT HOUR. This page ended on a privacy diagram and a
		     button, which told someone what to believe and nothing about what to
		     do. Naming the three steps turns a threshold into a path — and being
		     honest that the last one takes a while is what stops it feeling like
		     a trick halfway through. -->
		<div class="next">
			<p class="next-head">What happens now</p>
			<ol>
				<li>
					<span>Connect this Mac.</span> It already holds years of your messages, and it
					needs nothing but your permission.
				</li>
				<li>
					<span>Add whatever else you want it to know.</span> Your phone, your calendar,
					your mail. Any of it, in any order, whenever.
				</li>
				<li>
					<span>Tell it who you are.</span> Fourteen questions that nothing else can
					answer for you. It takes a while, and it saves as you go.
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

	/* The one line that is a claim rather than a sentence. */
	.mission {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.25rem;
		color: var(--color-foreground);
		margin: 0.4rem 0 !important;
	}

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

	/* Dashed and short of both boxes: it reads as a join that was never made,
	   rather than a wall between two things that would otherwise touch. */
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

	/* MASKED, NOT INVERTED. The signature is black ink with an alpha channel, and
	   the first version inverted it on the assumption of a dark ground — which
	   would have rendered white ink on white for the eight LIGHT themes,
	   starting with pemberley, the one a new box actually opens on.
	   Masking paints the ink in whatever the theme's foreground is, so it is
	   correct on all sixteen without anyone maintaining a list of which are
	   dark. */
	.sig {
		height: 3rem;
		width: 11rem;
		background-color: var(--color-foreground);
		opacity: 0.85;
		-webkit-mask: url('/images/adam_signature.png') no-repeat left center / contain;
		mask: url('/images/adam_signature.png') no-repeat left center / contain;
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
