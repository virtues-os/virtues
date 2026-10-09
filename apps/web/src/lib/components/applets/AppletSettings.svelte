<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import AppletSource from '$lib/components/applets/AppletSource.svelte';
	import Button from '$lib/components/Button.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { patchApplet, type Applet, type PatchAppletBody } from '$lib/api/client';
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

	/**
	 * Their prompt differs from the one we ship.
	 *
	 * Either they edited it, or they are on a box that predates us recording
	 * what we shipped. This deliberately does not guess which: both mean "what
	 * you are running is not what we would give you now", and the honest
	 * affordance for both is the same one.
	 */
	const customized = $derived(
		Boolean(
			action.agent_shipped && action.agent && action.agent.trim() !== action.agent_shipped.trim()
		)
	);

	function fields(a: Applet) {
		return { name: a.name, agent: a.agent ?? '', schedule: a.schedule ?? '', memory: a.memory ?? '' };
	}
	let edit = $state(fields(action));
	let isDirty = $state(false);

	// Editability follows `owner`, because that is genuinely what the server
	// enforces: reconcile owns those rows and would overwrite an edit anyway.
	const isSystem = $derived(action.owner === 'system');
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

	/**
	 * Take the shipped prompt back.
	 *
	 * Loads it into the editor rather than saving it, so the change is visible
	 * and reversible before it is committed — replacing prose someone wrote
	 * should not happen on one click with nothing shown.
	 */
	function useShippedPrompt() {
		if (!action.agent_shipped) return;
		edit.agent = action.agent_shipped;
		markDirty();
	}

	async function save() {
		saving = true;
		err = null;
		try {
			const patch: PatchAppletBody = {};
			if (!isSystem && edit.name !== action.name) patch.name = edit.name;
			if (!isSystem && edit.agent !== (action.agent ?? '')) {
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
				<Icon icon="ri:lock-line" width="12" />
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
					disabled={isSystem}
					oninput={markDirty}
					placeholder="What should this applet do each run?"
				></textarea>
			{:else}
				<div class="pipeline-note">
					<Icon icon="ri:terminal-line" width="14" />
					<span>
						Compiled program, run fresh each time it fires - <code
							>{action.command?.join(' ') ?? 'built in'}</code
						>. No model is involved.
					</span>
				</div>
			{/if}
			{#if isSystem && isAgent}
				<span class="hint">
					<Icon icon="ri:lock-line" width="12" /> Read-only. This prompt ships with the applet
				</span>
			{:else if customized}
				<!-- The only channel an edited applet has. A prompt you
				     wrote is never overwritten on upgrade, which is right
				     and also means a fix we ship cannot reach you — so
				     this line is how you find out one exists. -->
				<div class="prompt-drift">
					<span>
						You've edited this. The version that ships with the applet has changed since -
						improvements and fixes land there, not here.
					</span>
					<TextAction inline onclick={useShippedPrompt}>Use the version that ships</TextAction>
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

	{#if action.condition}
		<div class="field">
			<span class="label">Only when</span>
			<code class="readonly-value mono">{action.condition}</code>
			<span class="hint">
				Checked before each run. When it's false your server skips the run rather than failing
				it.
			</span>
		</div>
	{/if}

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
		border-radius: 6px;
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
		border-radius: 999px;
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
