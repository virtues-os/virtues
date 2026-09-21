<!--
	One door, out of the room and Home. It used to be the box's only exit
	while the app was closed off, and carried a warning to match; nothing is
	closed off now, so leaving is just leaving — the room keeps its place,
	the sidebar card says setup is unfinished, and you come back when you
	like. Deliberately quiet: something to find, not something offered.

	IT GAINS A WORD DURING THE INTERVIEW, and only then. Every other step has
	its own controls standing under the thread — "Not now" among them — but
	`RoomControls` renders nothing once the interview is underway, by design:
	the interview is one composer and no furniture. That left this glyph as
	the only exit from the longest thing in the room, unlabeled, at the
	moment someone is most likely to need it and least likely to go hunting.
	So the label appears where the need is, rather than a second control
	surface appearing under the conversation to say what this already does.

	It is not "write it up". Stopping keeps the place — the thread is the
	transcript, `interview_started_at` is fixed, and the reply count is read
	from the database — so coming back tomorrow resumes rather than restarts.
	Closing is the interviewer's own business and stays in the conversation,
	where it can ask first.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import Icon from "$lib/components/Icon.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	const underway = $derived(gettingStarted.interviewUnderway);
	const label = $derived(underway ? "Stop for now" : "Come back to this later");

	async function leave() {
		await goto("/home");
	}
</script>

<button
	type="button"
	class="door"
	class:labeled={underway}
	onclick={leave}
	title={underway ? "Your place waits - come back whenever you like" : label}
	aria-label={label}
>
	<Icon icon="ri:door-open-line" width="16" />
	{#if underway}<span class="label">Stop for now</span>{/if}
</button>

<style>
	.door {
		display: inline-flex;
		align-items: center;
		background: none;
		border: 1px solid transparent;
		border-radius: 6px;
		padding: 0.25rem 0.4rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
		flex: none;
	}
	.door:hover {
		color: var(--color-foreground);
		border-color: var(--color-border);
	}
	/* The word sits in the same chrome register as the rest of this top-right
	   corner — it is a label on a quiet door, not a call to action. */
	.door.labeled {
		gap: 0.375rem;
		padding-right: 0.55rem;
	}
	.label {
		font-size: 12px;
		line-height: 1;
		white-space: nowrap;
	}
</style>
