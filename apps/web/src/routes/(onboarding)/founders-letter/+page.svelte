<!--
  /founders-letter — the letter, to read again.

  The letter is Setup's preface now (/setup plays it after Hello, 2026-09-24,
  agents/plan/setup-plan.md). This route is where it can be read on its own
  afterward: from the rail's Setup panel, from the profile, from old links.
  It asks nothing and changes nothing; its one button goes back to wherever
  the reader came from.

  WAS the one screen before the app (2026-08-31), then the letter plus the
  subscription (2026-09-23). `?read` was how the rail asked to skip the cold
  open; there is no cold open here any more, so the parameter is simply
  ignored.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import FoundersLetter from "$lib/components/onboarding/document/FoundersLetter.svelte";

	const still =
		typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

	function close() {
		if (history.length > 1) history.back();
		else void goto("/home");
	}
</script>

<div class="ob-wrap letter-in" class:ob-still={still}>
	<div class="ob-sheet">
		<div class="ob-page">
			<FoundersLetter onbegin={close} beginLabel="Close the letter" />
		</div>
	</div>
</div>

<style>
	.letter-in {
		animation: letter-in 900ms cubic-bezier(0.2, 0.7, 0.2, 1) both;
	}
	@keyframes letter-in {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.letter-in {
			animation: none;
		}
	}
</style>
