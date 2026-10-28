<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import AppletSource from '$lib/components/applets/AppletSource.svelte';
	import AppletAllowList from '$lib/components/applets/AppletAllowList.svelte';
	import Button from '$lib/components/Button.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import {
		getAppletConnections,
		getAppletHosts,
		patchApplet,
		putAppletConnections,
		putAppletHosts,
		type Applet,
		type PatchAppletBody
	} from '$lib/api/client';
	import { describeSchedule } from '$lib/applets/palette';

	/**
	 * An applet's technical details: its prompt, schedule, wake-ups and notes,
	 * editable where the server lets them be. Reached from Info, because you
	 * change an applet by talking to it, and this is for when that isn't
	 * enough.
	 */
	let {
		action = $bindable(),
		onRenamed
	}: {
		action: Applet;
		onRenamed: (name: string) => void;
	} = $props();

	let saving = $state(false);
	let err = $state<string | null>(null);

	/** A shipped applet whose prompt or schedule you changed. */
	const customized = $derived(Boolean(action.has_override));

	function fields(a: Applet) {
		return { name: a.name, agent: a.agent ?? '', schedule: a.schedule ?? '', memory: a.memory ?? '' };
	}
	let edit = $state(fields(action));
	let isDirty = $state(false);

	// A shipped applet's name and code are its file's. Its prompt and
	// schedule can change: your server keeps those changes beside the shipped
	// file, so a shipped fix still reaches the rest.
	const isSystem = $derived(action.owner === 'system');
	// `lake` in its file mounts every file in the lake for its code to read,
	// so a host allowed below could receive any of them.
	const readsLake = $derived(
		Array.isArray(action.config?.allowed_tools) && action.config.allowed_tools.includes('lake')
	);
	const isAgent = $derived(Boolean(action.agent && action.agent.trim().length > 0));

	// What the user is told, though, follows `origin` — the distinction the
	// list page already learned. Every source fan-out row is owner='system',
	// so keying the EXPLANATION off owner told you the Gmail sync you
	// connected on purpose was an internal system pipeline.
	const managedNote = $derived.by(() => {
		if (!isSystem) return null;
		switch (action.origin) {
			case 'source':
				return 'Part of a source you connected. Its settings come from the connection - disconnect the source to remove it.';
			default:
				return 'Built in. It keeps your server running, so you can turn it off but not delete it - reconcile would recreate it.';
		}
	});

	const triggers = $derived(action.triggers ?? []);

	// Lifecycle, in words rather than a raw SQL string.
	const lifecycle = $derived.by(() => {
		if (action.archived_at) return `Finished ${new Date(action.archived_at).toLocaleDateString()}`;
		if (!action.until) return 'Runs for as long as it is on';
		if (action.until.toLowerCase() === 'once') return 'Runs once, then finishes';
		return `Finishes when: ${action.until}`;
	});

	function markDirty() {
		isDirty = true;
	}

	/** Drop your changes and run the shipped applet as it ships. */
	async function useShipped() {
		saving = true;
		err = null;
		try {
			const updated = await patchApplet(action.id, { reset: true });
			action = updated;
			edit = fields(updated);
			isDirty = false;
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}

	async function save() {
		saving = true;
		err = null;
		try {
			const patch: PatchAppletBody = {};
			if (!isSystem && edit.name !== action.name) patch.name = edit.name;
			if ((isAgent || !isSystem) && edit.agent !== (action.agent ?? '')) {
				patch.agent = edit.agent.trim() ? edit.agent : null;
			}
			if (edit.schedule !== (action.schedule ?? '')) {
				patch.schedule = edit.schedule.trim() ? edit.schedule : null;
			}
			if (edit.memory !== (action.memory ?? '')) {
				patch.memory = edit.memory.trim() ? edit.memory : null;
			}
			if (Object.keys(patch).length === 0) {
				isDirty = false;
				return;
			}
			const updated = await patchApplet(action.id, patch);
			action = updated;
			edit = fields(updated);
			isDirty = false;
			onRenamed(updated.name);
		} catch (e) {
			err = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}
</script>

<div class="settings">
	<label class="field">
		<span class="label">Name</span>
		<input type="text" bind:value={edit.name} disabled={isSystem} oninput={markDirty} />
		{#if managedNote}
			<span class="hint">
				<Icon icon="mynaui:lock" width="12" />
				{managedNote}
			</span>
		{/if}
	</label>

	<!-- A face-only applet has no server-side run and no prompt —
	     don't show an empty agent editor for it. -->
	{#if isAgent || !action.has_face}
		<label class="field">
			<!-- An applet's shape comes from which fields are set, and this
			     label is where a reader first learns which one they are
			     looking at. Calling a compiled sync's field "Agent prompt"
			     said the opposite of the truth on 22 of the 24 shipped
			     applets. -->
			<span class="label">{isAgent ? 'What it does each run' : 'What it runs'}</span>
			{#if isAgent || !isSystem}
				<textarea
					rows="10"
					bind:value={edit.agent}
					oninput={markDirty}
					placeholder="What should this applet do each run?"
				></textarea>
			{:else}
				<div class="pipeline-note">
					<Icon icon="mynaui:terminal" width="14" />
					<span>
						Compiled program, run fresh each time it fires - <code
							>{action.command?.join(' ') ?? 'built in'}</code
						>. No model is involved.
					</span>
				</div>
			{/if}
			{#if customized}
				<!-- Your prompt or schedule stays yours across upgrades, so a
				     fix we ship to either does not reach you. This line is
				     how you get back to what ships. -->
				<div class="prompt-drift">
					<span>
						You've changed this built-in applet. Fixes to what you changed don't reach you.
					</span>
					<TextAction inline onclick={useShipped}>Use the version that ships</TextAction>
				</div>
			{/if}
		</label>
	{/if}

	<label class="field">
		<span class="label">Schedule</span>
		<input
			type="text"
			bind:value={edit.schedule}
			placeholder="0 0 * * * *  (6-field cron, empty = on-demand)"
			oninput={markDirty}
		/>
		<span class="hint">{describeSchedule(edit.schedule || null)}</span>
	</label>

	{#if action.secrets?.length}
		<div class="field">
			<span class="label">Keys it needs</span>
			<ul class="keys">
				{#each action.secrets as key (key.name)}
					<li>
						<code>{key.name}</code>
						<span class="hint">
							{key.is_set ? 'Set' : 'Not set. Add it in Settings, under Keys.'}
						</span>
					</li>
				{/each}
			</ul>
		</div>
	{/if}

	<AppletAllowList
		appletId={action.id}
		label="Hosts it can reach"
		what="hosts"
		hint={readsLake
			? 'Its code asks to reach these hosts. Nothing goes out until you allow a host here. It can read every file in your lake, so a host you allow can receive any of them, and your records.'
			: 'Its code asks to reach these hosts. Nothing goes out until you allow a host here. A host you allow can receive what the applet reads, including your records.'}
		load={getAppletHosts}
		save={putAppletHosts}
	/>

	<AppletAllowList
		appletId={action.id}
		label="Services it can use"
		what="connections"
		hint="It asks to use these connected services. It can't until you allow one here. A service you allow can receive what the applet reads, including your records. Tools a service marks as deleting something are never offered to it."
		load={getAppletConnections}
		save={putAppletConnections}
		unavailable={(name, grant) =>
			grant.connected?.includes(name) ? null : 'Not connected. Connect it in Sources first.'}
	/>

	<!-- Three facts the page never showed, and the reason a person
	     could not tell why an applet had or hadn't run: what wakes
	     it, what it checks once awake, and when it is done. -->
	<div class="field">
		<span class="label">What wakes it</span>
		<div class="chips">
			{#each triggers as t (t)}
				<span class="chip">{t === 'cron' ? 'schedule' : t}</span>
			{/each}
			{#if triggers.length === 0}
				<span class="readonly-value dim">nothing. It never runs on its own</span>
			{/if}
		</div>
	</div>

	<div class="field">
		<span class="label">Lifecycle</span>
		<p class="readonly-value">{lifecycle}</p>
	</div>

	<label class="field">
		<!-- Not a settings field: this is the applet's own scratchpad,
		     written by it, for its next run. Editing it by hand is
		     allowed and is closer to amending a diary than filling a
		     form, so the label says whose it is. -->
		<span class="label">Notes it keeps</span>
		<textarea
			rows="6"
			bind:value={edit.memory}
			oninput={markDirty}
			placeholder="Empty. This applet has not written itself any notes yet."
		></textarea>
		<span class="hint">
			What this applet wrote down for its own next run. Yours to read, and to correct.
		</span>
	</label>

	{#if err}
		<p class="error-msg">{err}</p>
	{/if}

	<div class="save-row">
		<Button variant="primary" onclick={save} disabled={!isDirty || saving}>
			{saving ? 'Saving…' : 'Save changes'}
		</Button>
	</div>

	<section class="source-block">
		<h3>Source</h3>
		<p class="source-note">
			The code this applet runs. Read-only - editing forks it onto this server.
		</p>
		<AppletSource appletId={action.id} />
	</section>
</div>

<style>
	.settings {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.label {
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	.field input,
	.field textarea {
		font: inherit;
		font-size: 14px;
		padding: 8px 12px;
		border-radius: var(--radius-md);
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-foreground);
		resize: vertical;
	}
	.field textarea {
		font-family: var(--font-sans);
		line-height: 1.5;
	}
	.field input:disabled,
	.field textarea:disabled {
		opacity: 0.7;
		cursor: not-allowed;
	}
	.keys {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.keys li {
		display: flex;
		align-items: baseline;
		gap: 8px;
	}
	.hint {
		font-size: 12px;
		color: var(--color-foreground-subtle);
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.pipeline-note {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.pipeline-note code {
		font-size: 12px;
	}
	.readonly-value {
		margin: 0;
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground);
	}
	.readonly-value.dim {
		color: var(--color-foreground-subtle);
	}
	.readonly-value.mono {
		font-size: 13px;
		display: block;
		overflow-x: auto;
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}
	.chip {
		padding: 0 8px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-full);
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	/* The prompt you are running is not the prompt we ship. Stated plainly and
	   quietly: it is information, not a warning — an edited prompt is a
	   legitimate thing to have. */
	.prompt-drift {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 4px 8px;
		margin-top: 4px;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}
	.save-row {
		display: flex;
		justify-content: flex-end;
	}
	.error-msg {
		margin: 0;
		font-size: 13px;
		color: var(--color-error);
	}
	.source-block {
		margin-top: 8px;
		padding-top: 20px;
		border-top: 1px solid var(--color-border);
	}
	.source-block h3 {
		margin: 0 0 4px;
		font-size: 14px;
		font-weight: 600;
	}
	.source-note {
		margin: 0 0 12px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
</style>
