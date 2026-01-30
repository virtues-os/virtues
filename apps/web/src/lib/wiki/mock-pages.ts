/**
 * Mock Wiki Pages
 *
 * Example pages demonstrating each page type.
 * These serve as templates for the wiki UX.
 */

import type {
	WikiPage,
	ActPage,
	YearPage,
	PersonPage,
	PlacePage,
	OrganizationPage,
	DayPage,
	DayEvent,
	ContextVector,
	LinkedEntities,
	LinkedTemporal,
} from "./types";
import { emptyLinkedEntities, emptyLinkedTemporal } from "./types";

// =============================================================================
// ACT: Current - Building Virtues
// =============================================================================

export const ACT_BUILDING_VIRTUES: ActPage = {
	id: "act-building-virtues",
	type: "act",
	slug: "building-virtues",
	title: "Building Virtues",
	subtitle: "Creating something meaningful (2023–present)",

	period: { start: new Date(2023, 0, 1), end: undefined }, // ongoing
	location: "Austin, Texas",

	content: `The current act. Building a company that embodies the philosophical principles I've spent years learning.

## Chapters

- **[[Foundation]]** — The first year of building
- **[[First Users]]** — Opening to early adopters`,

	telos: { displayName: "The Good Life", pageSlug: "the-good-life", pageType: "telos" },
	chapters: [
		{ displayName: "Foundation", pageSlug: "foundation-2023", pageType: "chapter" },
		{ displayName: "First Users", pageSlug: "first-users", pageType: "chapter" },
	],
	keyPeople: [
		{ displayName: "Sarah Chen", pageSlug: "sarah-chen", pageType: "person" },
	],
	keyPlaces: [
		{ displayName: "Austin, TX", pageSlug: "austin-tx", pageType: "place" },
	],
	themes: ["creation", "philosophy", "technology", "purpose"],

	infobox: {
		fields: [
			{ label: "Period", value: "January 2023 – present" },
			{ label: "Location", value: "Austin, Texas" },
		],
		links: [],
	},

	citations: [],
	linkedPages: [],
	relatedPages: [],
	tags: ["current", "entrepreneurship", "philosophy"],
	createdAt: new Date(2024, 0, 1),
	updatedAt: new Date(2024, 11, 15),
	lastEditedBy: "human",
};

// =============================================================================
// ACT: Tech Career
// =============================================================================

export const ACT_TECH_CAREER: ActPage = {
	id: "act-tech-career",
	type: "act",
	slug: "tech-career",
	title: "Tech Career",
	subtitle: "Learning to build (2014–2022)",

	period: { start: new Date(2014, 5, 1), end: new Date(2022, 11, 31) },
	location: "San Francisco / Austin",

	content: `Eight years in the tech industry. Learned to build, to ship, to lead.

## Key Chapters

- **[[First Job]]** — Startup chaos and rapid learning
- **[[Big Tech]]** — Scale and stability
- **[[Leadership]]** — Managing teams
- **[[Exit]]** — Leaving to find something more`,

	telos: undefined,
	chapters: [
		{ displayName: "First Job", pageSlug: "first-job", pageType: "chapter" },
		{ displayName: "Big Tech", pageSlug: "big-tech", pageType: "chapter" },
		{ displayName: "Leadership", pageSlug: "leadership-chapter", pageType: "chapter" },
		{ displayName: "Exit", pageSlug: "exit-2022", pageType: "chapter" },
	],
	keyPeople: [],
	keyPlaces: [
		{ displayName: "San Francisco", pageSlug: "san-francisco", pageType: "place" },
	],
	themes: ["career", "technology", "growth"],

	infobox: {
		fields: [
			{ label: "Period", value: "June 2014 – December 2022" },
			{ label: "Location", value: "San Francisco, then Austin" },
		],
		links: [],
	},

	citations: [],
	linkedPages: [],
	relatedPages: [],
	tags: ["career", "technology"],
	createdAt: new Date(2024, 1, 1),
	updatedAt: new Date(2024, 10, 1),
	lastEditedBy: "ai",
};

// =============================================================================
// ACT: College Years
// =============================================================================

