<script lang="ts">
	import Input from "$lib/components/Input.svelte";

	// Accordion state (collapsed by default)
	let showDetails = $state(false);

	// Delight mode toggle
	let delight = $state(true);

	// Demo input values
	let value = $state("");
	let autoSaveValue = $state("");
	let simulateError = $state(false);
	let labelValue = $state("");
	let warningValue = $state("");
	let clearableValue = $state("Try typing something...");
	let showWarning = $state(false);

	// Simulated save function
	async function handleSave(val: string | number): Promise<void> {
		// Simulate network delay
		await new Promise((resolve) => setTimeout(resolve, 800));
		// Simulate occasional errors for demo
		if (simulateError) {
			throw new Error("Simulated save error");
		}
		console.log("Saved:", val);
	}
</script>

<svelte:head>
	<title>Kitchen · Presence Input</title>
</svelte:head>

<div class="h-full overflow-y-auto bg-background py-16 px-6">
	<div class="max-w-2xl mx-auto">
		<!-- Header (always visible) -->
		<header class="mb-10 text-center">
			<h1 class="font-serif text-3xl text-foreground">Presence</h1>
		</header>

		<!-- The Input (always visible) -->
		<section class="mb-8 space-y-6">
			<div>
				<Input
					bind:value
					placeholder="What's on your mind?"
					{delight}
				/>
			</div>
			<div>
				<Input
					bind:value={autoSaveValue}
					placeholder="Your display name..."
					{delight}
					autoSave
					onSave={handleSave}
				/>
			</div>
			<div>
				<Input
					bind:value={labelValue}
					label="Full Name"
					helperText="This will be used in your profile"
					placeholder="Enter your name"
					{delight}
					required
				/>
			</div>
			<div>
				<Input
					bind:value={warningValue}
					label="Password"
					helperText="Use at least 8 characters"
					warning={showWarning ? "Password is weak" : false}
					placeholder="Enter password"
					type="password"
					{delight}
				/>
			</div>
			<div>
				<Input
					bind:value={clearableValue}
					label="Search"
					helperText="Type to search, click X to clear"
					placeholder="Search..."
					clearable
					{delight}
				/>
			</div>
		</section>

		<!-- Accordion Toggle -->
		<button
			type="button"
			class="accordion-toggle"
			onclick={() => (showDetails = !showDetails)}
			aria-expanded={showDetails}
		>
			<span>Details</span>
			<svg
				width="16"
				height="16"
				viewBox="0 0 16 16"
				fill="none"
				class="accordion-icon"
				class:open={showDetails}
			>
				<path
					d="M4 6L8 10L12 6"
					stroke="currentColor"
					stroke-width="1.5"
					stroke-linecap="round"
					stroke-linejoin="round"
				/>
			</svg>
		</button>

		<!-- Collapsible Content -->
		<div class="accordion-content" class:open={showDetails}>
			<div class="accordion-inner">
				<!-- Description -->
				<p class="description">
					An input that's aware of you — with subtle state indicators
					for hover, focus, typing, paused, and complete.
				</p>

				<!-- Toggles -->
				<div class="toggles">
					<div class="toggle-row">
						<span class="text-sm text-foreground-subtle"
							>Delight mode</span
						>
						<button
							type="button"
							class="toggle"
							class:on={delight}
							onclick={() => (delight = !delight)}
							aria-pressed={delight}
							aria-label="Toggle delight mode"
						>
							<span class="toggle-thumb"></span>
						</button>
						<span class="text-xs text-foreground-subtle font-mono">
							{delight ? "on" : "off"}
						</span>
					</div>
					<div class="toggle-row">
						<span class="text-sm text-foreground-subtle"
							>Simulate save error</span
						>
						<button
							type="button"
							class="toggle"
							class:on={simulateError}
							onclick={() => (simulateError = !simulateError)}
							aria-pressed={simulateError}
							aria-label="Toggle simulate save error"
						>
							<span class="toggle-thumb"></span>
						</button>
						<span class="text-xs text-foreground-subtle font-mono">
							{simulateError ? "on" : "off"}
						</span>
					</div>
					<div class="toggle-row">
						<span class="text-sm text-foreground-subtle"
							>Show warning state</span
						>
						<button
							type="button"
							class="toggle"
							class:on={showWarning}
							onclick={() => (showWarning = !showWarning)}
							aria-pressed={showWarning}
							aria-label="Toggle warning state"
						>
							<span class="toggle-thumb"></span>
						</button>
						<span class="text-xs text-foreground-subtle font-mono">
							{showWarning ? "on" : "off"}
						</span>
					</div>
				</div>

				<!-- Ideas Section -->
				<section class="ideas">
					<h2 class="font-serif text-lg text-foreground mb-4">
						More state ideas
					</h2>
					<div class="ideas-grid">
						<div class="idea">
							<div class="idea-icon">⚠️</div>
							<div>
								<strong>Error / Invalid</strong>
								<p>Red X with subtle shake</p>
							</div>
						</div>
						<div class="idea">
							<div class="idea-icon">◔</div>
							<div>
								<strong>Character limit</strong>
								<p>Progress ring near max</p>
							</div>
						</div>
						<div class="idea">
							<div class="idea-icon">⟳</div>
							<div>
								<strong>Validating</strong>
								<p>Spinner for async checks</p>
							</div>
						</div>
						<div class="idea">
							<div class="idea-icon">✦</div>
							<div>
								<strong>Success flourish</strong>
								<p>Brief pulse on complete</p>
							</div>
						</div>
						<div class="idea">
							<div class="idea-icon">↩</div>
							<div>
								<strong>Cleared</strong>
								<p>Animation when deleted</p>
							</div>
						</div>
						<div class="idea">
							<div class="idea-icon">📋</div>
							<div>
								<strong>Paste detected</strong>
								<p>Brief acknowledgment</p>
							</div>
						</div>
					</div>
				</section>

				<!-- Footer -->
				<footer class="footer">
					<p class="text-foreground-subtle text-xs">
						The goal: one effect that sparks joy without ever
						becoming noise.
					</p>
				</footer>
			</div>
		</div>
	</div>
