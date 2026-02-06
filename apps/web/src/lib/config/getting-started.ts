/**
 * Getting Started Configuration
 *
 * Defines the onboarding checklist steps shown to new users
 * in the empty chat state.
 */

export interface GettingStartedStep {
	id: string;
	title: string;
	description: string;
	icon: string;
	action: StepAction;
	autoComplete?: AutoCompleteCheck;
}

export type StepAction =
	| { type: 'createChat'; title: string; content: string }
	| { type: 'navigate'; href: string }
	| { type: 'focusInput'; placeholder?: string }
	| { type: 'openExternal'; url: string }
	| { type: 'suggestPrompt' };

export type AutoCompleteCheck =
	| { type: 'hasSourcesConnected' }
	| { type: 'hasDevicePaired' }
	| { type: 'hasChatSessions' }
	| { type: 'none' };

/**
 * Welcome message for the "Learn about Virtues" intro session.
 * Warm & philosophical tone (~250 words)
 */
export const INTRO_SESSION_TITLE = 'Welcome to Virtues';

export const INTRO_MESSAGE = `Welcome to Virtues.

In a world where your data is scattered across dozens of services—each extracting value from your attention and information—Virtues offers a different path. This is your personal AI, built on the principle that your data belongs to you, and that technology should serve your flourishing rather than exploit it.

**What makes Virtues different:**

Virtues operates on the principle of *subsidiarity*—decisions and data should be handled at the most personal level possible. Your conversations, health metrics, calendar, and finances stay under your control, never shared or productized.

**What I can help with:**

- **Remember and connect** — I maintain context about your life to surface patterns you might miss
- **Search your world** — Ask questions across your emails, messages, calendar, health data, and more
- **Support reflection** — Understand your habits, track your goals, and align your actions with your values
- **Take action** — Create tasks, manage your calendar, and organize your digital life

**Getting started:**

Connect your first data source—perhaps your Google account or iPhone—and simply ask me a question. Try something like "What did I do yesterday?" or "Show me my recent emails about the project."

I'm here to help you live your life, your story more purposefully. What would you like to explore first?

For a long time, the trade-off has been: "Give us your data, and we'll give you a free service." But that usually means your attention is the product being sold.

Virtues flips that script. I think the most exciting part is the shift from fragmentation to coherence.

Think about it:

Your Health data is in one app.
Your Work projects are in another.
Your Personal reflections are in a third.
Your Finances are in a fourth.
Usually, you are the only bridge between those worlds, and it's exhausting to keep track of it all. By bringing them together here—privately—we can start to see patterns. We can see how your sleep affects your productivity, or how your spending aligns (or doesn't) with what you say your goals are.

It’s not just about being a "smart assistant"; it’s about being a mirror that helps you see your life more clearly.
`;

/**
 * Getting Started steps configuration
 */
export const GETTING_STARTED_STEPS: GettingStartedStep[] = [
	{
		id: 'intro-chat',
		title: 'Getting started chat',
		description: 'Open a chat with prepopulated info',
		icon: 'ri:chat-1-line',
		action: {
			type: 'createChat',
			title: INTRO_SESSION_TITLE,
			content: INTRO_MESSAGE
		}
	},
	{
		id: 'connections',
		title: 'Add sources',
		description: 'Add iOS, Gmail, Mac, etc.',
		icon: 'ri:link',
		action: { type: 'navigate', href: '/sources' },
		autoComplete: { type: 'hasSourcesConnected' }
	},
	{
		id: 'personalization',
		title: 'Personalization',
		description: 'Customize theme and settings',
		icon: 'ri:user-settings-line',
		action: { type: 'navigate', href: '/virtues/account' },
		autoComplete: { type: 'none' }
	},
	{
		id: 'user-docs',
		title: 'User Docs',
		description: 'View documentation at virtues.com',
		icon: 'ri:book-open-line',
		action: { type: 'openExternal', url: 'https://virtues.com' }
	}
];