export const ACT_COLLEGE_YEARS: ActPage = {
	id: "act-college-years",
	type: "act",
	slug: "college-years",
	title: "College Years",
	subtitle: "Finding myself at UT Austin (2010–2014)",

	// Temporal
	period: { start: new Date(2010, 7, 1), end: new Date(2014, 4, 31) },
	location: "Austin, Texas",

	// Narrative
	content: `The four years at [[UT Austin]] marked a transformative period of intellectual awakening and personal struggle. This was when I first encountered [[Stoicism]][1], battled depression, and formed friendships that endure today—particularly with [[Sarah Chen]] and [[Tom Wilson]].

Looking back, these years feel both impossibly distant and viscerally present. The person who arrived in Austin in 2010 and the one who left in 2014 share a name but little else. College was where I learned to think, to question, to fail, and eventually to endure.

## The Beginning

I arrived at UT in August 2010[2], fresh from a small town with big ambitions but no real direction. The first semester was disorienting—I changed my major three times before settling on Philosophy after a chance encounter with [[Prof. Miller]].

The dormitory was nothing like I'd imagined—smaller, noisier, more chaotic. My roommate, [[Tom Wilson]], seemed impossibly cool and confident. It would take months before we became actual friends.

## Chapters

This act contains four distinct chapters, each representing a significant phase:

- **[[Freshman Year]]** — Adjustment, loneliness, first friendships
- **[[The Philosophy Discovery]]** — Finding my intellectual home
- **[[Depression and Recovery]]** — The dark period and climbing out
- **[[Senior Year]]** — Integration and preparation for what comes next

## The Library

I spent an almost absurd amount of time at the [[Main Library]][3]. It became my sanctuary—a place where the chaos of social life faded and I could lose myself in ideas.

> "The library was where I first understood that ideas could be companions."

## Significance

This act established several patterns that would persist through my life: my relationship with [[Stoicism]] as a framework and my tendency toward isolation under stress.`,

	// Connections
	telos: undefined, // Could link to a life purpose page
	chapters: [
		{ displayName: "Freshman Year", pageSlug: "freshman-year", pageType: "chapter" },
		{ displayName: "The Philosophy Discovery", pageSlug: "philosophy-discovery", pageType: "chapter" },
		{ displayName: "Depression and Recovery", pageSlug: "depression-recovery", pageType: "chapter" },
		{ displayName: "Senior Year", pageSlug: "senior-year", pageType: "chapter" },
	],
	keyPeople: [
		{ displayName: "Sarah Chen", pageSlug: "sarah-chen", pageType: "person", preview: "Best friend since college" },
		{ displayName: "Tom Wilson", pageSlug: "tom-wilson", pageType: "person" },
		{ displayName: "Prof. Miller", pageSlug: "prof-miller", pageType: "person" },
	],
	keyPlaces: [
		{ displayName: "UT Austin", pageSlug: "ut-austin", pageType: "place" },
		{ displayName: "Main Library", pageSlug: "main-library", pageType: "place" },
	],
	themes: ["education", "philosophy", "personal-growth", "friendship", "struggle"],

	// Infobox
	infobox: {
		fields: [
			{ label: "Period", value: "August 2010 – May 2014" },
			{ label: "Location", value: "Austin, Texas" },
			{ label: "Institution", value: "University of Texas at Austin" },
			{ label: "Degree", value: "B.A. Philosophy" },
		],
		links: [
			{ label: "Sarah Chen", pageSlug: "sarah-chen" },
			{ label: "Stoicism", pageSlug: "stoicism" },
		],
	},

	// Citations
	citations: [
		{
			id: "c1",
			index: 1,
			sourceType: "ontology",
			label: "Meditations by Marcus Aurelius",
			preview: "First read October 2011, highlighted 47 passages",
			timestamp: new Date(2011, 9, 1),
			addedBy: "human",
		},
		{
			id: "c2",
			index: 2,
			sourceType: "ontology",
			label: "Email to Mom",
			preview: '"Just moved into my dorm! The roommate seems nice..."',
			timestamp: new Date(2010, 7, 15),
			addedBy: "ai",
		},
		{
			id: "c3",
			index: 3,
			sourceType: "aggregated",
			label: "847 visits to Main Library",
			preview: "Location data aggregated from 2010-2014",
			addedBy: "ai",
		},
	],

	// Linked pages (for [[wiki link]] resolution)
	linkedPages: [
		{ displayName: "UT Austin", pageSlug: "ut-austin", pageType: "place" },
		{ displayName: "Sarah Chen", pageSlug: "sarah-chen", pageType: "person", preview: "Best friend since college" },
		{ displayName: "Tom Wilson", pageSlug: "tom-wilson", pageType: "person" },
		{ displayName: "Prof. Miller", pageSlug: "prof-miller", pageType: "person" },
		{ displayName: "Freshman Year", pageSlug: "freshman-year", pageType: "chapter" },
		{ displayName: "The Philosophy Discovery", pageSlug: "philosophy-discovery", pageType: "chapter" },
		{ displayName: "Depression and Recovery", pageSlug: "depression-recovery", pageType: "chapter" },
		{ displayName: "Senior Year", pageSlug: "senior-year", pageType: "chapter" },
		{ displayName: "Main Library", pageSlug: "main-library", pageType: "place" },
	],

	relatedPages: [],

	tags: ["education", "philosophy", "personal-growth", "formative"],
	createdAt: new Date(2024, 0, 15),
	updatedAt: new Date(2024, 11, 10),
	lastEditedBy: "ai",
};

// =============================================================================
// PERSON: Sarah Chen
// =============================================================================

