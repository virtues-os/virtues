<!--
  The letter — the first screen anyone sees after their box is theirs.

  ITS OWN SURFACE, deliberately. Onboarding used to open with the letter as the
  first chapter of a long scroll, and a scroll is a reading grammar being used
  for a working session: read, then act, then wait, then read again, with no
  signal about which mode you are in. The reading gets its own screen so it can
  be read; the working checklist that follows can then be a checklist.

  No table of contents, no progress, no chrome. Someone who has just spent money
  and ten minutes on a box that knows nothing about them is not curious yet —
  they are checking whether they were had. This is the answer, and it is the
  only pure reading moment in the product.

  THE WORDS ARE NOT NEW. They are the /about letter ("A small correction"),
  unchanged. Rewriting them for onboarding would have produced a weaker second
  version of an argument that already lands, and the person who signed it should
  be the person who wrote it.

  The last paragraph is load-bearing and is why the letter comes BEFORE the
  permissions rather than after: privacy as a precondition for self-knowledge is
  the frame through which the next screen's Full Disk Access prompt gets read.
  With the frame, it is the BOX being trusted. Without it, it reads as us.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";

	let { onbegin, reduced = false }: { onbegin: () => void; reduced?: boolean } = $props();
</script>

<div class="letter" class:still={reduced}>
	<div class="sheet">
		<p class="kicker">Virtues</p>
		<h1>A small correction.</h1>

		<!-- Placeholder. A face carries an argument about trust in a way a
		     paragraph cannot, which is exactly the job of this screen — so the
		     slot is held open at full width rather than bolted on later. -->
		<div class="film" role="img" aria-label="A short film from the founder — coming soon">
			<div class="film-inner">
				<Icon icon="ri:play-circle-line" width="30" />
				<span>A word from the founder</span>
			</div>
		</div>

		<div class="body">
			<p>
				The most important things in a life have always been handled closest to
				home — your faith, your family, your formation, the shape of your days.
			</p>
			<p>
				It's strange, then, that the record of all of it now lives on machines you
				will never see, owned by companies you will never meet.
			</p>
			<p>
				Virtues is a small correction. A server, in your home, that holds the story
				of your life and answers only to you. Not safer because we promise it is —
				safer because there is no other way for it to be.
			</p>
			<p>
				Privacy isn't just a technical preference. It's a moral precondition for
				self-knowledge: you can't ask honest questions about your life if your
				honest answers are used to predict you, addict you, and sell you the next
				thing.
			</p>
		</div>

		<div class="sign">
			<img src="/images/adam_signature.png" alt="Adam Jace" />
			<div class="role">Founder, Virtues</div>
		</div>

		<!-- The trajectory, stated once and quietly. An account and a relay are
		     the parts of this that are still someone else's, and saying so here
		     turns the subscription from what you are buying into what you are
		     funding the end of. Promised in the manifesto already (phases 2-3);
		     this is the same promise where it is actually being asked for. -->
		<p class="trajectory">
			Today a subscription pays for sign-in and the relay that reaches your box from
			away. Both are on our side of the wall, and both are meant to go: as home
			hardware improves, the account goes, the relay goes, and what's left is a
			machine in your house that no one else can reach.
		</p>

		<button class="begin" onclick={onbegin}>
			Begin
			<Icon icon="ri:arrow-right-line" width="16" />
		</button>
	</div>
</div>

<style>
	/* One screen, centred, nothing else on it. The page behind this has no
	   sidebar and no toolbar — the letter is the whole window. */
	.letter {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 4rem 1.5rem;
	}

	.sheet {
		width: 100%;
		max-width: 34rem;
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

	/* Holds its aspect so the page does not reflow when a real film lands. */
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

	.sign {
		margin-top: 2.25rem;
	}

	/* The signature is ink on a dark ground: invert rather than ship a second
	   asset that can drift from the one on the website. */
	.sign img {
		height: 3rem;
		width: auto;
		filter: invert(1) brightness(1.6);
		opacity: 0.9;
	}

	.role {
		margin-top: 0.35rem;
		font-size: 12.5px;
		color: var(--color-foreground-subtle);
	}

	.trajectory {
		margin: 2.25rem 0 0;
		padding-top: 1.25rem;
		border-top: 1px solid var(--color-border);
		font-size: 13.5px;
		line-height: 1.6;
		color: var(--color-foreground-subtle);
		max-width: 30rem;
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
