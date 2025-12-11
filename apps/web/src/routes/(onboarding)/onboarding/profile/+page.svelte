<script lang="ts">
	import { getContext } from "svelte";
	import ThemePicker from "$lib/components/ThemePicker.svelte";
	import { type Theme } from "$lib/utils/theme";

	// Get onboarding context to control continue button and register data
	const { setCanContinue, registerStepData, initialData } = getContext<{
		setCanContinue: (value: boolean) => void;
		registerStepData: (data: Record<string, unknown>) => void;
		initialData: {
			profile?: { preferred_name?: string; employer?: string; theme?: Theme };
			assistantProfile?: { assistant_name?: string };
		};
	}>("onboarding");

	let name = $state(initialData?.profile?.preferred_name || "");
	let occupation = $state(initialData?.profile?.employer || "");
	let assistantName = $state(initialData?.assistantProfile?.assistant_name || "Ari");
	let currentTheme = $state<Theme>(initialData?.profile?.theme || "light");

	// AI assistant name suggestions with etymologies
	const AI_NAMES: { name: string; etymology: string }[] = [
		{ name: "Athena", etymology: "Greek goddess of wisdom and strategic warfare" },
		{ name: "Marcus", etymology: "Latin, dedicated to Mars, god of war" },
		{ name: "Stella", etymology: "Latin for 'star'" },
		{ name: "Nova", etymology: "Latin for 'new'; an exploding star" },
		{ name: "Orion", etymology: "Greek mythological hunter; a constellation" },
		{ name: "Luna", etymology: "Latin for 'moon'" },
		{ name: "Atlas", etymology: "Greek titan who held up the celestial heavens" },
		{ name: "Iris", etymology: "Greek goddess of the rainbow and messenger" },
		{ name: "Sage", etymology: "Latin 'salvus' meaning healthy; also wisdom" },
		{ name: "Phoenix", etymology: "Greek mythical bird reborn from ashes" },
		{ name: "Aria", etymology: "Italian for 'air'; a melodic song" },
		{ name: "Zephyr", etymology: "Greek god of the west wind" },
		{ name: "Cleo", etymology: "Greek for 'glory' or 'pride'" },
		{ name: "Felix", etymology: "Latin for 'happy' or 'fortunate'" },
		{ name: "Aurora", etymology: "Roman goddess of the dawn" },
		{ name: "Jasper", etymology: "Greek/Hebrew for 'spotted stone'; a gemstone" },
		{ name: "Lyra", etymology: "Greek for 'lyre'; a constellation" },
		{ name: "Cyrus", etymology: "Persian for 'sun' or 'throne'" },
		{ name: "Selene", etymology: "Greek goddess of the moon" },
		{ name: "Theo", etymology: "Greek for 'god' or 'divine gift'" },
		{ name: "Minerva", etymology: "Roman goddess of wisdom and arts" },
		{ name: "Apollo", etymology: "Greek god of light, music, and prophecy" },
		{ name: "Daphne", etymology: "Greek for 'laurel tree'" },
		{ name: "Echo", etymology: "Greek nymph who could only repeat others" },
		{ name: "Gaia", etymology: "Greek primordial goddess of Earth" },
		{ name: "Helios", etymology: "Greek personification of the sun" },
		{ name: "Juno", etymology: "Roman queen of the gods" },
		{ name: "Kira", etymology: "Greek 'kyrios' for 'lord'; Irish 'ciar' for 'dark'" },
		{ name: "Leo", etymology: "Latin for 'lion'" },
		{ name: "Mira", etymology: "Latin for 'wonderful'; a variable star" },
		{ name: "Nyx", etymology: "Greek primordial goddess of night" },
		{ name: "Pax", etymology: "Latin for 'peace'" },
		{ name: "Quinn", etymology: "Irish 'Ó Cuinn', descendant of Conn (chief)" },
		{ name: "Rhea", etymology: "Greek titan mother of the gods" },
		{ name: "Sol", etymology: "Latin for 'sun'" },
		{ name: "Thea", etymology: "Greek titan goddess of sight and light" },
		{ name: "Uma", etymology: "Sanskrit for 'tranquility' or 'splendor'" },
		{ name: "Vega", etymology: "Arabic for 'swooping eagle'; brightest star in Lyra" },
		{ name: "Wren", etymology: "Old English; a small songbird" },
		{ name: "Xena", etymology: "Greek for 'hospitable' or 'stranger'" },
		{ name: "Yara", etymology: "Brazilian Tupi for 'water lady'; a river spirit" },
		{ name: "Zara", etymology: "Arabic for 'blooming flower' or 'princess'" },
		{ name: "Aiden", etymology: "Irish for 'little fire'" },
		{ name: "Blair", etymology: "Scottish Gaelic for 'field' or 'plain'" },
		{ name: "Cassius", etymology: "Latin for 'hollow' or 'vain'" },
		{ name: "Diana", etymology: "Roman goddess of the hunt and moon" },
		{ name: "Elara", etymology: "Greek mythology; one of Jupiter's moons" },
		{ name: "Finn", etymology: "Irish for 'fair' or 'white'" },
		{ name: "Grace", etymology: "Latin 'gratia', meaning elegance or blessing" },
		{ name: "Hugo", etymology: "Germanic for 'mind' or 'intellect'" },
		{ name: "Ivy", etymology: "Old English; an evergreen climbing plant" },
		{ name: "Jade", etymology: "Spanish 'piedra de ijada', stone of the side" },
		{ name: "Kai", etymology: "Hawaiian for 'sea'; Japanese for 'forgiveness'" },
		{ name: "Lana", etymology: "Irish 'ailin' (little rock); Hawaiian for 'afloat'" },
		{ name: "Max", etymology: "Latin 'maximus', meaning 'greatest'" },
		{ name: "Nia", etymology: "Welsh for 'purpose'; Swahili for 'intention'" },
		{ name: "Oscar", etymology: "Irish 'os cara' (deer friend) or Norse 'god spear'" },
		{ name: "Petra", etymology: "Greek for 'rock' or 'stone'" },
		{ name: "Quill", etymology: "English; a writing instrument from a feather" },
		{ name: "Raven", etymology: "Old English; an intelligent black bird" },
		{ name: "Silas", etymology: "Latin 'Silvanus' (of the forest); Greek form of Saul" },
		{ name: "Tara", etymology: "Sanskrit for 'star'; Irish for 'hill'" },
		{ name: "Ursa", etymology: "Latin for 'bear'; a constellation" },
		{ name: "Vera", etymology: "Latin for 'truth'; Russian for 'faith'" },
		{ name: "Will", etymology: "Germanic for 'resolute protector'" },
		{ name: "Xander", etymology: "Greek for 'defender of the people'" },
		{ name: "Yuki", etymology: "Japanese for 'snow' or 'happiness'" },
		{ name: "Zoe", etymology: "Greek for 'life'" },
		{ name: "Astra", etymology: "Latin for 'star'" },
		{ name: "Bryn", etymology: "Welsh for 'hill' or 'mound'" },
		{ name: "Cosmo", etymology: "Greek for 'order' or 'universe'" },
		{ name: "Dawn", etymology: "Old English; first light of day" },
		{ name: "Eden", etymology: "Hebrew for 'delight' or 'paradise'" },
		{ name: "Fern", etymology: "Old English; a shade-loving plant" },
		{ name: "Grey", etymology: "Old English; the color between black and white" },
		{ name: "Haven", etymology: "Old English for 'safe place' or 'harbor'" },
		{ name: "Indigo", etymology: "Greek 'indikon', a deep blue-violet dye" },
		{ name: "Jules", etymology: "Latin for 'youthful' or 'downy'" },
		{ name: "Knox", etymology: "Scottish for 'round hill'" },
		{ name: "Lark", etymology: "Old English; a songbird that sings at dawn" },
		{ name: "Maple", etymology: "Old English; a deciduous tree" },
		{ name: "North", etymology: "Old English; cardinal direction" },
		{ name: "Onyx", etymology: "Greek for 'claw'; a black gemstone" },
		{ name: "Pearl", etymology: "Latin 'perla'; a gem from the sea" },
		{ name: "Reese", etymology: "Welsh for 'enthusiasm' or 'ardor'" },
		{ name: "Sable", etymology: "Old French; a dark fur-bearing animal" },
		{ name: "Tatum", etymology: "Old English 'Tata's homestead'" },
		{ name: "Unity", etymology: "Latin 'unitas', meaning oneness" },
		{ name: "Vale", etymology: "Latin for 'valley'" },
		{ name: "Winter", etymology: "Germanic; the coldest season" },
	];

	let nameIndex = $state(0);
	let currentEtymology = $state("");
	let lastRandomizedName = $state("");

	function randomizeName() {
		nameIndex = (nameIndex + 1) % AI_NAMES.length;
		assistantName = AI_NAMES[nameIndex].name;
		lastRandomizedName = assistantName;
		currentEtymology = AI_NAMES[nameIndex].etymology;
	}

	// Clear etymology if user manually changes the name
	$effect(() => {
		if (assistantName !== lastRandomizedName) {
			currentEtymology = "";
		}
	});

	// Update canContinue and register data whenever fields change
	$effect(() => {
		const allFilled =
			!!name.trim() &&
			!!occupation.trim() &&
			!!assistantName.trim() &&
			!!currentTheme;
		setCanContinue(allFilled);
		registerStepData({ name, occupation, assistantName, currentTheme });
	});

	function handleThemeChange(theme: Theme) {
		currentTheme = theme;
	}
