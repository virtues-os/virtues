<script lang="ts">
	// The axiology step is typically a conversational AI experience
	// For now, we'll show a placeholder that explains the next step
</script>

<div class="axiology-step max-w-xl mx-auto">
	<header class="step-header">
		<h1>Discover Your Values</h1>
		<p class="step-description">
			In the final step, you'll have a guided conversation to explore your
			personal values, virtues, and what matters most to you.
		</p>
	</header>

	<div class="values-preview">
		<div class="preview-section">
			<h3>What we'll explore</h3>
			<ul class="exploration-list">
				<li>
					<span class="item-title">Virtues</span>
					<span class="item-description"
						>The qualities you aspire to embody</span
					>
				</li>
				<li>
					<span class="item-title">Values</span>
					<span class="item-description"
						>What you hold most important in life</span
					>
				</li>
				<li>
					<span class="item-title">Temperament</span>
					<span class="item-description"
						>Your natural patterns of thought and behavior</span
					>
				</li>
				<li>
					<span class="item-title">Aspirations</span>
					<span class="item-description"
						>The life you're working toward</span
					>
				</li>
			</ul>
		</div>
	</div>

	<div class="axiology-note">
		<p>
			This conversation takes about 10-15 minutes. Your responses help
			Virtues provide personalized guidance aligned with who you truly
			are.
		</p>
	</div>
</div>

<style>
	.axiology-step {
		width: 100%;
		margin: 0 auto;
	}

	.step-header {
		margin-bottom: 40px;
	}

	h1 {
		font-family: var(--font-serif);
		font-size: 28px;
		font-weight: 400;
		letter-spacing: -0.02em;
		color: var(--foreground);
		margin-bottom: 12px;
	}

	.step-description {
		font-family: var(--font-sans);
		font-size: 15px;
		color: var(--foreground-muted);
		line-height: 1.6;
	}

	.values-preview {
		margin-bottom: 32px;
	}

	.preview-section h3 {
		font-family: var(--font-sans);
		font-size: 12px;
		font-weight: 500;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--foreground-subtle);
		margin-bottom: 16px;
	}

	.exploration-list {
		list-style: none;
		padding: 0;
		margin: 0;
		border-top: 1px solid var(--border);
	}

	.exploration-list li {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 16px 0;
		border-bottom: 1px solid var(--border);
	}

	.item-title {
		font-family: var(--font-serif);
		font-size: 16px;
		font-weight: 400;
		color: var(--foreground);
	}

	.item-description {
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--foreground-muted);
	}

	.axiology-note {
		padding: 20px;
		background: var(--background);
		border-left: 2px solid var(--border);
	}

	.axiology-note p {
		font-family: var(--font-sans);
		font-size: 14px;
		color: var(--foreground-muted);
		line-height: 1.6;
		margin: 0;
	}
</style>
