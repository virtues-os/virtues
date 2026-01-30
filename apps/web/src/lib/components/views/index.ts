/**
 * Content Views
 * 
 * Platform-agnostic content components that can be used in:
 * - Tab views (web)
 * - Modals
 * - Mobile WebViews (iOS, Android)
 * - Desktop apps (Electron, Tauri)
 * 
 * These are the source of truth for all content UI.
 * Tab views in /tabs/views/ are thin wrappers around these components.
 */

// Extracted content components
export { default as WikiContent } from './WikiContent.svelte';
export { default as PageContent } from './PageContent.svelte';

// Re-export existing tab views as content components for API consistency
// These are already the canonical implementations
export { default as ChatContent } from '../tabs/views/ChatView.svelte';
export { default as DataSourcesContent } from '../tabs/views/DataSourcesView.svelte';
export { default as DataSourceDetailContent } from '../tabs/views/DataSourceDetailView.svelte';
export { default as DriveContent } from '../tabs/views/DriveView.svelte';
export { default as ProfileContent } from '../tabs/views/ProfileView.svelte';
export { default as UsageContent } from '../tabs/views/UsageView.svelte';
export { default as HistoryContent } from '../tabs/views/HistoryView.svelte';
export { default as JobsContent } from '../tabs/views/JobsView.svelte';
