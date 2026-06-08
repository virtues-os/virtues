<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import { toast } from 'svelte-sonner';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Types
	interface BuiltinTool {
		id: string;
		name: string;
		description: string | null;
		tool_type: 'builtin';
		category: string | null;
		icon: string | null;
		enabled: boolean;
	}

	interface McpTool {
		id: string;
		tool_name: string;
		description: string | null;
		enabled: boolean;
	}

	interface McpServer {
		id: string;
		name: string;
		url: string;
		description: string | null;
		enabled: boolean;
		status: 'disconnected' | 'connecting' | 'connected' | 'error';
		last_error: string | null;
		tool_count: number;
	}

	interface McpServerDetail extends McpServer {
		tools: McpTool[];
	}

	// State
	let builtinTools = $state<BuiltinTool[]>([]);
	let mcpServers = $state<McpServerDetail[]>([]);
	let loading = $state(true);
	let expandedServers = $state<Set<string>>(new Set());

	// Add server form
	let showAddForm = $state(false);
	let addName = $state('');
	let addUrl = $state('');
	let addDescription = $state('');
	let addAuthToken = $state('');
	let addLoading = $state(false);

	// Catalog state
	let showCatalog = $state(false);
	let catalogQuery = $state('');
	let catalogResults = $state<{ name: string; title: string; description: string; url?: string }[]>([]);
	let catalogLoading = $state(false);

	$effect(() => {
		if (active && builtinTools.length === 0) loadAll();
	});

	async function loadAll() {
		loading = true;
		try {
			const [toolsRes, serversRes] = await Promise.all([
				fetch('/api/tools'),
				fetch('/api/mcp/servers'),
			]);

			if (toolsRes.ok) {
				const allTools = await toolsRes.json();
				builtinTools = allTools.filter((t: BuiltinTool) => t.tool_type === 'builtin');
			}

			if (serversRes.ok) {
				const servers: McpServer[] = await serversRes.json();
				// Load details for each server
				const details = await Promise.all(
					servers.map(async (s) => {
						const res = await fetch(`/api/mcp/servers/${s.id}`);
						if (res.ok) return await res.json();
						return { ...s, tools: [] };
					})
				);
				mcpServers = details;
				// Auto-expand connected servers
				const expanded = new Set(expandedServers);
				for (const s of mcpServers) {
					if (s.status === 'connected') expanded.add(s.id);
				}
				expandedServers = expanded;
			}
		} catch (e) {
			console.error('Failed to load tools:', e);
		} finally {
			loading = false;
		}
	}

	async function toggleBuiltinTool(toolId: string, currentEnabled: boolean) {
		// Update via assistant profile enabled_tools
		try {
			const profileRes = await fetch('/api/assistant-profile');
			if (!profileRes.ok) return;
			const profile = await profileRes.json();
			const enabledTools = profile.enabled_tools ? JSON.parse(profile.enabled_tools) : {};
			enabledTools[toolId] = !currentEnabled;

			await fetch('/api/assistant-profile', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ enabled_tools: JSON.stringify(enabledTools) }),
			});

			// Update local state
			builtinTools = builtinTools.map((t) =>
				t.id === toolId ? { ...t, enabled: !currentEnabled } : t
			);
		} catch (e) {
			toast.error('Failed to toggle tool');
		}
	}

	async function toggleMcpTool(toolId: string, serverId: string) {
		try {
			const res = await fetch(`/api/mcp/tools/${encodeURIComponent(toolId)}/toggle`, {
				method: 'PATCH',
			});
			if (res.ok) {
				const { enabled } = await res.json();
				mcpServers = mcpServers.map((s) =>
					s.id === serverId
						? {
								...s,
								tools: s.tools.map((t) =>
									t.id === toolId ? { ...t, enabled } : t
								),
							}
						: s
				);
			}
		} catch (e) {
			toast.error('Failed to toggle tool');
		}
	}

	async function connectServer(serverId: string) {
		try {
			const res = await fetch(`/api/mcp/servers/${serverId}/connect`, { method: 'POST' });
			if (res.ok) {
				toast.success('Connected');
				await loadAll();
			} else {
				const err = await res.json();
				toast.error(err.error || 'Failed to connect');
			}
		} catch (e) {
			toast.error('Failed to connect');
		}
	}

	async function disconnectServer(serverId: string) {
		try {
			await fetch(`/api/mcp/servers/${serverId}/disconnect`, { method: 'POST' });
			toast.success('Disconnected');
			await loadAll();
		} catch (e) {
			toast.error('Failed to disconnect');
		}
	}

	async function deleteServer(serverId: string) {
		if (!confirm('Delete this MCP server and all its tools?')) return;
		try {
			await fetch(`/api/mcp/servers/${serverId}`, { method: 'DELETE' });
			toast.success('Server deleted');
			mcpServers = mcpServers.filter((s) => s.id !== serverId);
		} catch (e) {
			toast.error('Failed to delete server');
		}
	}

	async function addServer() {
		if (!addName || !addUrl) return;
		addLoading = true;
		try {
			const res = await fetch('/api/mcp/servers', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					name: addName,
					url: addUrl,
					description: addDescription || undefined,
					auth_token: addAuthToken || undefined,
				}),
			});
			if (res.ok) {
				toast.success('Server added');
				addName = '';
				addUrl = '';
				addDescription = '';
				addAuthToken = '';
				showAddForm = false;
				await loadAll();
			} else {
				const err = await res.json();
				toast.error(err.error || 'Failed to add server');
			}
		} catch (e) {
			toast.error('Failed to add server');
		} finally {
			addLoading = false;
		}
	}

	async function searchCatalog() {
		if (!catalogQuery.trim()) return;
		catalogLoading = true;
		try {
			const res = await fetch(
				`https://registry.modelcontextprotocol.io/v0.1/servers?search=${encodeURIComponent(catalogQuery)}&limit=20`
			);
			if (res.ok) {
				const data = await res.json();
				catalogResults = (data.servers || []).map((entry: Record<string, any>) => {
					const s = entry.server || entry;
					return {
						name: String(s.name || ''),
						title: String(s.name || ''),
						description: String(s.description || ''),
						url: (Array.isArray(s.remotes) && s.remotes[0]?.url) ? String(s.remotes[0].url) : '',
					};
				});
			}
		} catch (e) {
			toast.error('Failed to search registry');
		} finally {
			catalogLoading = false;
		}
	}

	function addFromCatalog(entry: { name: string; title: string; description: string; url?: string }) {
		addName = entry.title || entry.name;
		addUrl = entry.url || '';
		addDescription = entry.description;
		showCatalog = false;
		showAddForm = true;
	}

	function toggleExpand(serverId: string) {
		if (expandedServers.has(serverId)) {
			expandedServers.delete(serverId);
		} else {
			expandedServers.add(serverId);
		}
		expandedServers = new Set(expandedServers);
	}

	function statusDot(status: string): string {
		switch (status) {
			case 'connected': return 'bg-success';
			case 'connecting': return 'bg-warning animate-pulse';
			case 'error': return 'bg-error';
			default: return 'bg-neutral-400';
		}
	}