</div>

<style>
	/* ===================================
	   ACCORDION
	   =================================== */
	.accordion-toggle {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		width: 100%;
		padding: 8px;
		background: none;
		border: none;
		color: var(--color-foreground-subtle);
		font-family: var(--font-sans);
		font-size: 13px;
		cursor: pointer;
		transition: color 0.2s ease;
	}

	.accordion-toggle:hover {
		color: var(--color-foreground-muted);
	}

	.accordion-icon {
		transition: transform 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	.accordion-icon.open {
		transform: rotate(180deg);
	}

	.accordion-content {
		display: grid;
		grid-template-rows: 0fr;
		transition: grid-template-rows 0.3s ease;
	}

	.accordion-content.open {
		grid-template-rows: 1fr;
	}

	.accordion-inner {
		overflow: hidden;
	}

	.description {
		text-align: center;
		color: var(--color-foreground-muted);
		font-size: 14px;
		margin: 16px 0 24px;
		line-height: 1.5;
	}

	.toggles {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin-bottom: 24px;
	}

	.toggle-row {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 12px;
	}

	.footer {
		margin-top: 24px;
		padding-top: 16px;
		border-top: 1px solid var(--color-border-subtle);
		text-align: center;
	}

	/* ===================================
	   TOGGLE
	   =================================== */
	.toggle {
		width: 36px;
		height: 20px;
		background: var(--color-border);
		border: none;
		border-radius: 10px;
		cursor: pointer;
		position: relative;
		transition: background 0.2s ease;
	}

	.toggle.on {
		background: var(--color-primary);
	}

	.toggle-thumb {
		position: absolute;
		top: 2px;
		left: 2px;
		width: 16px;
		height: 16px;
		background: white;
		border-radius: 50%;
		transition: transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
	}

	.toggle.on .toggle-thumb {
		transform: translateX(16px);
	}

	/* ===================================
	   IDEAS SECTION
	   =================================== */
	.ideas {
		padding: 20px;
		background: var(--color-surface-elevated);
		border-radius: 12px;
	}

	.ideas-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 12px;
	}

	.idea {
		display: flex;
		gap: 10px;
		padding: 10px;
		background: var(--color-surface);
		border-radius: 8px;
		border: 1px solid var(--color-border-subtle);
	}

	.idea-icon {
		font-size: 14px;
		width: 20px;
		text-align: center;
		flex-shrink: 0;
	}

	.idea strong {
		display: block;
		font-size: 13px;
		color: var(--color-foreground);
		margin-bottom: 1px;
	}

	.idea p {
		font-size: 11px;
		color: var(--color-foreground-subtle);
		line-height: 1.3;
	}
</style>