export const PERSON_SARAH_CHEN: PersonPage = {
	id: "person-sarah-chen",
	type: "person",
	slug: "sarah-chen",
	title: "Sarah Chen",
	subtitle: "Best friend since college",

	// Identity
	nickname: "Sare",
	relationship: "Best Friend",
	connectionTier: "inner-circle",

	// Contact Info
	emails: ["sarah.chen@example.com"],
	phones: ["+1 (206) 555-0147"],
	socials: {
		linkedin: "sarahchen",
		twitter: "sarahchen_bio",
	},

	// About
	location: "Seattle, WA",
	company: "University of Washington",
	role: "Bioethicist",
	birthday: new Date(1991, 3, 12), // April 12

	// Notes
	content: `Met in [[Freshman Year]] philosophy class. Paired for a project at the [[Main Library]]. She noticed when I was struggling and pushed me to get help.

Weekly calls every Sunday, 7 PM her time. Shares my interest in [[Stoicism]].`,

	// Infobox
	infobox: {
		fields: [
			{ label: "Relationship", value: "Best Friend" },
			{ label: "Location", value: "Seattle, WA" },
			{ label: "Work", value: "Bioethicist at UW" },
		],
		links: [
			{ label: "College Years", pageSlug: "college-years" },
			{ label: "Main Library", pageSlug: "main-library" },
		],
	},

	// Citations
	citations: [
		{
			id: "c1",
			index: 1,
			sourceType: "ontology",
			label: "PHIL 201 Course Roster",
			preview: "Project pairing: Sarah Chen, January 2011",
			timestamp: new Date(2011, 0, 18),
			addedBy: "ai",
		},
	],

	// Linked pages
	linkedPages: [
		{ displayName: "Freshman Year", pageSlug: "freshman-year", pageType: "chapter" },
		{ displayName: "Main Library", pageSlug: "main-library", pageType: "place" },
	],

	relatedPages: [{ slug: "college-years", title: "College Years", pageType: "act" }],

	tags: ["friendship", "college", "core-relationship"],
	createdAt: new Date(2024, 1, 1),
	updatedAt: new Date(2024, 11, 10),
	lastEditedBy: "human",
};

// =============================================================================
// PLACE: Main Library
// =============================================================================

export const PLACE_MAIN_LIBRARY: PlacePage = {
	id: "place-main-library",
	type: "place",
	slug: "main-library",
	title: "Main Library",
	subtitle: "Perry-Castañeda Library, UT Austin",

	// Location
	coordinates: { lat: 30.2827, lng: -97.7381 },
	address: "101 E 21st St, Austin, TX 78712",
	city: "Austin, Texas",
	placeType: "third-place",

	// Temporal
	firstVisit: new Date(2010, 8, 1),
	lastVisit: new Date(2014, 4, 15),
	visitCount: 847,

	// Narrative
	content: `The Perry-Castañeda Library—PCL to everyone at UT—became my sanctuary during college. I spent hundreds of hours here, most of them alone with books.

## Why It Mattered

The library was where I did my best thinking. Something about the quiet, the smell of old books, the sense of being surrounded by accumulated human knowledge.

> "The library was where I first understood that ideas could be companions."

## The Spots

- **5th floor stacks** — Philosophy section, my usual territory
- **Reading room** — For when I needed formality
- **Basement café** — Late night coffee and cramming

## Key Moments

This is where I first read Marcus Aurelius[1]. Where I met [[Sarah Chen]] for our first project meeting. Where I hid during the worst of my depression, finding comfort in routine.`,

	significance: "Intellectual sanctuary during college years",

	// Connections
	associatedPeople: [
		{ displayName: "Sarah Chen", pageSlug: "sarah-chen", pageType: "person" },
		{ displayName: "Prof. Miller", pageSlug: "prof-miller", pageType: "person" },
	],
	activities: ["Reading", "Studying", "Writing", "Thinking"],
	narrativeContext: [
		{ displayName: "College Years", pageSlug: "college-years", pageType: "act" },
	],

	// Infobox
	infobox: {
		fields: [
			{ label: "Type", value: "University Library" },
			{ label: "Location", value: "UT Austin Campus" },
			{ label: "First Visit", value: "September 2010" },
			{ label: "Total Visits", value: "847" },
		],
		links: [
			{ label: "College Years", pageSlug: "college-years" },
			{ label: "Sarah Chen", pageSlug: "sarah-chen" },
		],
	},

	// Citations
	citations: [
		{
			id: "c1",
			index: 1,
			sourceType: "aggregated",
			label: "Location history",
			preview: "847 visits from 2010-2014",
			addedBy: "ai",
		},
	],

	// Linked pages
	linkedPages: [
		{ displayName: "Sarah Chen", pageSlug: "sarah-chen", pageType: "person" },
		{ displayName: "Prof. Miller", pageSlug: "prof-miller", pageType: "person" },
	],

	relatedPages: [{ slug: "college-years", title: "College Years", pageType: "act" }],

	tags: ["library", "college", "sanctuary", "austin"],
	createdAt: new Date(2024, 1, 1),
	updatedAt: new Date(2024, 11, 1),
	lastEditedBy: "ai",
};