</script>

<Page
	title="Tools"
	description="Manage built-in tools and connect external MCP servers."
	maxWidth="narrow"
>
	{#if loading}
		<div class="flex items-center justify-center h-32">
			<div class="text-sm text-foreground-muted">Loading tools...</div>
		</div>
	{:else}
		<div class="space-y-8">

			<!-- Built-in Virtues tools -->
			<div class="space-y-2">
				<div class="flex items-center gap-2 text-sm font-medium text-neutral-400 uppercase tracking-wide">
					<Icon icon="ri:compass-3-line" width={14} height={14} />
					Virtues (built-in)
				</div>
				<div class="divide-y divide-neutral-800 border border-neutral-800 rounded-lg overflow-hidden">
					{#each builtinTools as tool}
						<div class="flex items-center justify-between px-4 py-3 hover:bg-neutral-800/50">
							<div class="flex items-center gap-3 min-w-0">
								<Icon icon={tool.icon || 'ri:tools-line'} width={16} height={16} class="text-neutral-400 shrink-0" />
								<div class="min-w-0">
									<div class="text-sm font-medium truncate">{tool.name}</div>
									{#if tool.description}
										<div class="text-xs text-neutral-500 truncate">{tool.description}</div>
									{/if}
								</div>
							</div>
							<button
								class="relative w-9 h-5 rounded-full transition-colors shrink-0 {tool.enabled ? 'bg-primary' : 'bg-surface-elevated'}"
								onclick={() => toggleBuiltinTool(tool.id, tool.enabled)}
								aria-label="Toggle {tool.name}"
							>
								<div class="absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform {tool.enabled ? 'translate-x-4' : ''}"></div>
							</button>
						</div>
					{/each}
				</div>
			</div>

			<!-- MCP Servers -->
			{#each mcpServers as server}
				<div class="space-y-2">
					<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div
					class="flex items-center justify-between w-full text-left group cursor-pointer"
					onclick={() => toggleExpand(server.id)}
					onkeydown={(e) => e.key === 'Enter' && toggleExpand(server.id)}
					role="button"
					tabindex="0"
				>
						<div class="flex items-center gap-2 text-sm font-medium text-neutral-400 uppercase tracking-wide">
							<div class="w-2 h-2 rounded-full {statusDot(server.status)}"></div>
							{server.name}
							<span class="text-xs font-normal normal-case text-neutral-500">
								({server.tools.length} tools)
							</span>
						</div>
						<div class="flex items-center gap-2">
							{#if server.status === 'connected'}
								<button
									class="text-xs text-neutral-500 hover:text-neutral-300 px-2 py-1"
									onclick={(e) => { e.stopPropagation(); disconnectServer(server.id); }}
								>
									Disconnect
								</button>
							{:else}
								<button
									class="text-xs text-neutral-500 hover:text-neutral-300 px-2 py-1"
									onclick={(e) => { e.stopPropagation(); connectServer(server.id); }}
								>
									Connect
								</button>
							{/if}
							<button
								class="text-xs text-error/60 hover:text-error px-2 py-1"
								onclick={(e) => { e.stopPropagation(); deleteServer(server.id); }}
							>
								Delete
							</button>
							<Icon
								icon={expandedServers.has(server.id) ? 'ri:arrow-up-s-line' : 'ri:arrow-down-s-line'}
								width={16}
								height={16}
								class="text-neutral-500"
							/>
						</div>
					</div>

					{#if server.last_error}
						<div class="text-xs text-error bg-error/10 px-3 py-2 rounded-lg">
							{server.last_error}
						</div>
					{/if}

					{#if expandedServers.has(server.id)}
						<div class="divide-y divide-neutral-800 border border-neutral-800 rounded-lg overflow-hidden">
							{#each server.tools as tool}
								<div class="flex items-center justify-between px-4 py-3 hover:bg-neutral-800/50">
									<div class="min-w-0">
										<div class="text-sm font-medium truncate">{tool.tool_name}</div>
										{#if tool.description}
											<div class="text-xs text-neutral-500 truncate">{tool.description}</div>
										{/if}
									</div>
									<button
										class="relative w-9 h-5 rounded-full transition-colors shrink-0 {tool.enabled ? 'bg-primary' : 'bg-surface-elevated'}"
										onclick={() => toggleMcpTool(tool.id, server.id)}
										aria-label="Toggle {tool.tool_name}"
									>
										<div class="absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform {tool.enabled ? 'translate-x-4' : ''}"></div>
									</button>
								</div>
							{:else}
								<div class="px-4 py-3 text-sm text-neutral-500">No tools discovered</div>
							{/each}
						</div>
					{/if}
				</div>
			{/each}

			<!-- Add Server -->
			<div class="space-y-3">
				{#if !showAddForm && !showCatalog}
					<div class="flex gap-2">
						<button
							class="flex items-center gap-2 text-sm text-neutral-400 hover:text-neutral-200 px-3 py-2 border border-dashed border-neutral-700 rounded-lg hover:border-neutral-500 transition-colors"
							onclick={() => (showAddForm = true)}
						>
							<Icon icon="ri:add-line" width={14} height={14} />
							Add Server
						</button>
						<button
							class="flex items-center gap-2 text-sm text-neutral-400 hover:text-neutral-200 px-3 py-2 border border-dashed border-neutral-700 rounded-lg hover:border-neutral-500 transition-colors"
							onclick={() => (showCatalog = true)}
						>
							<Icon icon="ri:search-line" width={14} height={14} />
							Browse Catalog
						</button>
					</div>
				{/if}

				{#if showCatalog}
					<div class="border border-neutral-800 rounded-lg p-4 space-y-3">
						<div class="flex items-center justify-between">
							<div class="text-sm font-medium">MCP Server Catalog</div>
							<button class="text-xs text-neutral-500 hover:text-neutral-300" onclick={() => (showCatalog = false)}>
								Close
							</button>
						</div>
						<div class="flex gap-2">
							<input
								type="text"
								bind:value={catalogQuery}
								placeholder="Search servers..."
								class="flex-1 bg-neutral-900 border border-neutral-700 rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-neutral-500"
								onkeydown={(e) => e.key === 'Enter' && searchCatalog()}
							/>
							<button
								class="px-3 py-2 bg-neutral-800 hover:bg-neutral-700 rounded-lg text-sm"
								onclick={searchCatalog}
								disabled={catalogLoading}
							>
								{catalogLoading ? '...' : 'Search'}
							</button>
						</div>
						{#if catalogResults.length > 0}
							<div class="divide-y divide-neutral-800 max-h-64 overflow-y-auto">
								{#each catalogResults as entry}
									<div class="flex items-center justify-between py-2">
										<div class="min-w-0 pr-3">
											<div class="text-sm font-medium truncate">{entry.title}</div>
											<div class="text-xs text-neutral-500 truncate">{entry.description}</div>
										</div>
										<button
											class="text-xs text-primary hover:text-primary/80 px-2 py-1 shrink-0"
											onclick={() => addFromCatalog(entry)}
										>
											Add
										</button>
									</div>
								{/each}
							</div>
						{/if}
					</div>
				{/if}

				{#if showAddForm}
					<div class="border border-neutral-800 rounded-lg p-4 space-y-3">
						<div class="flex items-center justify-between">
							<div class="text-sm font-medium">Add MCP Server</div>
							<button class="text-xs text-neutral-500 hover:text-neutral-300" onclick={() => (showAddForm = false)}>
								Cancel
							</button>
						</div>
						<input
							type="text"
							bind:value={addName}
							placeholder="Server name (e.g. GitHub)"
							class="w-full bg-neutral-900 border border-neutral-700 rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-neutral-500"
						/>
						<input
							type="text"
							bind:value={addUrl}
							placeholder="Server URL (e.g. https://mcp.github.com/sse)"
							class="w-full bg-neutral-900 border border-neutral-700 rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-neutral-500"
						/>
						<input
							type="text"
							bind:value={addDescription}
							placeholder="Description (optional)"
							class="w-full bg-neutral-900 border border-neutral-700 rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-neutral-500"
						/>
						<input
							type="password"
							bind:value={addAuthToken}
							placeholder="Auth token (optional)"
							class="w-full bg-neutral-900 border border-neutral-700 rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-neutral-500"
						/>
						<button
							class="w-full px-3 py-2 bg-primary hover:bg-primary-hover rounded-lg text-sm font-medium disabled:opacity-50"
							onclick={addServer}
							disabled={addLoading || !addName || !addUrl}
						>
							{addLoading ? 'Adding...' : 'Add & Connect'}
						</button>
					</div>
				{/if}
			</div>
		</div>
	{/if}
</Page>
