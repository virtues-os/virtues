<script lang="ts">
	import { Page } from '$lib';
	import { goto } from '$app/navigation';
	import 'iconify-icon';
	import type { PageData } from './$types';
	import type {
		Chunk,
		LocationChunk,
		TransitChunk,
		MissingDataChunk
	} from './+page';

	export let data: PageData;

	// Rome timezone for display
	const ROME_TIMEZONE = 'Europe/Rome';

	// Reactive state for date/time picker
	let selectedDate = data.selectedDate;
	let selectedHour = 0; // Start hour for the 24-hour window

	// Navigate when date/time changes
	function updateUrl() {
		goto(`/timeline?date=${selectedDate}`, { replaceState: true });
	}

	// Format single time (HH:MM)
	function formatTime(date: Date): string {
		return date.toLocaleString('en-US', {
			timeZone: ROME_TIMEZONE,
			hour: '2-digit',
			minute: '2-digit',
			hour12: false
		});
	}

	// Format duration nicely
	function formatDuration(minutes: number): string {
		if (minutes < 60) return `${minutes}m`;
		const hours = Math.floor(minutes / 60);
		const remainingMins = minutes % 60;
		return remainingMins > 0 ? `${hours}h ${remainingMins}m` : `${hours}h`;
	}

	// Format date for header
	function formatDate(dateStr: string): string {
		const date = new Date(`${dateStr}T12:00:00`);
		return date.toLocaleDateString('en-US', {
			weekday: 'long',
			month: 'long',
			day: 'numeric',
			year: 'numeric'
		});
	}

	// Get location display name
	function getLocationName(chunk: LocationChunk): string {
		if (chunk.place_name) return chunk.place_name;
		return `${chunk.latitude.toFixed(2)}°N, ${chunk.longitude.toFixed(2)}°E`;
	}

	// Get missing data reason text
	function getMissingReason(reason: string): string {
		const reasons: Record<string, string> = {
			sleep: 'Sleep',
			indoors: 'Indoors',
			phone_off: 'Phone off',
			unknown: 'Unknown'
		};
		return reasons[reason] || reason;
	}

	// Count total attached items for a location chunk
	function countAttachedItems(chunk: LocationChunk): number {
		return (
			chunk.messages.length +
			chunk.transcripts.length +
			chunk.calendar_events.length +
			chunk.emails.length +
			chunk.health_events.length
		);
	}

	// Generate hour markers (0-24)
	function getHourMarkers(): number[] {
		return Array.from({ length: 25 }, (_, i) => i);
	}

	// Compute window start/end based on selected date + hour
	$: windowStart = new Date(`${data.dayView.date}T${selectedHour.toString().padStart(2, '0')}:00:00+01:00`); // Rome timezone
	$: windowEnd = new Date(windowStart.getTime() + 24 * 60 * 60 * 1000);

	// Filter chunks that overlap with the 24-hour window
	$: visibleChunks = data.dayView.chunks.filter((chunk) => {
		const chunkStart = new Date(chunk.start_time);
		const chunkEnd = new Date(chunk.end_time);
		return chunkStart < windowEnd && chunkEnd > windowStart;
	});

	// Calculate block position and width from chunk times
	function getBlockStyle(chunk: Chunk): { left: string; width: string } | null {
		const chunkStart = new Date(chunk.start_time);
		const chunkEnd = new Date(chunk.end_time);

		// Clamp to window boundaries
		const clampedStart = new Date(Math.max(chunkStart.getTime(), windowStart.getTime()));
		const clampedEnd = new Date(Math.min(chunkEnd.getTime(), windowEnd.getTime()));

		if (clampedStart >= clampedEnd) return null;

		const startPercent = ((clampedStart.getTime() - windowStart.getTime()) / (24 * 60 * 60 * 1000)) * 100;
		const endPercent = ((clampedEnd.getTime() - windowStart.getTime()) / (24 * 60 * 60 * 1000)) * 100;
		const widthPercent = endPercent - startPercent;

		return {
			left: `${startPercent}%`,
			width: `${Math.max(widthPercent, 0.5)}%`
		};
	}

	// Get chunk styling classes
	function getChunkClasses(chunk: Chunk): string {
		if (chunk.type === 'location') {
			return chunk.is_known_place
				? 'bg-blue-400 border-blue-500'
				: 'bg-blue-300 border-blue-400';
		} else if (chunk.type === 'transit') {
			return 'bg-neutral-300 border-neutral-400';
		} else {
			return 'bg-surface-elevated border-border border-dashed';
		}
	}

	// Track selected chunk for details panel
	let selectedChunkIndex: number | null = null;

	function selectChunk(index: number) {
		selectedChunkIndex = selectedChunkIndex === index ? null : index;
	}

	// Summary stats
	$: locationCount = data.dayView.chunks.filter((c) => c.type === 'location').length;
	$: transitCount = data.dayView.chunks.filter((c) => c.type === 'transit').length;
	$: hourMarkers = getHourMarkers();
</script>