// =============================================================================
// ORGANIZATION: UT Austin
// =============================================================================

export const ORG_UT_AUSTIN: OrganizationPage = {
	id: "org-ut-austin",
	type: "organization",
	slug: "ut-austin",
	title: "University of Texas at Austin",
	subtitle: "Where I studied philosophy (2010–2014)",

	// Classification
	orgType: "school",

	// Involvement
	period: { start: new Date(2010, 7, 1), end: new Date(2014, 4, 31) },
	role: "Student, B.A. Philosophy",

	// Narrative
	content: `The University of Texas at Austin was where I spent four transformative years studying philosophy and finding myself.

## Why UT

I chose UT for practical reasons—in-state tuition, strong academics—but it became so much more. The philosophy department introduced me to [[Stoicism]] and gave me [[Prof. Miller]] as a mentor.

## The Experience

College was equal parts intellectual awakening and personal struggle. The [[Main Library]] became my sanctuary.`,

	// Connections
	keyContacts: [
		{ displayName: "Prof. Miller", pageSlug: "prof-miller", pageType: "person" },
		{ displayName: "Sarah Chen", pageSlug: "sarah-chen", pageType: "person" },
	],
	locations: [
		{ displayName: "Main Library", pageSlug: "main-library", pageType: "place" },
	],
	narrativeContext: [
		{ displayName: "College Years", pageSlug: "college-years", pageType: "act" },
	],

	// Infobox
	infobox: {
		fields: [
			{ label: "Type", value: "University" },
			{ label: "Period", value: "2010–2014" },
			{ label: "Role", value: "Student" },
			{ label: "Degree", value: "B.A. Philosophy" },
		],
		links: [
			{ label: "College Years", pageSlug: "college-years" },
		],
	},

	// Citations
	citations: [],

	// Linked pages
	linkedPages: [
		{ displayName: "Prof. Miller", pageSlug: "prof-miller", pageType: "person" },
		{ displayName: "Main Library", pageSlug: "main-library", pageType: "place" },
	],

	relatedPages: [{ slug: "college-years", title: "College Years", pageType: "act" }],

	tags: ["education", "philosophy", "formative"],
	createdAt: new Date(2024, 1, 1),
	updatedAt: new Date(2024, 11, 1),
	lastEditedBy: "ai",
};

// =============================================================================
// DAY PAGE: December 10, 2024
// =============================================================================

const MOCK_DAY_EVENTS: DayEvent[] = [
	{
		id: "1",
		startTime: new Date(2025, 11, 10, 0, 0),
		endTime: new Date(2025, 11, 10, 7, 0),
		durationMinutes: 420,
		autoLabel: "Sleep",
		autoLocation: "Home",
		sourceIds: ["health_sleep"],
		isUserAdded: false,
		isUserEdited: false,
	},
	{
		id: "u1",
		startTime: new Date(2025, 11, 10, 7, 0),
		endTime: new Date(2025, 11, 10, 8, 15),
		durationMinutes: 75,
		autoLabel: "Unknown",
		sourceIds: [],
		isUnknown: true,
		isUserAdded: false,
		isUserEdited: false,
	},
	{
		id: "2",
		startTime: new Date(2025, 11, 10, 8, 15),
		endTime: new Date(2025, 11, 10, 8, 35),
		durationMinutes: 20,
		autoLabel: "Transit",
		sourceIds: ["location_point"],
		isTransit: true,
		isUserAdded: false,
		isUserEdited: true,
		userLabel: "Transit to office",
	},
	{
		id: "3",
		startTime: new Date(2025, 11, 10, 8, 35),
		endTime: new Date(2025, 11, 10, 12, 0),
		durationMinutes: 205,
		autoLabel: "Work",
		autoLocation: "Office",
		sourceIds: ["location_visit", "calendar"],
		isUserAdded: false,
		isUserEdited: false,
	},
	{
		id: "4",
		startTime: new Date(2025, 11, 10, 12, 0),
		endTime: new Date(2025, 11, 10, 13, 0),
		durationMinutes: 60,
		autoLabel: "Lunch",
		autoLocation: "Café",
		sourceIds: ["financial_transaction", "location_visit"],
		isUserAdded: false,
		isUserEdited: false,
	},
	{
		id: "5",
		startTime: new Date(2025, 11, 10, 13, 0),
		endTime: new Date(2025, 11, 10, 17, 0),
		durationMinutes: 240,
		autoLabel: "Work",
		autoLocation: "Office",
		sourceIds: ["location_visit", "calendar"],
		isUserAdded: false,
		isUserEdited: false,
	},
	{
		id: "6",
		startTime: new Date(2025, 11, 10, 17, 0),
		endTime: new Date(2025, 11, 10, 17, 30),
		durationMinutes: 30,
		autoLabel: "Transit",
		sourceIds: ["location_point", "health_steps"],
		isTransit: true,
		isUserAdded: false,
		isUserEdited: true,
		userLabel: "Walking home",
	},
	{
		id: "7",
		startTime: new Date(2025, 11, 10, 17, 30),
		endTime: new Date(2025, 11, 10, 19, 0),
		durationMinutes: 90,
		autoLabel: "Workout",
		autoLocation: "Gym",
		sourceIds: ["health_workout", "location_visit"],
		isUserAdded: false,
		isUserEdited: false,
	},
	{
		id: "u2",
		startTime: new Date(2025, 11, 10, 19, 0),
		endTime: new Date(2025, 11, 10, 20, 30),
		durationMinutes: 90,
		autoLabel: "Unknown",
		sourceIds: [],
		isUnknown: true,
		isUserAdded: false,
		isUserEdited: false,
	},
	{
		id: "8",
		startTime: new Date(2025, 11, 10, 20, 30),
		endTime: new Date(2025, 11, 10, 21, 30),
		durationMinutes: 60,
		autoLabel: "Call",
		autoLocation: "Home",
		sourceIds: ["calendar"],
		isUserAdded: false,
		isUserEdited: true,
		userLabel: "Call with Sarah",
	},
	{
		id: "u3",
		startTime: new Date(2025, 11, 10, 21, 30),
		endTime: new Date(2025, 11, 11, 0, 0),
		durationMinutes: 150,
		autoLabel: "Unknown",
		sourceIds: [],
		isUnknown: true,
		isUserAdded: false,
		isUserEdited: false,
	},
];

