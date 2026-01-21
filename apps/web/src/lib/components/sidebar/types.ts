// Sidebar TypeScript interfaces

export type NavItemType = 'link' | 'action';

export interface SidebarNavItemData {
	id: string;
	type: NavItemType;
	label: string;
	icon?: string;
	href?: string;
	pagespace?: string; // For route matching
	onclick?: () => void;
	statusIcon?: string; // Completion checkmark for onboarding
	forceActive?: boolean; // Override active state
}

export interface SidebarSectionData {
	id: string;
	title: string;
	icon?: string;
	items: SidebarNavItemData[];
	defaultExpanded?: boolean;
	badge?: string; // e.g., "2/4" for onboarding progress
}
