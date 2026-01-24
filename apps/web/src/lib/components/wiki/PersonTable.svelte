<!--
	PersonTable.svelte

	Attio-style table view for managing people in the wiki.
	Clean, full-width, with badges and relative dates.
-->

<script lang="ts">
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import { getAllPersons, addPerson } from "$lib/wiki";
	import type { PersonPage, ContactFrequency } from "$lib/wiki/types";
	import "iconify-icon";

	// Reactive list of people
	let people = $state(getAllPersons());

	// New row state
	let newName = $state("");
	let newRelationship = $state("");
	let isAddingRow = $state(false);
	let nameInputRef: HTMLInputElement | undefined = $state();

	// Relationship options
	const relationshipOptions = [
		"Best Friend",
		"Friend",
		"Family",
		"Partner",
		"Colleague",
		"Manager",
		"Mentor",
		"Client",
		"Acquaintance",
	];

	// Relationship badge colors
	const relationshipColors: Record<string, string> = {
		"Best Friend": "badge-purple",
		"Friend": "badge-blue",
		"Family": "badge-pink",
		"Colleague": "badge-green",
		"Mentor": "badge-orange",
		"Acquaintance": "badge-gray",
		"Partner": "badge-pink",
		"Manager": "badge-green",
		"Client": "badge-blue",
	};

	// Frequency badge colors
	const frequencyColors: Record<ContactFrequency, string> = {
		daily: "badge-green",
		weekly: "badge-blue",
		monthly: "badge-purple",
		quarterly: "badge-orange",
		yearly: "badge-gray",
		rare: "badge-gray",
		"lost-touch": "badge-red",
	};

	// Format contact frequency for display
	function formatFrequency(freq?: ContactFrequency): string | null {
		if (!freq) return null;
		const labels: Record<ContactFrequency, string> = {
			daily: "Daily",
			weekly: "Weekly",
			monthly: "Monthly",
			quarterly: "Quarterly",
			yearly: "Yearly",
			rare: "Rarely",
			"lost-touch": "Lost touch",
		};
		return labels[freq] || freq;
	}

	// Format relative date
	function formatRelativeDate(date?: Date): string | null {
		if (!date) return null;
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

		if (diffDays === 0) return "Today";
		if (diffDays === 1) return "Yesterday";
		if (diffDays < 7) return `${diffDays} days ago`;
		if (diffDays < 14) return "About 1 week ago";
		if (diffDays < 30) return `${Math.floor(diffDays / 7)} weeks ago`;
		if (diffDays < 60) return "About 1 month ago";
		if (diffDays < 365) return `${Math.floor(diffDays / 30)} months ago`;
		return `${Math.floor(diffDays / 365)} years ago`;
	}

	// Get badge class for relationship
	function getRelationshipClass(rel: string): string {
		return relationshipColors[rel] || "badge-gray";
	}

	// Get badge class for frequency
	function getFrequencyClass(freq?: ContactFrequency): string {
		if (!freq) return "badge-gray";
		return frequencyColors[freq] || "badge-gray";
	}

	// Handle row click - navigate to person page
	function handleRowClick(person: PersonPage) {
		workspaceStore.openTabFromRoute(`/wiki/${person.slug}`);
	}

	// Start adding a new row
	function startAddingRow() {
		isAddingRow = true;
		setTimeout(() => nameInputRef?.focus(), 0);
	}

	// Handle adding a new person
	function handleAddPerson() {
		if (!newName.trim()) return;

		addPerson({
			name: newName.trim(),
			relationship: newRelationship.trim() || "Contact",
		});

		people = getAllPersons();
		newName = "";
		newRelationship = "";
		isAddingRow = false;
	}

	// Handle key events in the add row
	function handleKeyDown(e: KeyboardEvent) {
		if (e.key === "Enter" && newName.trim()) {
			handleAddPerson();
		} else if (e.key === "Escape") {
			newName = "";
			newRelationship = "";
			isAddingRow = false;
		}
	}

	// Handle blur
	function handleBlur() {
		setTimeout(() => {
			const activeEl = document.activeElement;
			const isInAddRow = activeEl?.closest(".add-row");
			if (!isInAddRow) {
				if (newName.trim()) {
					handleAddPerson();
				} else {
					isAddingRow = false;
				}
			}
		}, 100);
	}
</script>