const MOCK_CONTEXT_VECTOR: ContextVector = {
	when: 0.92,
	where: 0.85,
	who: 0.58,
	what: 0.75,
	why: 0.22,
	how: 0.45,
};

const MOCK_LINKED_ENTITIES: LinkedEntities = {
	people: [
		{
			displayName: "Sarah Chen",
			pageSlug: "sarah-chen",
			pageType: "person",
			preview: "Best friend since college",
		},
	],
	places: [
		{ displayName: "Home", pageSlug: "home", pageType: "place" },
		{ displayName: "Office", pageSlug: "office", pageType: "place" },
		{ displayName: "Café", pageSlug: "lunch-cafe", pageType: "place" },
		{ displayName: "Gym", pageSlug: "gym", pageType: "place" },
	],
	organizations: [],
};

const MOCK_LINKED_TEMPORAL: LinkedTemporal = {
	act: {
		displayName: "Living Intentionally",
		pageSlug: "living-intentionally",
		pageType: "act",
		preview: "2022–Present",
	},
	chapter: {
		displayName: "Building Virtues",
		pageSlug: "building-virtues",
		pageType: "chapter",
		preview: "The current chapter",
	},
	previousDay: { displayName: "Tuesday, December 9, 2025", pageSlug: "2025-12-09", pageType: "day" },
	nextDay: { displayName: "Thursday, December 11, 2025", pageSlug: "2025-12-11", pageType: "day" },
	events: [
		{
			displayName: "Call with Sarah Chen",
			pageSlug: "2024-12-10-call-sarah",
			pageType: "day", // Notable moment, could become its own page
			preview: "The pivot of the day",
		},
	],
	related: [
		{
			displayName: "Sunday, December 8, 2024",
			pageSlug: "2024-12-08",
			pageType: "day",
			preview: "Similar day pattern",
		},
	],
};

