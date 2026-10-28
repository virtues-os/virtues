<!--
	SkillsPanel.svelte

	The `/` menu in the chat composer's tray. As with the `@` menu, the field
	keeps the focus and what is typed after the `/` filters the list, so Up,
	Down, Enter and Escape are caught here before the editor sees them. The
	skill that writes skills is pinned below the list as its create row, and
	the arrows reach it last.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";
	import type { SkillSummary } from "$lib/api/client";
	import { skills } from "$lib/stores/skills.svelte";
	import { inComposition } from "$lib/utils/ime";

	let {
		query,
		active = true,
		onSelect,
		onClose,
	}: {
		query: string;
		/** False while the tray folds it away: it no longer takes keys. */
		active?: boolean;
		onSelect: (skill: SkillSummary) => void;
		onClose: () => void;
	} = $props();

	const uid = $props.id();
	let panelEl: HTMLDivElement | null = $state(null);
	let listEl: HTMLDivElement | null = $state(null);
	let selected = $state(0);
	let atEnd = $state(true);

	const matches = $derived.by(() => {
		const q = query.trim().toLowerCase();
		if (!q) return skills.items;
		const starts = skills.items.filter((s) => s.name.startsWith(q));
		const rest = skills.items.filter(
			(s) => !s.name.startsWith(q) && (s.name.includes(q) || s.description.toLowerCase().includes(q)),
		);
		return [...starts, ...rest];
	});
	// The rows the arrows walk: the matches, then the create row.
	const rows = $derived(skills.create ? [...matches, skills.create] : matches);

	$effect(() => {
		void query;
		selected = 0;
		listEl?.scrollTo({ top: 0 });
		queueMicrotask(updateFade);
	});

	function updateFade() {
		if (!listEl) return;
		atEnd = listEl.scrollTop + listEl.clientHeight >= listEl.scrollHeight - 2;
	}

	function move(by: number) {
		if (rows.length === 0) return;
		selected = (selected + by + rows.length) % rows.length;
		panelEl
			?.querySelector(`#${CSS.escape(`${uid}-row-${selected}`)}`)
			?.scrollIntoView({ block: "nearest" });
	}

	function onKeydown(e: KeyboardEvent) {
		if (!active || inComposition(e)) return;
		const shell = panelEl?.closest(".chat-input-wrapper");
		if (!shell || !shell.contains(e.target as Node)) return;
		let handled = true;
		if (e.key === "ArrowDown") move(1);
		else if (e.key === "ArrowUp") move(-1);
		else if (e.key === "Escape") onClose();
		else if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey && rows[selected]) onSelect(rows[selected]);
		else handled = false;
		if (handled) {
			e.preventDefault();
			e.stopPropagation();
		}
	}

	onMount(() => {
		// Each opening rereads the list, so a skill saved a moment ago is here.
		void skills.load();
		document.addEventListener("keydown", onKeydown, true);
		return () => document.removeEventListener("keydown", onKeydown, true);
	});
</script>

<!-- A press inside the panel would otherwise take the focus from the field. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="skills-panel" bind:this={panelEl} onmousedown={(e) => e.preventDefault()}>
	<div
		class="skills-list"
		class:fade={!atEnd}
		bind:this={listEl}
		onscroll={updateFade}
		role="listbox"
		aria-label="Skills"
	>
		{#each matches as s, i (s.name)}
			<button
				type="button"
				id="{uid}-row-{i}"
				class="skill-row"
				class:selected={i === selected}
				role="option"
				aria-selected={i === selected}
				onmousemove={() => (selected = i)}
				onclick={() => onSelect(s)}
			>
				<span class="skill-tile"><Icon icon={s.icon ?? "mynaui:zap"} width="15" style="color: inherit" /></span>
				<span class="skill-name">{s.name}</span>
				<span class="skill-description">{s.description}</span>
			</button>
		{:else}
			<p class="skills-empty" role="status">
				{#if !skills.loaded}
					<!-- Nothing while the list is on its way. -->
				{:else if skills.failed}
					Skills didn't load. Check that your server is reachable, then type / again.
				{:else if query}
					No skill matches "{query}".
				{:else}
					You have no skills yet.
				{/if}
			</p>
		{/each}
	</div>
	{#if skills.create}
		{@const i = matches.length}
		<button
			type="button"
			id="{uid}-row-{i}"
			class="skill-row create"
			class:selected={i === selected}
			onmousemove={() => (selected = i)}
			onclick={() => skills.create && onSelect(skills.create)}
		>
			<span class="skill-tile"><Icon icon="mynaui:plus" width="15" style="color: inherit" /></span>
			<span class="skill-name">Create new skill with Virtues</span>
		</button>
	{/if}
</div>

<style>
	.skills-panel {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		width: 100%;
		min-width: 0;
	}

	/* The same fixed height as the @ list, so the tray does not change size
	   as the query narrows it. */
	.skills-list {
		display: flex;
		flex-direction: column;
		height: 10.125rem;
		overflow-y: auto;
		overscroll-behavior: contain;
	}

	.skills-list.fade {
		mask-image: linear-gradient(to bottom, black calc(100% - 2.5rem), transparent);
	}

	.skill-row {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		flex-shrink: 0;
		width: 100%;
		min-height: 2.25rem;
		padding: 0.25rem 0.5rem 0.25rem 0.25rem;
		border-radius: var(--radius-md);
		text-align: left;
		cursor: pointer;
	}

	.skill-row.selected {
		background: var(--hover-bg);
	}

	.skill-tile {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		width: 1.75rem;
		height: 1.75rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		color: var(--color-foreground-muted);
	}

	.skill-name {
		flex-shrink: 0;
		font-size: 0.875rem;
		color: var(--color-foreground);
		white-space: nowrap;
	}

	.skill-description {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.create .skill-name {
		color: var(--color-foreground-muted);
	}

	.skills-empty {
		margin: auto 0;
		padding: 0.75rem 0.5rem;
		text-align: center;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
</style>
