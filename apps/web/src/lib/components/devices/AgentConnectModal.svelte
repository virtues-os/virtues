<script lang="ts">
	// Connect an AI agent on this Mac (Claude Code, Claude Desktop, Cursor) to
	// the server over MCP. The agent gets a device of its own, kind `agent`,
	// which the server lets reach its MCP endpoint and nothing else; this
	// sheet mints the pair token, has the app pair the bridge, and shows the
	// settings the agent needs to launch it. agents/plan/mcp-plan.md, tier 1.
	import { Button, Input } from "$lib";
	import Modal from "$lib/components/Modal.svelte";
	import MarkdownCodeBlock from "$lib/components/MarkdownCodeBlock.svelte";
	import { apiSendProven } from "$lib/api/client";
	import {
		connectAgent,
		shellSupports,
		CONNECT_AGENT_SURFACE,
		type AgentConnection,
	} from "$lib/tauri/bridge";

	let {
		open,
		onClose,
		onConnected,
	}: { open: boolean; onClose: () => void; onConnected: () => void } = $props();

	let name = $state("Claude Code");
	let connecting = $state(false);
	let error = $state<string | null>(null);
	let connection = $state<AgentConnection | null>(null);
	let supported = $state<boolean | null>(null);

	$effect(() => {
		if (open && supported === null) {
			void shellSupports(CONNECT_AGENT_SURFACE).then((ok) => (supported = ok));
		}
	});

	async function connect() {
		connecting = true;
		error = null;
		try {
			// Minting a pair code needs the app's owner proof, for an agent as
			// for any other device.
			const minted = await apiSendProven<{ token: string }>("POST", "/pair/mint", {
				intended_kind: "agent",
			});
			connection = await connectAgent(minted.token, name);
			onConnected();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			connecting = false;
		}
	}

	/** One shell word, single-quoted so paths with spaces survive. */
	const quote = (word: string) => `'${word.replaceAll("'", `'\\''`)}'`;

	const claudeCode = $derived.by(() => {
		if (!connection) return "";
		const env = Object.entries(connection.env).map(([k, v]) => `-e ${k}=${quote(v)} `);
		const args = connection.args.map(quote).join(" ");
		return `claude mcp add virtues ${env.join("")}-- ${quote(connection.command)} ${args}`;
	});

	const settingsJson = $derived.by(() => {
		if (!connection) return "";
		const server: Record<string, unknown> = {
			command: connection.command,
			args: connection.args,
		};
		if (Object.keys(connection.env).length > 0) server.env = connection.env;
		return JSON.stringify({ mcpServers: { virtues: server } }, null, 2);
	});

	function close() {
		connection = null;
		error = null;
		onClose();
	}
</script>

<Modal {open} onClose={close} title="Connect an AI agent" width="md">
	{#if supported === false}
		<p class="lede">Update the Virtues app to connect an agent on this Mac.</p>
	{:else if !connection}
		<p class="lede">
			The agent gets its own device. It can read and write your data through MCP and
			nothing else, and you can revoke it here.
		</p>
		<Input label="Agent name" bind:value={name} placeholder="Claude Code" />
		{#if error}
			<p class="error">{error}</p>
		{/if}
		<div class="actions">
			<Button variant="primary" loading={connecting} disabled={!name.trim()} onclick={connect}>
				Connect
			</Button>
		</div>
	{:else}
		<p class="lede">{connection.name} is connected. Add it to your agent's settings.</p>

		<h3 class="label">Claude Code: run this in Terminal</h3>
		<MarkdownCodeBlock token={{ lang: "sh", text: claudeCode }} />

		<h3 class="label">Claude Desktop or Cursor: add this to the MCP settings file</h3>
		<MarkdownCodeBlock token={{ lang: "json", text: settingsJson }} />

		<div class="actions">
			<Button variant="primary" onclick={close}>Done</Button>
		</div>
	{/if}
</Modal>

<style>
	.lede {
		margin: 0 0 1rem;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.label {
		margin: 1rem 0 0.5rem;
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground);
	}
	.error {
		margin: 0.75rem 0 0;
		font-size: 13px;
		color: var(--color-error);
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		margin-top: 1.25rem;
	}
</style>