export const MOCK_DAY_PAGE: DayPage = {
	id: "day-2025-12-10",
	type: "day",
	slug: "2025-12-10",
	title: "Wednesday, December 10, 2025",
	subtitle: "A day reconstructed from personal data streams",

	// Temporal identity
	date: new Date(2025, 11, 10),
	dayOfWeek: "Wednesday",
	startTimezone: "America/Los_Angeles",
	endTimezone: null,

	// Layer 1: Data
	contextVector: MOCK_CONTEXT_VECTOR,
	linkedEntities: MOCK_LINKED_ENTITIES,
	linkedTemporal: MOCK_LINKED_TEMPORAL,

	// Layer 2: Timeline
	events: MOCK_DAY_EVENTS,

	// Layer 3: Autobiography
	autobiography: `## Morning

Sleep data shows 6h 18m, ending at 6:42[1]. Heart rate elevated slightly during the final REM cycle—likely the alarm. First location ping at 7:23 places me at the kitchen, then a 20-minute gap before transit began.

The commute logs show I was reading something on my phone—screen time data marks 18 minutes of Kindle activity. Based on my library, probably the Marcus Aurelius I've been working through.

## Afternoon

Work block from 8:35 to 12:00. Calendar shows "Architecture Review" from 10-11, which aligns with the elevated typing activity and three Slack threads that started during that window. The transaction at 12:07—$14.20 at the café downstairs—marks the lunch break.

The message thread with [[Sarah Chen]] started at 14:12. Seven exchanges over 23 minutes. The gap from 14:25–15:10 has no data—phone stayed in one location, no app activity, no heart rate spikes. Either I was reading, thinking, or just staring out the window.[6] [7]

## Evening

Transit home logged at 17:00. The workout from 17:30–19:00 shows up in both location (gym) and health data—42 minutes of elevated heart rate, categorized as "functional training."

The call with [[Sarah Chen]] ran 20:30–21:30. Calendar had it blocked as "Catch up" but based on the earlier message thread, it was probably about the team situation I've been avoiding. She has a way of asking questions that make the obvious suddenly visible.

## Patterns

The data shows a day split between focused work and scattered attention. The afternoon gap is interesting—no inputs, no outputs, just... processing. The evening call with Sarah appears connected to the mid-day messages. [[Stoicism]] keeps coming up in my reading logs lately.`,

	// Infobox
	infobox: {
		fields: [
			{ label: "Date", value: "December 10, 2025" },
			{ label: "Day", value: "Wednesday" },
			{ label: "Focus", value: "Narrative built from real streams" },
			{
				label: "Primary Sources",
				value: "location · microphone · health · transactions · calendar",
			},
		],
		links: [
			{ label: "Sarah Chen", pageSlug: "sarah-chen" },
			{ label: "Stoicism", pageSlug: "stoicism" },
		],
	},

	// Citations
	citations: [
		{
			id: "c1",
			index: 1,
			sourceType: "ontology",
			label: "location_point",
			preview: "GPS points and derived place/transit segments",
			timestamp: new Date(2025, 11, 10),
			addedBy: "ai",
		},
		{
			id: "c2",
			index: 2,
			sourceType: "ontology",
			label: "health_heart_rate",
			preview: "Resting vs active vs workout HR samples",
			timestamp: new Date(2025, 11, 10),
			addedBy: "ai",
		},
		{
			id: "c3",
			index: 3,
			sourceType: "ontology",
			label: "health_sleep",
			preview: "Sleep window and quality metrics",
			timestamp: new Date(2025, 11, 10),
			addedBy: "ai",
		},
		{
			id: "c4",
			index: 4,
			sourceType: "ontology",
			label: "speech_transcription",
			preview: "Raw speech snippets grouped into sessions",
			timestamp: new Date(2025, 11, 10),
			addedBy: "ai",
		},
		{
			id: "c5",
			index: 5,
			sourceType: "ontology",
			label: "financial_transaction",
			preview: "Purchases with merchant/category/location",
			timestamp: new Date(2025, 11, 10),
			addedBy: "ai",
		},
		{
			id: "c6",
			index: 6,
			sourceType: "ontology",
			label: "praxis_calendar",
			preview: "Scheduled blocks and meeting metadata",
			timestamp: new Date(2025, 11, 10),
			addedBy: "ai",
		},
		{
			id: "c7",
			index: 7,
			sourceType: "ontology",
			label: "social_message",
			preview: "Message thread around the mid-day change",
			timestamp: new Date(2025, 11, 10),
			addedBy: "ai",
		},
	],

	// Linked pages
	linkedPages: [
		{
			displayName: "Sarah Chen",
			pageSlug: "sarah-chen",
			pageType: "person",
		},
	],

	relatedPages: [],

	tags: ["daily", "timeline", "ontology", "transactions", "biometrics", "transcription"],
	content: "", // DayPage uses autobiography field for content
	createdAt: new Date(2025, 11, 10),
	updatedAt: new Date(2025, 11, 10),
	lastEditedBy: "human",
};

// =============================================================================
// EXPORTS
// =============================================================================

export const MOCK_PAGES: Record<string, WikiPage> = {
	// Acts
	"building-virtues": ACT_BUILDING_VIRTUES,
	"tech-career": ACT_TECH_CAREER,
	"college-years": ACT_COLLEGE_YEARS,
	// Entities
	"sarah-chen": PERSON_SARAH_CHEN,
	"main-library": PLACE_MAIN_LIBRARY,
	"ut-austin": ORG_UT_AUSTIN,
	// Days
	"2025-12-10": MOCK_DAY_PAGE,
};

export function getPageBySlug(slug: string): WikiPage | undefined {
	return MOCK_PAGES[slug];
}

export function getAllPages(): WikiPage[] {
	return Object.values(MOCK_PAGES);
}

/**
 * Get all Act pages, sorted by period start (most recent first).
 */
export function getAllActs(): ActPage[] {
	return Object.values(MOCK_PAGES)
		.filter((p): p is ActPage => p.type === "act")
		.sort((a, b) => b.period.start.getTime() - a.period.start.getTime());
}