<div class="table-wrapper">
	<!-- Toolbar -->
	<div class="table-toolbar">
		<div class="toolbar-left">
			<button class="toolbar-btn">
				<iconify-icon icon="ri:arrow-up-down-line" width="14"></iconify-icon>
				Sort
			</button>
			<button class="toolbar-btn">
				<iconify-icon icon="ri:filter-3-line" width="14"></iconify-icon>
				Filter
			</button>
		</div>
		<button class="toolbar-btn toolbar-btn-primary" onclick={startAddingRow}>
			<iconify-icon icon="ri:add-line" width="14"></iconify-icon>
			Add person
		</button>
	</div>

	<!-- Table -->
	<div class="table-container">
		<table class="data-table">
			<thead>
				<tr>
					<th class="col-name">
						<iconify-icon icon="ri:user-line" width="14"></iconify-icon>
						Name
					</th>
					<th class="col-relationship">
						<iconify-icon icon="ri:heart-line" width="14"></iconify-icon>
						Relationship
					</th>
					<th class="col-location">
						<iconify-icon icon="ri:map-pin-line" width="14"></iconify-icon>
						Location
					</th>
					<th class="col-frequency">
						<iconify-icon icon="ri:time-line" width="14"></iconify-icon>
						Frequency
					</th>
					<th class="col-last-contact">
						<iconify-icon icon="ri:calendar-line" width="14"></iconify-icon>
						Last Contact
					</th>
				</tr>
			</thead>
			<tbody>
				{#each people as person}
					<tr class="data-row" onclick={() => handleRowClick(person)}>
						<td class="col-name">
							<div class="name-cell">
								<div class="avatar">
									{person.title.charAt(0).toUpperCase()}
								</div>
								<span class="name-text">{person.title}</span>
							</div>
						</td>
						<td class="col-relationship">
							<span class="badge {getRelationshipClass(person.relationship)}">
								{person.relationship}
							</span>
						</td>
						<td class="col-location">
							{#if person.currentLocation}
								<span class="location-text">{person.currentLocation}</span>
							{:else}
								<span class="empty-cell">—</span>
							{/if}
						</td>
						<td class="col-frequency">
							{#if person.contactFrequency}
								<span class="badge {getFrequencyClass(person.contactFrequency)}">
									{formatFrequency(person.contactFrequency)}
								</span>
							{:else}
								<span class="empty-cell">—</span>
							{/if}
						</td>
						<td class="col-last-contact">
							{#if person.lastContact}
								<span class="date-text">{formatRelativeDate(person.lastContact)}</span>
							{:else}
								<span class="empty-cell">—</span>
							{/if}
						</td>
					</tr>
				{/each}

				<!-- Add row -->
				{#if isAddingRow}
					<tr class="add-row">
						<td class="col-name">
							<div class="name-cell">
								<div class="avatar avatar-empty">+</div>
								<input
									bind:this={nameInputRef}
									type="text"
									class="inline-input"
									placeholder="Name"
									bind:value={newName}
									onkeydown={handleKeyDown}
									onblur={handleBlur}
								/>
							</div>
						</td>
						<td class="col-relationship">
							<select
								class="inline-select"
								bind:value={newRelationship}
								onkeydown={handleKeyDown}
								onblur={handleBlur}
							>
								<option value="">Select...</option>
								{#each relationshipOptions as opt}
									<option value={opt}>{opt}</option>
								{/each}
							</select>
						</td>
						<td class="col-location"><span class="empty-cell">—</span></td>
						<td class="col-frequency"><span class="empty-cell">—</span></td>
						<td class="col-last-contact"><span class="empty-cell">—</span></td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>

	
	<!-- Empty state -->
	{#if people.length === 0 && !isAddingRow}
		<div class="empty-state">
			<iconify-icon icon="ri:user-add-line" width="32"></iconify-icon>
			<p>No people yet</p>
			<button class="empty-add-btn" onclick={startAddingRow}>Add your first person</button>
		</div>
	{/if}
</div>

<style>
	.table-wrapper {
		width: 100%;
		padding: 0 2rem;
		overflow: visible;
	}

	/* Toolbar */
	.table-toolbar {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.5rem 0;
		position: relative;
	}

	.table-toolbar::after {
		content: "";
		position: absolute;
		left: -2rem;
		right: -2rem;
		bottom: 0;
		height: 1px;
		background: var(--color-border);
	}

	.toolbar-left {
		display: flex;
		gap: 0.5rem;
	}

	.toolbar-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.625rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.toolbar-btn:hover {
		background: var(--color-background-hover);
		color: var(--color-foreground);
	}

	.toolbar-btn-primary {
		color: var(--color-foreground);
		border-color: var(--color-border-strong);
	}

	/* Table container */
	.table-container {
		overflow: visible;
	}

	.data-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8125rem;
		overflow: visible;
	}

	/* Header */
	thead tr {
		background: transparent;
	}

	th {
		text-align: left;
		font-weight: 500;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		padding: 0.625rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
		white-space: nowrap;
	}

	th:first-child {
		padding-left: 0;
	}

	th:last-child {
		padding-right: 0;
	}

	/* Full-width borders using pseudo-element */
	thead tr {
		position: relative;
	}

	thead tr::after {
		content: "";
		position: absolute;
		left: -2rem;
		right: -2rem;
		bottom: 0;
		height: 1px;
		background: var(--color-border);
	}

	th {
		border-bottom: none;
	}

	th iconify-icon {
		vertical-align: -2px;
		margin-right: 0.375rem;
		opacity: 0.7;
	}

	/* Column widths */
	.col-name {
		width: 30%;
		min-width: 200px;
	}

	.col-relationship {
		width: 18%;
		min-width: 120px;
	}

	.col-location {
		width: 22%;
		min-width: 140px;
	}

	.col-frequency {
		width: 15%;
		min-width: 100px;
	}

	.col-last-contact {
		width: 15%;
		min-width: 120px;
	}

	/* Data rows */
	td {
		padding: 0.5rem 0.75rem;
		color: var(--color-foreground);
		vertical-align: middle;
	}

	td:first-child {
		padding-left: 0;
	}

	td:last-child {
		padding-right: 0;
	}

	.data-row {
		cursor: pointer;
		transition: background-color 0.1s ease;
		position: relative;
	}

	.data-row::after {
		content: "";
		position: absolute;
		left: -2rem;
		right: -2rem;
		bottom: 0;
		height: 1px;
		background: var(--color-border);
	}

	.data-row:hover {
		background: var(--color-background-hover);
	}


	/* Name cell with avatar */
	.name-cell {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.avatar {
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: linear-gradient(135deg, var(--color-primary), color-mix(in srgb, var(--color-primary) 70%, #000));
		color: white;
		font-size: 0.625rem;
		font-weight: 600;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.avatar-empty {
		background: var(--color-border);
		color: var(--color-foreground-muted);
	}

	.name-text {
		font-weight: 500;
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* Badges */
	.badge {
		display: inline-flex;
		align-items: center;
		padding: 0.125rem 0.5rem;
		font-size: 0.75rem;
		font-weight: 500;
		border-radius: 9999px;
		white-space: nowrap;
	}

	.badge-gray {
		background: color-mix(in srgb, var(--color-foreground) 10%, transparent);
		color: var(--color-foreground-muted);
	}

	.badge-blue {
		background: color-mix(in srgb, #3b82f6 15%, transparent);
		color: #2563eb;
	}

	.badge-green {
		background: color-mix(in srgb, #22c55e 15%, transparent);
		color: #16a34a;
	}

	.badge-purple {
		background: color-mix(in srgb, #8b5cf6 15%, transparent);
		color: #7c3aed;
	}

	.badge-pink {
		background: color-mix(in srgb, #ec4899 15%, transparent);
		color: #db2777;
	}

	.badge-orange {
		background: color-mix(in srgb, #f97316 15%, transparent);
		color: #ea580c;
	}

	.badge-red {
		background: color-mix(in srgb, #ef4444 15%, transparent);
		color: #dc2626;
	}

	/* Text styles */
	.location-text {
		color: var(--color-foreground);
	}

	.date-text {
		color: var(--color-foreground-muted);
	}

	.empty-cell {
		color: var(--color-foreground-subtle);
	}

	/* Add row */
	.add-row td {
		padding: 0.5rem 0.75rem;
		background: color-mix(in srgb, var(--color-primary) 3%, transparent);
	}

	.add-row td:first-child {
		padding-left: 0;
	}

	.add-row td:last-child {
		padding-right: 0;
	}

	.inline-input {
		width: 100%;
		max-width: 180px;
		padding: 0.375rem 0.5rem;
		font-size: 0.8125rem;
		color: var(--color-foreground);
		background: var(--color-background);
		border: 1px solid var(--color-border);
		border-radius: 4px;
		outline: none;
	}

	.inline-input:focus {
		border-color: var(--color-primary);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-primary) 20%, transparent);
	}

	.inline-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.inline-input-sm {
		max-width: 120px;
	}

	.inline-select {
		padding: 0.25rem 0.5rem;
		font-size: 0.75rem;
		color: var(--color-foreground);
		background: var(--color-background);
		border: 1px solid var(--color-border);
		border-radius: 9999px;
		outline: none;
		cursor: pointer;
	}

	.inline-select:focus {
		border-color: var(--color-primary);
	}

	/* Empty state */
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
		padding: 3rem 2rem;
		color: var(--color-foreground-muted);
	}

	.empty-state iconify-icon {
		opacity: 0.5;
	}

	.empty-state p {
		margin: 0;
		font-size: 0.875rem;
	}

	.empty-add-btn {
		padding: 0.5rem 1rem;
		font-size: 0.8125rem;
		font-weight: 500;
		color: white;
		background: var(--color-primary);
		border: none;
		border-radius: 6px;
		cursor: pointer;
		transition: opacity 0.15s ease;
	}

	.empty-add-btn:hover {
		opacity: 0.9;
	}

	/* Responsive */
	@media (max-width: 768px) {
		.col-location,
		.col-frequency,
		.col-last-contact {
			display: none;
		}

		.col-name {
			width: 60%;
		}

		.col-relationship {
			width: 40%;
		}

		th,
		td {
			padding: 0.625rem 0.75rem;
		}
	}
</style>
