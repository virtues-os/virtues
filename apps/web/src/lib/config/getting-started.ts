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
	| { type: 'createSession'; title: string; content: string }
	| { type: 'navigate'; href: string }
	| { type: 'focusInput'; placeholder?: string };

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

I'm here to help you live your story more purposefully. What would you like to explore first?`;

/**
 * Getting Started steps configuration
 */
export const GETTING_STARTED_STEPS: GettingStartedStep[] = [
	{
		id: 'learn',
		title: 'Learn about Virtues',
		description: 'Discover what your AI assistant can do',
		icon: 'ri:lightbulb-line',
		action: {
			type: 'createSession',
			title: INTRO_SESSION_TITLE,
			content: INTRO_MESSAGE
		}
	},
	{
		id: 'connect-source',
		title: 'Connect a data source',
		description: 'Link Google, Notion, or Plaid',
		icon: 'ri:link',
		action: { type: 'navigate', href: '/data/sources/add' },
		autoComplete: { type: 'hasSourcesConnected' }
	},
	{
		id: 'pair-device',
		title: 'Pair your iPhone',
		description: 'Sync health, location, and more',
		icon: 'ri:smartphone-line',
		action: { type: 'navigate', href: '/pair' },
		autoComplete: { type: 'hasDevicePaired' }
	},
	{
		id: 'first-question',
		title: 'Ask your first question',
		description: 'Try "What did I do yesterday?"',
		icon: 'ri:question-line',
		action: {
			type: 'focusInput',
			placeholder: 'What did I do yesterday?'
		},
		autoComplete: { type: 'hasChatSessions' }
	},
	{
		id: 'explore-data',
		title: 'Explore your data',
		description: "See what's connected and synced",
		icon: 'ri:database-2-line',
		action: { type: 'navigate', href: '/data/sources' }
	},
	{
		id: 'personalize',
		title: 'Personalize your assistant',
		description: 'Set name and preferences',
		icon: 'ri:user-settings-line',
		action: { type: 'navigate', href: '/profile/assistant' }
	}
];