/**
 * Get the current (ongoing) act, if any.
 */
export function getCurrentAct(): ActPage | undefined {
	return getAllActs().find((act) => act.period.end === undefined);
}

/**
 * Get mock activity data for the heatmap.
 * Returns a map of date slugs to activity levels (0-4).
 */
export function getMockActivityData(): Map<string, number> {
	const data = new Map<string, number>();

	// Generate some realistic-looking activity over the past year
	const today = new Date();
	for (let i = 0; i < 365; i++) {
		const date = new Date(today);
		date.setDate(date.getDate() - i);
		const slug = date.toISOString().split("T")[0];

		// Generate pseudo-random but deterministic activity level
		const dayOfWeek = date.getDay();
		const weekOfYear = Math.floor(i / 7);

		// Lower activity on weekends
		const baseChance = dayOfWeek === 0 || dayOfWeek === 6 ? 0.3 : 0.7;

		// Use date as seed for pseudo-randomness
		const seed = date.getDate() + date.getMonth() * 31 + weekOfYear;
		const random = Math.sin(seed) * 10000;
		const normalizedRandom = random - Math.floor(random);

		if (normalizedRandom < baseChance) {
			// Assign activity level based on another pseudo-random value
			const levelSeed = seed * 1.5;
			const levelRandom = Math.sin(levelSeed) * 10000;
			const normalizedLevel = levelRandom - Math.floor(levelRandom);

			if (normalizedLevel < 0.3) data.set(slug, 1);
			else if (normalizedLevel < 0.6) data.set(slug, 2);
			else if (normalizedLevel < 0.85) data.set(slug, 3);
			else data.set(slug, 4);
		}
	}

	// Mark the specific mock day page as having high activity
	data.set("2024-12-10", 4);

	return data;
}

export function getDayPageBySlug(slug: string): DayPage | undefined {
	const page = MOCK_PAGES[slug];
	if (page?.type === "day") {
		return page as DayPage;
	}
	return undefined;
}

// =============================================================================
// LAZY DAY PAGE CREATION
// =============================================================================

const DAY_NAMES = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

function formatDayTitle(date: Date): string {
	const dayName = DAY_NAMES[date.getDay()];
	return `${dayName}, ${date.toLocaleDateString("en-US", {
		month: "long",
		day: "numeric",
		year: "numeric",
	})}`;
}

/**
 * Get or create a day page by slug.
 * If the slug is a valid date (YYYY-MM-DD) and no page exists,
 * creates an empty stub page.
 */
export function getOrCreateDayPage(slug: string): DayPage | undefined {
	// Check if page already exists
	const existing = MOCK_PAGES[slug];
	if (existing?.type === "day") {
		return existing as DayPage;
	}

	// Validate slug is a date (YYYY-MM-DD)
	const dateMatch = slug.match(/^(\d{4})-(\d{2})-(\d{2})$/);
	if (!dateMatch) {
		return undefined;
	}

	// Parse and validate the date
	const [, yearStr, monthStr, dayStr] = dateMatch;
	const year = parseInt(yearStr, 10);
	const month = parseInt(monthStr, 10) - 1; // JS months are 0-indexed
	const day = parseInt(dayStr, 10);

	const date = new Date(year, month, day);

	// Validate it's a real date (not Feb 30, etc.)
	if (
		date.getFullYear() !== year ||
		date.getMonth() !== month ||
		date.getDate() !== day
	) {
		return undefined;
	}

	// Create empty stub page
	const emptyPage: DayPage = {
		id: slug,
		slug,
		type: "day",
		title: formatDayTitle(date),

		// Temporal identity
		date,
		dayOfWeek: DAY_NAMES[date.getDay()],
		startTimezone: "America/Chicago",
		endTimezone: null,

		// Layer 1: Data (empty)
		contextVector: { when: 0, where: 0, who: 0, what: 0, why: 0, how: 0 },
		linkedEntities: emptyLinkedEntities(),
		linkedTemporal: emptyLinkedTemporal(),

		// Layer 2: Timeline (empty)
		events: [],

		// Layer 3: Autobiography (empty)
		autobiography: "",

		// Standard fields
		citations: [],
		linkedPages: [],
		tags: [],
		content: "", // DayPage uses autobiography field for content
		createdAt: new Date(),
		updatedAt: new Date(),
		lastEditedBy: "ai",
	};

	// Cache it for subsequent requests
	MOCK_PAGES[slug] = emptyPage;

	return emptyPage;
}

// =============================================================================
// LAZY YEAR PAGE CREATION
// =============================================================================

/**
 * Get or create a year page by slug.
 * If the slug is a valid year (YYYY) and no page exists,
 * creates an empty stub page.
 */
/**
 * Get all Person pages.
 */