<Page>
	<div class="max-w-7xl">
		<!-- Header -->
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">Timeline</h1>
			<p class="text-foreground-muted mb-4">Location-first day view</p>

			<!-- Date/Time Picker -->
			<div class="flex items-center gap-4 mb-4">
				<div class="flex items-center gap-2">
					<label for="date" class="text-sm text-foreground-muted">Date:</label>
					<input
						id="date"
						type="date"
						bind:value={selectedDate}
						on:change={updateUrl}
						class="px-3 py-1.5 border border-border rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-primary"
					/>
				</div>
				<div class="flex items-center gap-2">
					<label for="hour" class="text-sm text-foreground-muted">Start hour:</label>
					<select
						id="hour"
						bind:value={selectedHour}
						class="px-3 py-1.5 border border-border rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-primary"
					>
						{#each Array(24) as _, i}
							<option value={i}>{i.toString().padStart(2, '0')}:00</option>
						{/each}
					</select>
				</div>
			</div>
		</div>

		{#if data.error}
			<!-- Error State -->
			<div class="border-2 border-error rounded-lg p-8 text-center bg-error-subtle">
				<iconify-icon icon="ri:error-warning-line" class="text-4xl text-error mb-2"></iconify-icon>
				<h3 class="text-lg font-medium text-foreground mb-1">Error loading timeline data</h3>
				<p class="text-foreground-muted">{data.error}</p>
			</div>
		{:else if data.dayView.chunks.length === 0}
			<!-- No Data State -->
			<div class="border-2 border-dashed border-border rounded-lg p-8 text-center">
				<iconify-icon icon="ri:database-line" class="text-4xl text-foreground-subtle mb-2"></iconify-icon>
				<h3 class="text-lg font-medium text-foreground mb-1">No timeline data for this day</h3>
				<p class="text-foreground-muted mb-3">Try selecting a different date or run the seed script</p>
				<code class="text-sm text-foreground-muted px-3 py-2 rounded" style="background-color: #F3F2E9;">
					make db-reset && make seed
				</code>
			</div>
		{:else}
			<!-- Day Summary -->
			<div class="mb-6">
				<h2 class="text-lg font-serif font-medium text-foreground mb-2">
					{formatDate(data.dayView.date)}
				</h2>
				<div class="flex flex-wrap gap-4 text-sm text-foreground-muted">
					<span>{locationCount} location{locationCount !== 1 ? 's' : ''}</span>
					<span>·</span>
					<span>{transitCount} transit{transitCount !== 1 ? 's' : ''}</span>
					<span>·</span>
					<span>{formatDuration(data.dayView.total_location_minutes)} at places</span>
					<span>·</span>
					<span>{formatDuration(data.dayView.total_transit_minutes)} moving</span>
				</div>
			</div>

			<!-- Horizontal Timeline -->
			<div class="relative bg-surface border border-border rounded-lg p-6">
				<!-- Hour grid with labels -->
				<div class="relative">
					<!-- Hour lines and labels -->
					<div class="absolute inset-x-0 top-0 h-16 flex">
						{#each hourMarkers as hour}
							<div
								class="absolute top-0 bottom-0 w-px bg-surface-elevated"
								style="left: {(hour / 24) * 100}%;"
							></div>
						{/each}
					</div>

					<!-- Timeline track -->
					<div class="relative h-16">
						{#each visibleChunks as chunk, index}
							{@const style = getBlockStyle(chunk)}
							{#if style}
								<button
									class="absolute top-2 bottom-2 {getChunkClasses(chunk)} border rounded cursor-pointer hover:brightness-95 transition-all {selectedChunkIndex === index ? 'ring-2 ring-primary ring-offset-1' : ''}"
									style="left: {style.left}; width: {style.width};"
									on:click={() => selectChunk(index)}
								>
									<!-- Minimal content inside block -->
									<span class="sr-only">
										{chunk.type === 'location' ? getLocationName(chunk) : chunk.type}
									</span>
								</button>
							{/if}
						{/each}
					</div>

					<!-- Hour labels -->
					<div class="relative h-5 mt-1">
						{#each hourMarkers as hour}
							{#if hour % 3 === 0}
								<div
									class="absolute text-[10px] text-foreground-subtle -translate-x-1/2"
									style="left: {(hour / 24) * 100}%;"
								>
									{((selectedHour + hour) % 24).toString().padStart(2, '0')}:00
								</div>
							{/if}
						{/each}
					</div>
				</div>
			</div>

			<!-- Selected Chunk Details -->
			{#if selectedChunkIndex !== null && visibleChunks[selectedChunkIndex]}
				{@const chunk = visibleChunks[selectedChunkIndex]}
				<div class="mt-4 p-4 bg-surface-elevated border border-border rounded-lg">
					{#if chunk.type === 'location'}
						<div class="flex items-start gap-3">
							<iconify-icon
								icon={chunk.is_known_place ? 'ri:map-pin-fill' : 'ri:map-pin-line'}
								class="text-primary text-xl mt-0.5"
							></iconify-icon>
							<div class="flex-1">
								<div class="flex items-baseline justify-between gap-2 mb-1">
									<span class="font-medium text-foreground">
										{getLocationName(chunk)}
									</span>
									<span class="text-sm text-foreground-subtle">
										{formatDuration(chunk.duration_minutes)}
									</span>
								</div>
								<div class="text-sm text-foreground-muted mb-3">
									{formatTime(new Date(chunk.start_time))} - {formatTime(new Date(chunk.end_time))}
								</div>

								<!-- Attached data -->
								{#if countAttachedItems(chunk) > 0}
									<div class="space-y-2 pt-2 border-t border-border">
										{#each chunk.messages as msg}
											<div class="flex items-start gap-2 text-sm">
												<iconify-icon icon="ri:chat-1-line" class="text-orange-500 mt-0.5"></iconify-icon>
												<span class="text-foreground-muted">{msg.from_name || msg.channel}: {msg.body_preview}</span>
											</div>
										{/each}
										{#each chunk.transcripts as transcript}
											<div class="flex items-start gap-2 text-sm">
												<iconify-icon icon="ri:mic-line" class="text-purple-500 mt-0.5"></iconify-icon>
												<span class="text-foreground-muted">{transcript.transcript_preview}</span>
											</div>
										{/each}
										{#each chunk.calendar_events as event}
											<div class="flex items-start gap-2 text-sm">
												<iconify-icon icon="ri:calendar-line" class="text-green-500 mt-0.5"></iconify-icon>
												<span class="text-foreground-muted">{event.title}</span>
											</div>
										{/each}
										{#each chunk.emails as email}
											<div class="flex items-start gap-2 text-sm">
												<iconify-icon icon="ri:mail-line" class="text-red-500 mt-0.5"></iconify-icon>
												<span class="text-foreground-muted">{email.from_name || 'Email'}: {email.subject || '(no subject)'}</span>
											</div>
										{/each}
										{#each chunk.health_events as health}
											<div class="flex items-start gap-2 text-sm">
												<iconify-icon icon="ri:heart-pulse-line" class="text-pink-500 mt-0.5"></iconify-icon>
												<span class="text-foreground-muted">{health.description}</span>
											</div>
										{/each}
									</div>
								{/if}
							</div>
						</div>
					{:else if chunk.type === 'transit'}
						<div class="flex items-start gap-3">
							<iconify-icon icon="ri:walk-line" class="text-foreground-subtle text-xl mt-0.5"></iconify-icon>
							<div class="flex-1">
								<div class="flex items-baseline justify-between gap-2 mb-1">
									<span class="font-medium text-foreground">Transit</span>
									<span class="text-sm text-foreground-subtle">{formatDuration(chunk.duration_minutes)}</span>
								</div>
								<div class="text-sm text-foreground-muted">
									{formatTime(new Date(chunk.start_time))} - {formatTime(new Date(chunk.end_time))}
									<span class="ml-2">{chunk.distance_km.toFixed(1)} km · {chunk.avg_speed_kmh.toFixed(0)} km/h</span>
								</div>
								{#if chunk.from_place || chunk.to_place}
									<div class="text-sm text-foreground-subtle mt-1">
										{chunk.from_place || '?'} → {chunk.to_place || '?'}
									</div>
								{/if}
							</div>
						</div>
					{:else if chunk.type === 'missing_data'}
						<div class="flex items-start gap-3">
							<iconify-icon icon="ri:question-line" class="text-foreground-subtle text-xl mt-0.5"></iconify-icon>
							<div class="flex-1">
								<div class="flex items-baseline justify-between gap-2 mb-1">
									<span class="font-medium text-foreground-muted">Missing Data</span>
									<span class="text-sm text-foreground-subtle">{formatDuration(chunk.duration_minutes)}</span>
								</div>
								<div class="text-sm text-foreground-muted">
									{formatTime(new Date(chunk.start_time))} - {formatTime(new Date(chunk.end_time))}
									<span class="ml-2 text-foreground-subtle">({getMissingReason(chunk.likely_reason)})</span>
								</div>
								{#if chunk.last_known_location || chunk.next_known_location}
									<div class="text-sm text-foreground-subtle mt-1">
										{chunk.last_known_location || '?'} → {chunk.next_known_location || '?'}
									</div>
								{/if}
							</div>
						</div>
					{/if}
				</div>
			{/if}

			<!-- Legend -->
			<div class="flex flex-wrap gap-4 text-xs mt-6">
				<div class="flex items-center gap-2">
					<div class="w-4 h-4 bg-blue-400 border border-blue-500 rounded"></div>
					<span class="text-foreground-muted">Location</span>
				</div>
				<div class="flex items-center gap-2">
					<div class="w-4 h-4 bg-neutral-300 border border-neutral-400 rounded"></div>
					<span class="text-foreground-muted">Transit</span>
				</div>
				<div class="flex items-center gap-2">
					<div class="w-4 h-4 bg-surface-elevated border-2 border-dashed border-border rounded"></div>
					<span class="text-foreground-muted">Missing Data</span>
				</div>
			</div>
		{/if}
	</div>
</Page>