</script>

<div class="markdown w-full max-w-xl mx-auto">
	<header>
		<h1 class="text-4xl!">Your Profile</h1>
		<p class="mt-2 text-foreground-muted">
			Tell us a bit about yourself so your Personal AI can personalize
			your experience. All of these fields are editable later in your
			settings.
		</p>
	</header>

	<section class="mt-8">
		<h2>About You</h2>
		<div class="flex flex-col gap-4 mt-4">
			<label class="flex flex-col gap-2">
				<span class="text-sm text-foreground-subtle">Your name</span>
				<input
					type="text"
					class="w-full p-3 bg-surface border border-border rounded-lg text-foreground placeholder:text-foreground-subtle focus:outline-none focus:border-foreground transition-colors"
					placeholder="Enter your name"
					bind:value={name}
				/>
			</label>

			<label class="flex flex-col gap-2">
				<span class="text-sm text-foreground-subtle"
					>Employer or school</span
				>
				<input
					type="text"
					class="w-full p-3 bg-surface border border-border rounded-lg text-foreground placeholder:text-foreground-subtle focus:outline-none focus:border-foreground transition-colors"
					placeholder="e.g., Google, Stanford University"
					bind:value={occupation}
				/>
			</label>
		</div>
	</section>

	<section class="mt-8">
		<h2>Your Personal AI</h2>
		<div class="flex flex-col gap-4 mt-4">
			<div class="flex flex-col gap-2">
				<span class="text-sm text-foreground-subtle"
					>Assistant name</span
				>
				<div class="flex gap-2">
					<input
						type="text"
						class="flex-1 p-3 bg-surface border border-border rounded-lg text-foreground placeholder:text-foreground-subtle focus:outline-none focus:border-foreground transition-colors"
						placeholder="e.g., Athena, Marcus, Stella"
						bind:value={assistantName}
					/>
					<button
						type="button"
						onclick={randomizeName}
						class="px-4 py-2 bg-surface-elevated text-foreground rounded-lg hover:bg-accent/10 transition-colors cursor-pointer text-sm"
					>
						Randomize
					</button>
				</div>
				{#if currentEtymology}
					<p class="text-sm text-foreground-muted italic">{currentEtymology}</p>
				{/if}
			</div>
		</div>
	</section>

	<section class="mt-8">
		<h2>Appearance</h2>
		<p class="mt-2 text-foreground-muted">
			Choose a visual theme for your workspace.
		</p>
		<div class="mt-4">
			<ThemePicker value={currentTheme} onchange={handleThemeChange} />
		</div>
	</section>
</div>