export function getAllPersons(): PersonPage[] {
	return Object.values(MOCK_PAGES)
		.filter((p): p is PersonPage => p.type === "person")
		.sort((a, b) => a.title.localeCompare(b.title));
}

/**
 * Get all Place pages.
 */
export function getAllPlaces(): PlacePage[] {
	return Object.values(MOCK_PAGES)
		.filter((p): p is PlacePage => p.type === "place")
		.sort((a, b) => a.title.localeCompare(b.title));
}

/**
 * Get all Organization pages.
 */
export function getAllOrganizations(): OrganizationPage[] {
	return Object.values(MOCK_PAGES)
		.filter((p): p is OrganizationPage => p.type === "organization")
		.sort((a, b) => a.title.localeCompare(b.title));
}

// =============================================================================
// CREATE PERSON
// =============================================================================

function generateSlug(name: string): string {
	return name
		.toLowerCase()
		.trim()
		.replace(/[^a-z0-9\s-]/g, "")
		.replace(/\s+/g, "-")
		.replace(/-+/g, "-");
}

/**
 * Create a new person and add to the wiki.
 * Returns the created page.
 */
export function addPerson(data: {
	name: string;
	relationship: string;
	subtitle?: string;
	location?: string;
	connectionTier?: PersonPage["connectionTier"];
	emails?: string[];
	phones?: string[];
	company?: string;
	role?: string;
}): PersonPage {
	let slug = generateSlug(data.name);

	// Ensure unique slug
	let counter = 1;
	while (MOCK_PAGES[slug]) {
		slug = `${generateSlug(data.name)}-${counter}`;
		counter++;
	}

	const now = new Date();

	const personPage: PersonPage = {
		id: `person-${slug}`,
		type: "person",
		slug,
		title: data.name,
		subtitle: data.subtitle,

		// Identity
		relationship: data.relationship,
		connectionTier: data.connectionTier,

		// Contact Info
		emails: data.emails,
		phones: data.phones,

		// About
		location: data.location,
		company: data.company,
		role: data.role,

		// Notes
		content: "",

		// Infobox
		infobox: {
			fields: [
				{ label: "Relationship", value: data.relationship },
				...(data.location
					? [{ label: "Location", value: data.location }]
					: []),
			],
			links: [],
		},

		// Standard fields
		citations: [],
		linkedPages: [],
		relatedPages: [],
		tags: [],
		createdAt: now,
		updatedAt: now,
		lastEditedBy: "human",
	};

	// Add to pages store
	MOCK_PAGES[slug] = personPage;

	return personPage;
}

// =============================================================================
// LAZY YEAR PAGE CREATION
// =============================================================================

export function getOrCreateYearPage(slug: string): YearPage | undefined {
	// Check if page already exists
	const existing = MOCK_PAGES[slug];
	if (existing?.type === "year") {
		return existing as YearPage;
	}

	// Validate slug is a year (YYYY)
	const yearMatch = slug.match(/^(\d{4})$/);
	if (!yearMatch) {
		return undefined;
	}

	const year = parseInt(yearMatch[1], 10);

	// Validate reasonable year range
	if (year < 1900 || year > 2100) {
		return undefined;
	}

	// Get acts that overlap with this year
	const allActs = getAllActs();
	const overlappingActs = allActs.filter((act) => {
		const actStartYear = act.period.start.getFullYear();
		const actEndYear = act.period.end?.getFullYear() ?? new Date().getFullYear();
		return year >= actStartYear && year <= actEndYear;
	});

	// Generate month summaries
	const months = Array.from({ length: 12 }, (_, i) => {
		const month = i + 1;
		const daysInMonth = new Date(year, month, 0).getDate();
		// For now, simulate some activity based on overlapping acts
		const hasActivity = overlappingActs.length > 0;
		const activeDays = hasActivity ? Math.floor(Math.random() * daysInMonth * 0.7) : 0;
		return {
			month,
			activeDays,
			totalDays: daysInMonth,
			highlights: [],
		};
	});

	// Create empty stub page
	const yearPage: YearPage = {
		id: `year-${year}`,
		slug: String(year),
		type: "year",
		title: String(year),

		// Temporal
		year,
		period: {
			start: new Date(year, 0, 1),
			end: new Date(year, 11, 31),
		},

		// Content
		content: "",
		months,

		// Connections
		acts: overlappingActs.map((act) => ({
			displayName: act.title,
			pageSlug: act.slug,
			pageType: "act" as const,
		})),
		chapters: [],
		significantDays: [],
		keyPeople: [],
		keyPlaces: [],
		themes: [],

		// Standard fields
		citations: [],
		linkedPages: [],
		tags: [],
		createdAt: new Date(),
		updatedAt: new Date(),
		lastEditedBy: "ai",
	};

	// Cache it for subsequent requests
	MOCK_PAGES[slug] = yearPage;

	return yearPage;
}
