<script lang="ts">
	/**
	 * What the assistant can do: one switch per capability
	 * (`/api/settings/capabilities`). Your server enforces them
	 * (`api/capabilities.rs`): an off capability's tools are not offered in a
	 * chat, and the tool runner refuses them for applets too.
	 */
	import { onMount } from 'svelte';
	import SettingRow from './SettingRow.svelte';
	import Switch from './Switch.svelte';
	import { getCapabilities, setCapability, ApiError, type Capability } from '$lib/api/client';
	import { findSetting, titleOf } from '$lib/settings';

	let { assistantName }: { assistantName: string } = $props();

	const WORDS: Record<Capability['id'], { label: string; help: string }> = {
		web_search: { label: 'Search the web', help: 'Looks things up online when a question needs it.' },
		browser: { label: 'Use your browser', help: 'Works in the browser in the Virtues app on your Mac, while the app is open.' },
		applets: { label: 'Make and change applets', help: 'Builds applets that run on their own, and edits them.' },
		pages: { label: 'Write in your pages', help: 'Creates pages and edits them when you ask.' },
	};

	let capabilities = $state<Capability[] | null>(null);
	let loadError = $state(false);
	let saving = $state<Capability['id'] | null>(null);
	let error = $state<string | null>(null);

	async function load() {
		loadError = false;
		try {
			capabilities = await getCapabilities();
		} catch {
			loadError = true;
		}
	}

	onMount(load);

	async function toggle(id: Capability['id'], on: boolean) {
		saving = id;
		error = null;
		try {
			capabilities = await setCapability(id, on);
		} catch (e) {
			error = e instanceof ApiError ? e.message : "Your server couldn't change this. Try again.";
		} finally {
			saving = null;
		}
	}

	const title = $derived(titleOf(findSetting('capabilities')?.title ?? 'What {name} can do', assistantName));
</script>

<SettingRow id="capabilities" label={title} help="Turn one off and {assistantName} won't do it, in a chat or in an applet." block>
	{#if loadError}
		<p class="note">
			Your server couldn't read these.
			<button type="button" class="again" onclick={load}>Try again</button>
		</p>
	{:else if capabilities}
		<ul>
			{#each capabilities as capability (capability.id)}
				{@const words = WORDS[capability.id]}
				{#if words}
					<li>
						<span class="words">
							<span class="name">{words.label}</span>
							<span class="help">{words.help}</span>
						</span>
						<Switch
							checked={capability.on}
							label={words.label}
							disabled={saving !== null}
							onchange={(on) => toggle(capability.id, on)}
						/>
					</li>
				{/if}
			{/each}
		</ul>
		{#if error}<p class="note error">{error}</p>{/if}
	{/if}
</SettingRow>

<style>
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 24px;
		padding: 8px 0;
	}

	.words {
		min-width: 0;
	}

	.name {
		display: block;
		font-size: 14px;
		color: var(--color-foreground);
	}

	.help {
		display: block;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.note {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.note.error {
		margin-top: 8px;
		color: var(--color-error);
	}

	.again {
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
</style>
