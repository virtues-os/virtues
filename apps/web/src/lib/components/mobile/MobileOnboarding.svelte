<script lang="ts">
	/**
	 * First-run "Set up your streams" flow (paired phone shell, shown once).
	 *
	 * One card per collector with a plain-language *why*; the OS permission prompt
	 * fires only when you tap Enable. Fully skippable — "Skip for now" lands you in
	 * the app with everything off (strongly discouraged, but a real path: Apple
	 * review expects the app to be usable without granting permissions, and nothing
	 * should gate reaching your box). Re-openable later from This device.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { onMount } from "svelte";

	interface Stream {
		key: string;
		title: string;
		icon: string;
		why: string;
		enableCmd: string;
		statusCmd?: string; // returns { authorized }
		state: "off" | "enabling" | "on";
	}

	let streams = $state<Stream[]>([
		{
			key: "location",
			title: "Location",
			icon: "ri:map-pin-line",
			why: "Builds your timeline of places — even in the background, recorded on your box.",
			enableCmd: "plugin:location-probe|start_probe",
			state: "off",
		},
		{
			key: "health",
			title: "Health",
			icon: "ri:heart-pulse-line",
			why: "Heart rate, steps, sleep and more from Apple Health — three years of history.",
			enableCmd: "plugin:health|enable",
			statusCmd: "plugin:health|status",
			state: "off",
		},
		{
			key: "calendar",
			title: "Calendar",
			icon: "ri:calendar-line",
			why: "Your events, past and upcoming, on your personal timeline.",
			enableCmd: "plugin:eventkit|enable",
			statusCmd: "plugin:eventkit|status",
			state: "off",
		},
		{
			key: "contacts",
			title: "Contacts",
			icon: "ri:contacts-book-line",
			why: "The people in your life become entities in your wiki.",
			enableCmd: "plugin:contacts|enable",
			statusCmd: "plugin:contacts|status",
			state: "off",
		},
		{
			key: "finance",
			title: "Finance",
			icon: "ri:bank-card-line",
			why: "Apple Card, Cash and connected accounts — three years of transactions.",
			enableCmd: "plugin:finance|enable",
			statusCmd: "plugin:finance|status",
			state: "off",
		},
		{
			key: "audio",
			title: "Audio",
			icon: "ri:mic-line",
			why: "Ambient sound of your day — conversations, places, atmosphere — recorded and transcribed on your box.",
			enableCmd: "plugin:audio|enable",
			statusCmd: "plugin:audio|status",
			state: "off",
		},
	]);

	const anyOn = $derived(streams.some((s) => s.state === "on"));

	onMount(async () => {
		// Reflect anything already granted (e.g. re-opened from This device).
		for (const s of streams) {
			if (!s.statusCmd) continue;
			try {
				const st = await invoke<{ authorized: boolean }>(s.statusCmd);
				if (st?.authorized) s.state = "on";
			} catch {
				/* ignore */
			}
		}
	});

	async function enable(s: Stream) {
		if (s.state === "on") return;
		s.state = "enabling";
		try {
			const res = await invoke<{ authorized?: boolean } | null>(s.enableCmd);
			// location's start_probe has no `authorized` field — treat a clean
			// return as success (it prompts + starts).
			s.state = res && res.authorized === false ? "off" : "on";
		} catch {
			s.state = "off";
		}
	}
</script>

{#if mobileLayout.onboardingOpen}
	<section class="onboarding">
		<header class="head">
			<button class="skip" onclick={() => mobileLayout.finishOnboarding()}>Skip for now</button>
		</header>

		<div class="body">
			<h1>Set up Virtues</h1>
			<p class="intro">
				Choose what this phone collects. Everything is stored on <b>your own box</b> —
				you can change any of this later in <b>This device</b>.
			</p>

			{#each streams as s (s.key)}
				<div class="card" class:on={s.state === "on"}>
					<div class="c-icon" class:on={s.state === "on"}>
						<Icon icon={s.icon} width={20} />
					</div>
					<div class="c-body">
						<div class="c-title">{s.title}</div>
						<div class="c-why">{s.why}</div>
					</div>
					{#if s.state === "on"}
						<span class="c-on"><Icon icon="ri:check-line" width={18} /></span>
					{:else}
						<button class="c-enable" onclick={() => enable(s)} disabled={s.state === "enabling"}>
							{s.state === "enabling" ? "…" : "Enable"}
						</button>
					{/if}
				</div>
			{/each}

		</div>

		<div class="foot">
			<button class="done" onclick={() => mobileLayout.finishOnboarding()}>
				{anyOn ? "Done" : "Continue without collecting"}
			</button>
		</div>
	</section>
{/if}

<style>
	.onboarding {
		position: fixed;
		inset: 0;
		z-index: 70;
		display: flex;
		flex-direction: column;
		background: var(--color-surface);
		color: var(--color-foreground);
		animation: rise 0.24s cubic-bezier(0.32, 0.72, 0, 1);
	}
	.head {
		display: flex;
		justify-content: flex-end;
		padding: max(10px, env(safe-area-inset-top)) 16px 4px;
		flex: none;
	}
	.skip {
		border: 0;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 15px;
		padding: 6px;
		cursor: pointer;
	}
	.body {
		flex: 1;
		overflow-y: auto;
		-webkit-overflow-scrolling: touch;
		padding: 8px 18px 16px;
	}
	h1 {
		font-size: 28px;
		font-weight: 700;
		margin: 8px 0 8px;
		letter-spacing: -0.02em;
	}
	.intro {
		font-size: 15px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
		margin: 0 0 24px;
	}
	.card {
		display: flex;
		align-items: center;
		gap: 13px;
		padding: 14px;
		border: 1px solid var(--color-border);
		border-radius: 14px;
		margin-bottom: 12px;
		transition: border-color 0.15s ease;
	}
	.card.on {
		border-color: color-mix(in srgb, var(--color-success) 55%, var(--color-border));
	}
	.card.soon {
		opacity: 0.6;
	}
	.c-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 40px;
		height: 40px;
		flex: none;
		border-radius: 10px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground-muted);
	}
	.c-icon.on {
		background: color-mix(in srgb, var(--color-success) 18%, transparent);
		color: color-mix(in srgb, var(--color-success) 75%, #000);
	}
	.c-body {
		flex: 1;
	}
	.c-title {
		font-size: 16px;
		font-weight: 600;
	}
	.c-why {
		font-size: 13px;
		line-height: 1.35;
		color: var(--color-foreground-muted);
		margin-top: 2px;
	}
	.c-enable {
		flex: none;
		border: 1px solid var(--color-primary, #2b6cff);
		color: var(--color-primary, #2b6cff);
		background: transparent;
		border-radius: 9px;
		padding: 8px 16px;
		font-size: 14px;
		font-weight: 600;
		cursor: pointer;
	}
	.c-enable:disabled {
		opacity: 0.5;
	}
	.c-on {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 50%;
		background: var(--color-success);
		color: #fff;
		flex: none;
	}
	.c-soon {
		font-size: 12px;
		color: var(--color-foreground-muted);
		padding: 4px 10px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}
	.foot {
		flex: none;
		padding: 12px 18px calc(16px + env(safe-area-inset-bottom));
		border-top: 1px solid var(--color-border);
	}
	.done {
		width: 100%;
		border: 0;
		border-radius: 12px;
		background: var(--color-primary, #2b6cff);
		color: #fff;
		font-size: 16px;
		font-weight: 600;
		padding: 15px;
		cursor: pointer;
	}
	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
</style>
