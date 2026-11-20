<script lang="ts">
	/**
	 * One job, opened in place on the Jobs page: what it does, when it runs,
	 * the hosts its last runs reached, and what those runs did. It answers
	 * "what does this do on my server" without leaving the list. Its full
	 * run history and its technical details are on the job's own page.
	 */
	import Switch from '$lib/components/settings/Switch.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { getAppletLog, patchApplet, runApplet, type Applet, type AppletLogEntry } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { describeTriggers, hostsReached, runLine } from '$lib/applets/palette';
	import { canRun } from '$lib/applets/standing';
	import { optionsRoute } from '$lib/applets/routes';
	import { formatMicrosPrecise } from '$lib/utils/currency';
	import { whenInWords } from '$lib/utils/dateUtils';

	let { job, onchanged }: { job: Applet; onchanged: () => void } = $props();

	/** How many log lines to show. A line stands for one run, or a run of the same result. */
	const LINES = 5;

	let log = $state<AppletLogEntry[] | null>(null);
	let logErr = $state(false);
	let busy = $state(false);
	let err = $state<string | null>(null);

	async function read(id: string) {
		try {
			log = await getAppletLog(id, LINES);
			logErr = false;
		} catch {
			logErr = true;
		}
	}

	$effect(() => {
		log = null;
		void read(job.id);
	});

	const hosts = $derived(log ? hostsReached(log) : []);

	async function runNow() {
		busy = true;
		err = null;
		try {
			await runApplet(job.id);
			onchanged();
			await read(job.id);
		} catch (e) {
			err = `Your server didn't start it: ${e instanceof Error ? e.message : String(e)}. Try again.`;
		} finally {
			busy = false;
		}
	}

	async function setEnabled(on: boolean) {
		busy = true;
		err = null;
		try {
			await patchApplet(job.id, { enabled: on });
			onchanged();
		} catch (e) {
			err = `Your server didn't save that: ${e instanceof Error ? e.message : String(e)}. Try again.`;
		} finally {
			busy = false;
		}
	}

	function openHistory() {
		windowShellStore.openRouteBeside(optionsRoute(job.id, 'history'), job.name);
	}
</script>

<div class="job-open">
	{#if job.description}
		<p class="what">{job.description}</p>
	{/if}

	<div class="fact">
		<span class="label">When it runs</span>
		<span class="value">{describeTriggers(job)}</span>
		<span class="acts">
			{#if canRun(job)}
				<TextAction quiet onclick={runNow} disabled={busy}>Run now</TextAction>
			{/if}
			<Switch checked={job.enabled} label="Run it" disabled={busy} onchange={setEnabled} />
		</span>
	</div>

	{#if hosts.length}
		<div class="fact">
			<span class="label">Reaches</span>
			<span class="value hosts">{hosts.join(', ')}</span>
		</div>
	{/if}

	{#if err}
		<p class="err" role="alert">{err}</p>
	{/if}

	<div class="runs">
		<span class="label">Last runs</span>
		{#if logErr}
			<p class="quiet">Your server couldn't read its runs.</p>
		{:else if log === null}
			<p class="quiet">Reading its runs…</p>
		{:else if log.length === 0}
			<p class="quiet">It hasn't run yet.</p>
		{:else}
			<ul role="list">
				{#each log as e (e.run_id ?? e.last_at)}
					<li>
						<span class="when">{e.last_at ? whenInWords(e.last_at) : ''}</span>
						<span class="did" class:failed={e.status === 'error'}>{runLine(e)}</span>
						{#if e.cost_micros > 0}
							<span class="cost">{formatMicrosPrecise(e.cost_micros)}</span>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	<TextAction quiet onclick={openHistory}>Every run, and its technical details</TextAction>
</div>

<style>
	.job-open {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 4px 0 20px;
	}
	.what {
		margin: 0;
		max-width: 60ch;
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground);
	}
	.fact {
		display: grid;
		grid-template-columns: 96px minmax(0, 1fr) auto;
		align-items: center;
		gap: 12px;
		font-size: 13px;
	}
	.label {
		font-size: 13px;
		color: var(--color-foreground-subtle);
	}
	.value {
		color: var(--color-foreground);
		min-width: 0;
	}
	.hosts {
		overflow-wrap: anywhere;
	}
	.acts {
		display: flex;
		align-items: center;
		gap: 16px;
	}
	.runs {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.runs ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.runs li {
		display: grid;
		grid-template-columns: 168px minmax(0, 1fr) auto;
		gap: 12px;
		padding: 6px 0;
		font-size: 13px;
		line-height: 1.45;
	}
	.runs li + li {
		border-top: 1px solid var(--color-border);
	}
	.when,
	.cost {
		color: var(--color-foreground-muted);
	}
	.did {
		color: var(--color-foreground);
		min-width: 0;
	}
	.did.failed {
		color: var(--color-warning);
	}
	.quiet {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.err {
		margin: 0;
		font-size: 13px;
		color: var(--color-error);
	}
	@container (max-width: 520px) {
		.fact,
		.runs li {
			grid-template-columns: minmax(0, 1fr);
			gap: 2px;
		}
	}
</style>
