/**
 * The shell's icons: Fluent (Microsoft's open set), Regular at rest and Filled
 * for the selected state (a selected row, the active tab, the sidebar toggle
 * while the sidebar is open). Fluent draws every icon as a Regular and Filled
 * pair, with round caps and generous corners, and draws it again for each
 * size on that size's pixel grid, so each size here uses its own drawing:
 * 20 in the sidebar and the window header, 16 in tabs. Bundled rather than
 * fetched, like the Remix set in icons.ts, so nothing shifts while one loads.
 *
 * Two maps lead in: the shell's own glyph names (what `AtlasIcon` is asked
 * for) and the Remix names tabs carry as their defaults. Anything outside
 * both, such as an icon or emoji the owner picked for a page, is left as it
 * is: that is their content.
 */
import { addIcon } from '@iconify/svelte';

import flAdd16Regular from '@iconify-icons/fluent/add-16-regular';
import flAdd16Filled from '@iconify-icons/fluent/add-16-filled';
import flAdd20Regular from '@iconify-icons/fluent/add-20-regular';
import flAdd20Filled from '@iconify-icons/fluent/add-20-filled';
import flApps16Regular from '@iconify-icons/fluent/apps-16-regular';
import flApps16Filled from '@iconify-icons/fluent/apps-16-filled';
import flApps20Regular from '@iconify-icons/fluent/apps-20-regular';
import flApps20Filled from '@iconify-icons/fluent/apps-20-filled';
import flArrowLeft16Regular from '@iconify-icons/fluent/arrow-left-16-regular';
import flArrowLeft16Filled from '@iconify-icons/fluent/arrow-left-16-filled';
import flArrowLeft20Regular from '@iconify-icons/fluent/arrow-left-20-regular';
import flArrowLeft20Filled from '@iconify-icons/fluent/arrow-left-20-filled';
import flArrowRight16Regular from '@iconify-icons/fluent/arrow-right-16-regular';
import flArrowRight16Filled from '@iconify-icons/fluent/arrow-right-16-filled';
import flArrowRight20Regular from '@iconify-icons/fluent/arrow-right-20-regular';
import flArrowRight20Filled from '@iconify-icons/fluent/arrow-right-20-filled';
import flBoard16Regular from '@iconify-icons/fluent/board-16-regular';
import flBoard16Filled from '@iconify-icons/fluent/board-16-filled';
import flBoard20Regular from '@iconify-icons/fluent/board-20-regular';
import flBoard20Filled from '@iconify-icons/fluent/board-20-filled';
import flBookOpen16Regular from '@iconify-icons/fluent/book-open-16-regular';
import flBookOpen16Filled from '@iconify-icons/fluent/book-open-16-filled';
import flBookOpen20Regular from '@iconify-icons/fluent/book-open-20-regular';
import flBookOpen20Filled from '@iconify-icons/fluent/book-open-20-filled';
import flBookmark16Regular from '@iconify-icons/fluent/bookmark-16-regular';
import flBookmark16Filled from '@iconify-icons/fluent/bookmark-16-filled';
import flBookmark20Regular from '@iconify-icons/fluent/bookmark-20-regular';
import flBookmark20Filled from '@iconify-icons/fluent/bookmark-20-filled';
import flBuilding16Regular from '@iconify-icons/fluent/building-16-regular';
import flBuilding16Filled from '@iconify-icons/fluent/building-16-filled';
import flBuilding20Regular from '@iconify-icons/fluent/building-20-regular';
import flBuilding20Filled from '@iconify-icons/fluent/building-20-filled';
import flBuildingShop16Regular from '@iconify-icons/fluent/building-shop-16-regular';
import flBuildingShop16Filled from '@iconify-icons/fluent/building-shop-16-filled';
import flBuildingShop20Regular from '@iconify-icons/fluent/building-shop-20-regular';
import flBuildingShop20Filled from '@iconify-icons/fluent/building-shop-20-filled';
import flCalendarLtr16Regular from '@iconify-icons/fluent/calendar-ltr-16-regular';
import flCalendarLtr16Filled from '@iconify-icons/fluent/calendar-ltr-16-filled';
import flCalendarLtr20Regular from '@iconify-icons/fluent/calendar-ltr-20-regular';
import flCalendarLtr20Filled from '@iconify-icons/fluent/calendar-ltr-20-filled';
import flCalendarMonth16Regular from '@iconify-icons/fluent/calendar-month-16-regular';
import flCalendarMonth16Filled from '@iconify-icons/fluent/calendar-month-16-filled';
import flCalendarMonth20Regular from '@iconify-icons/fluent/calendar-month-20-regular';
import flCalendarMonth20Filled from '@iconify-icons/fluent/calendar-month-20-filled';
import flChat16Regular from '@iconify-icons/fluent/chat-16-regular';
import flChat16Filled from '@iconify-icons/fluent/chat-16-filled';
import flChat20Regular from '@iconify-icons/fluent/chat-20-regular';
import flChat20Filled from '@iconify-icons/fluent/chat-20-filled';
import flCompose16Regular from '@iconify-icons/fluent/compose-16-regular';
import flCompose16Filled from '@iconify-icons/fluent/compose-16-filled';
import flCompose20Regular from '@iconify-icons/fluent/compose-20-regular';
import flCompose20Filled from '@iconify-icons/fluent/compose-20-filled';
import flContactCard16Regular from '@iconify-icons/fluent/contact-card-16-regular';
import flContactCard16Filled from '@iconify-icons/fluent/contact-card-16-filled';
import flContactCard20Regular from '@iconify-icons/fluent/contact-card-20-regular';
import flContactCard20Filled from '@iconify-icons/fluent/contact-card-20-filled';
import flDataPie16Regular from '@iconify-icons/fluent/data-pie-16-regular';
import flDataPie16Filled from '@iconify-icons/fluent/data-pie-16-filled';
import flDataPie20Regular from '@iconify-icons/fluent/data-pie-20-regular';
import flDataPie20Filled from '@iconify-icons/fluent/data-pie-20-filled';
import flDatabase16Regular from '@iconify-icons/fluent/database-16-regular';
import flDatabase16Filled from '@iconify-icons/fluent/database-16-filled';
import flDatabase20Regular from '@iconify-icons/fluent/database-20-regular';
import flDatabase20Filled from '@iconify-icons/fluent/database-20-filled';
import flDelete16Regular from '@iconify-icons/fluent/delete-16-regular';
import flDelete16Filled from '@iconify-icons/fluent/delete-16-filled';
import flDelete20Regular from '@iconify-icons/fluent/delete-20-regular';
import flDelete20Filled from '@iconify-icons/fluent/delete-20-filled';
import flDesktop16Regular from '@iconify-icons/fluent/desktop-16-regular';
import flDesktop16Filled from '@iconify-icons/fluent/desktop-16-filled';
import flDesktop20Regular from '@iconify-icons/fluent/desktop-20-regular';
import flDesktop20Filled from '@iconify-icons/fluent/desktop-20-filled';
import flDismiss16Regular from '@iconify-icons/fluent/dismiss-16-regular';
import flDismiss16Filled from '@iconify-icons/fluent/dismiss-16-filled';
import flDismiss20Regular from '@iconify-icons/fluent/dismiss-20-regular';
import flDismiss20Filled from '@iconify-icons/fluent/dismiss-20-filled';
import flDocument16Regular from '@iconify-icons/fluent/document-16-regular';
import flDocument16Filled from '@iconify-icons/fluent/document-16-filled';
import flDocument20Regular from '@iconify-icons/fluent/document-20-regular';
import flDocument20Filled from '@iconify-icons/fluent/document-20-filled';
import flDocumentText16Regular from '@iconify-icons/fluent/document-text-16-regular';
import flDocumentText16Filled from '@iconify-icons/fluent/document-text-16-filled';
import flDocumentText20Regular from '@iconify-icons/fluent/document-text-20-regular';
import flDocumentText20Filled from '@iconify-icons/fluent/document-text-20-filled';
import flFlag16Regular from '@iconify-icons/fluent/flag-16-regular';
import flFlag16Filled from '@iconify-icons/fluent/flag-16-filled';
import flFlag20Regular from '@iconify-icons/fluent/flag-20-regular';
import flFlag20Filled from '@iconify-icons/fluent/flag-20-filled';
import flFolder16Regular from '@iconify-icons/fluent/folder-16-regular';
import flFolder16Filled from '@iconify-icons/fluent/folder-16-filled';
import flFolder20Regular from '@iconify-icons/fluent/folder-20-regular';
import flFolder20Filled from '@iconify-icons/fluent/folder-20-filled';
import flFolderOpen16Regular from '@iconify-icons/fluent/folder-open-16-regular';
import flFolderOpen16Filled from '@iconify-icons/fluent/folder-open-16-filled';
import flFolderOpen20Regular from '@iconify-icons/fluent/folder-open-20-regular';
import flFolderOpen20Filled from '@iconify-icons/fluent/folder-open-20-filled';
import flGlobe16Regular from '@iconify-icons/fluent/globe-16-regular';
import flGlobe16Filled from '@iconify-icons/fluent/globe-16-filled';
import flGlobe20Regular from '@iconify-icons/fluent/globe-20-regular';
import flGlobe20Filled from '@iconify-icons/fluent/globe-20-filled';
import flHardDrive16Regular from '@iconify-icons/fluent/hard-drive-16-regular';
import flHardDrive16Filled from '@iconify-icons/fluent/hard-drive-16-filled';
import flHardDrive20Regular from '@iconify-icons/fluent/hard-drive-20-regular';
import flHardDrive20Filled from '@iconify-icons/fluent/hard-drive-20-filled';
import flHistory16Regular from '@iconify-icons/fluent/history-16-regular';
import flHistory16Filled from '@iconify-icons/fluent/history-16-filled';
import flHistory20Regular from '@iconify-icons/fluent/history-20-regular';
import flHistory20Filled from '@iconify-icons/fluent/history-20-filled';
import flHome16Regular from '@iconify-icons/fluent/home-16-regular';
import flHome16Filled from '@iconify-icons/fluent/home-16-filled';
import flHome20Regular from '@iconify-icons/fluent/home-20-regular';
import flHome20Filled from '@iconify-icons/fluent/home-20-filled';
import flImage16Regular from '@iconify-icons/fluent/image-16-regular';
import flImage16Filled from '@iconify-icons/fluent/image-16-filled';
import flImage20Regular from '@iconify-icons/fluent/image-20-regular';
import flImage20Filled from '@iconify-icons/fluent/image-20-filled';
import flImageMultiple16Regular from '@iconify-icons/fluent/image-multiple-16-regular';
import flImageMultiple16Filled from '@iconify-icons/fluent/image-multiple-16-filled';
import flImageMultiple20Regular from '@iconify-icons/fluent/image-multiple-20-regular';
import flImageMultiple20Filled from '@iconify-icons/fluent/image-multiple-20-filled';
import flLocation16Regular from '@iconify-icons/fluent/location-16-regular';
import flLocation16Filled from '@iconify-icons/fluent/location-16-filled';
import flLocation20Regular from '@iconify-icons/fluent/location-20-regular';
import flLocation20Filled from '@iconify-icons/fluent/location-20-filled';
import flMoreVertical16Regular from '@iconify-icons/fluent/more-vertical-16-regular';
import flMoreVertical16Filled from '@iconify-icons/fluent/more-vertical-16-filled';
import flMoreVertical20Regular from '@iconify-icons/fluent/more-vertical-20-regular';
import flMoreVertical20Filled from '@iconify-icons/fluent/more-vertical-20-filled';
import flPanelLeft16Regular from '@iconify-icons/fluent/panel-left-16-regular';
import flPanelLeft16Filled from '@iconify-icons/fluent/panel-left-16-filled';
import flPanelLeft20Regular from '@iconify-icons/fluent/panel-left-20-regular';
import flPanelLeft20Filled from '@iconify-icons/fluent/panel-left-20-filled';
import flPayment16Regular from '@iconify-icons/fluent/payment-16-regular';
import flPayment16Filled from '@iconify-icons/fluent/payment-16-filled';
import flPayment20Regular from '@iconify-icons/fluent/payment-20-regular';
import flPayment20Filled from '@iconify-icons/fluent/payment-20-filled';
import flPen16Regular from '@iconify-icons/fluent/pen-16-regular';
import flPen16Filled from '@iconify-icons/fluent/pen-16-filled';
import flPen20Regular from '@iconify-icons/fluent/pen-20-regular';
import flPen20Filled from '@iconify-icons/fluent/pen-20-filled';
import flPeople16Regular from '@iconify-icons/fluent/people-16-regular';
import flPeople16Filled from '@iconify-icons/fluent/people-16-filled';
import flPeople20Regular from '@iconify-icons/fluent/people-20-regular';
import flPeople20Filled from '@iconify-icons/fluent/people-20-filled';
import flPerson16Regular from '@iconify-icons/fluent/person-16-regular';
import flPerson16Filled from '@iconify-icons/fluent/person-16-filled';
import flPerson20Regular from '@iconify-icons/fluent/person-20-regular';
import flPerson20Filled from '@iconify-icons/fluent/person-20-filled';
import flPhone16Regular from '@iconify-icons/fluent/phone-16-regular';
import flPhone16Filled from '@iconify-icons/fluent/phone-16-filled';
import flPhone20Regular from '@iconify-icons/fluent/phone-20-regular';
import flPhone20Filled from '@iconify-icons/fluent/phone-20-filled';
import flPhoneLaptop16Regular from '@iconify-icons/fluent/phone-laptop-16-regular';
import flPhoneLaptop16Filled from '@iconify-icons/fluent/phone-laptop-16-filled';
import flPhoneLaptop20Regular from '@iconify-icons/fluent/phone-laptop-20-regular';
import flPhoneLaptop20Filled from '@iconify-icons/fluent/phone-laptop-20-filled';
import flPulse20Regular from '@iconify-icons/fluent/pulse-20-regular';
import flPulse20Filled from '@iconify-icons/fluent/pulse-20-filled';
import flSearch16Regular from '@iconify-icons/fluent/search-16-regular';
import flSearch16Filled from '@iconify-icons/fluent/search-16-filled';
import flSearch20Regular from '@iconify-icons/fluent/search-20-regular';
import flSearch20Filled from '@iconify-icons/fluent/search-20-filled';
import flServer16Regular from '@iconify-icons/fluent/server-16-regular';
import flServer16Filled from '@iconify-icons/fluent/server-16-filled';
import flServer20Regular from '@iconify-icons/fluent/server-20-regular';
import flServer20Filled from '@iconify-icons/fluent/server-20-filled';
import flSettings16Regular from '@iconify-icons/fluent/settings-16-regular';
import flSettings16Filled from '@iconify-icons/fluent/settings-16-filled';
import flSettings20Regular from '@iconify-icons/fluent/settings-20-regular';
import flSettings20Filled from '@iconify-icons/fluent/settings-20-filled';
import flShare16Regular from '@iconify-icons/fluent/share-16-regular';
import flShare16Filled from '@iconify-icons/fluent/share-16-filled';
import flShare20Regular from '@iconify-icons/fluent/share-20-regular';
import flShare20Filled from '@iconify-icons/fluent/share-20-filled';
import flSparkle16Regular from '@iconify-icons/fluent/sparkle-16-regular';
import flSparkle16Filled from '@iconify-icons/fluent/sparkle-16-filled';
import flSparkle20Regular from '@iconify-icons/fluent/sparkle-20-regular';
import flSparkle20Filled from '@iconify-icons/fluent/sparkle-20-filled';
import flTable16Regular from '@iconify-icons/fluent/table-16-regular';
import flTable16Filled from '@iconify-icons/fluent/table-16-filled';
import flTable20Regular from '@iconify-icons/fluent/table-20-regular';
import flTable20Filled from '@iconify-icons/fluent/table-20-filled';
import flTaskListLtr20Regular from '@iconify-icons/fluent/task-list-ltr-20-regular';
import flTaskListLtr20Filled from '@iconify-icons/fluent/task-list-ltr-20-filled';
import flTextBulletListLtr16Regular from '@iconify-icons/fluent/text-bullet-list-ltr-16-regular';
import flTextBulletListLtr16Filled from '@iconify-icons/fluent/text-bullet-list-ltr-16-filled';
import flTextBulletListLtr20Regular from '@iconify-icons/fluent/text-bullet-list-ltr-20-regular';
import flTextBulletListLtr20Filled from '@iconify-icons/fluent/text-bullet-list-ltr-20-filled';
import flTimeline20Regular from '@iconify-icons/fluent/timeline-20-regular';
import flTimeline20Filled from '@iconify-icons/fluent/timeline-20-filled';
import flWeatherSunny16Regular from '@iconify-icons/fluent/weather-sunny-16-regular';
import flWeatherSunny16Filled from '@iconify-icons/fluent/weather-sunny-16-filled';
import flWeatherSunny20Regular from '@iconify-icons/fluent/weather-sunny-20-regular';
import flWeatherSunny20Filled from '@iconify-icons/fluent/weather-sunny-20-filled';
import flWindowConsole20Regular from '@iconify-icons/fluent/window-console-20-regular';
import flWindowConsole20Filled from '@iconify-icons/fluent/window-console-20-filled';

const ICONS = {
	'add': { 16: [flAdd16Regular, flAdd16Filled], 20: [flAdd20Regular, flAdd20Filled] },
	'apps': { 16: [flApps16Regular, flApps16Filled], 20: [flApps20Regular, flApps20Filled] },
	'arrow-left': { 16: [flArrowLeft16Regular, flArrowLeft16Filled], 20: [flArrowLeft20Regular, flArrowLeft20Filled] },
	'arrow-right': { 16: [flArrowRight16Regular, flArrowRight16Filled], 20: [flArrowRight20Regular, flArrowRight20Filled] },
	'board': { 16: [flBoard16Regular, flBoard16Filled], 20: [flBoard20Regular, flBoard20Filled] },
	'book-open': { 16: [flBookOpen16Regular, flBookOpen16Filled], 20: [flBookOpen20Regular, flBookOpen20Filled] },
	'bookmark': { 16: [flBookmark16Regular, flBookmark16Filled], 20: [flBookmark20Regular, flBookmark20Filled] },
	'building': { 16: [flBuilding16Regular, flBuilding16Filled], 20: [flBuilding20Regular, flBuilding20Filled] },
	'building-shop': { 16: [flBuildingShop16Regular, flBuildingShop16Filled], 20: [flBuildingShop20Regular, flBuildingShop20Filled] },
	'calendar-ltr': { 16: [flCalendarLtr16Regular, flCalendarLtr16Filled], 20: [flCalendarLtr20Regular, flCalendarLtr20Filled] },
	'calendar-month': { 16: [flCalendarMonth16Regular, flCalendarMonth16Filled], 20: [flCalendarMonth20Regular, flCalendarMonth20Filled] },
	'chat': { 16: [flChat16Regular, flChat16Filled], 20: [flChat20Regular, flChat20Filled] },
	'compose': { 16: [flCompose16Regular, flCompose16Filled], 20: [flCompose20Regular, flCompose20Filled] },
	'contact-card': { 16: [flContactCard16Regular, flContactCard16Filled], 20: [flContactCard20Regular, flContactCard20Filled] },
	'data-pie': { 16: [flDataPie16Regular, flDataPie16Filled], 20: [flDataPie20Regular, flDataPie20Filled] },
	'database': { 16: [flDatabase16Regular, flDatabase16Filled], 20: [flDatabase20Regular, flDatabase20Filled] },
	'delete': { 16: [flDelete16Regular, flDelete16Filled], 20: [flDelete20Regular, flDelete20Filled] },
	'desktop': { 16: [flDesktop16Regular, flDesktop16Filled], 20: [flDesktop20Regular, flDesktop20Filled] },
	'dismiss': { 16: [flDismiss16Regular, flDismiss16Filled], 20: [flDismiss20Regular, flDismiss20Filled] },
	'document': { 16: [flDocument16Regular, flDocument16Filled], 20: [flDocument20Regular, flDocument20Filled] },
	'document-text': { 16: [flDocumentText16Regular, flDocumentText16Filled], 20: [flDocumentText20Regular, flDocumentText20Filled] },
	'flag': { 16: [flFlag16Regular, flFlag16Filled], 20: [flFlag20Regular, flFlag20Filled] },
	'folder': { 16: [flFolder16Regular, flFolder16Filled], 20: [flFolder20Regular, flFolder20Filled] },
	'folder-open': { 16: [flFolderOpen16Regular, flFolderOpen16Filled], 20: [flFolderOpen20Regular, flFolderOpen20Filled] },
	'globe': { 16: [flGlobe16Regular, flGlobe16Filled], 20: [flGlobe20Regular, flGlobe20Filled] },
	'hard-drive': { 16: [flHardDrive16Regular, flHardDrive16Filled], 20: [flHardDrive20Regular, flHardDrive20Filled] },
	'history': { 16: [flHistory16Regular, flHistory16Filled], 20: [flHistory20Regular, flHistory20Filled] },
	'home': { 16: [flHome16Regular, flHome16Filled], 20: [flHome20Regular, flHome20Filled] },
	'image': { 16: [flImage16Regular, flImage16Filled], 20: [flImage20Regular, flImage20Filled] },
	'image-multiple': { 16: [flImageMultiple16Regular, flImageMultiple16Filled], 20: [flImageMultiple20Regular, flImageMultiple20Filled] },
	'location': { 16: [flLocation16Regular, flLocation16Filled], 20: [flLocation20Regular, flLocation20Filled] },
	'more-vertical': { 16: [flMoreVertical16Regular, flMoreVertical16Filled], 20: [flMoreVertical20Regular, flMoreVertical20Filled] },
	'panel-left': { 16: [flPanelLeft16Regular, flPanelLeft16Filled], 20: [flPanelLeft20Regular, flPanelLeft20Filled] },
	'payment': { 16: [flPayment16Regular, flPayment16Filled], 20: [flPayment20Regular, flPayment20Filled] },
	'pen': { 16: [flPen16Regular, flPen16Filled], 20: [flPen20Regular, flPen20Filled] },
	'people': { 16: [flPeople16Regular, flPeople16Filled], 20: [flPeople20Regular, flPeople20Filled] },
	'person': { 16: [flPerson16Regular, flPerson16Filled], 20: [flPerson20Regular, flPerson20Filled] },
	'phone': { 16: [flPhone16Regular, flPhone16Filled], 20: [flPhone20Regular, flPhone20Filled] },
	'phone-laptop': { 16: [flPhoneLaptop16Regular, flPhoneLaptop16Filled], 20: [flPhoneLaptop20Regular, flPhoneLaptop20Filled] },
	'pulse': { 20: [flPulse20Regular, flPulse20Filled] },
	'search': { 16: [flSearch16Regular, flSearch16Filled], 20: [flSearch20Regular, flSearch20Filled] },
	'server': { 16: [flServer16Regular, flServer16Filled], 20: [flServer20Regular, flServer20Filled] },
	'settings': { 16: [flSettings16Regular, flSettings16Filled], 20: [flSettings20Regular, flSettings20Filled] },
	'share': { 16: [flShare16Regular, flShare16Filled], 20: [flShare20Regular, flShare20Filled] },
	'sparkle': { 16: [flSparkle16Regular, flSparkle16Filled], 20: [flSparkle20Regular, flSparkle20Filled] },
	'table': { 16: [flTable16Regular, flTable16Filled], 20: [flTable20Regular, flTable20Filled] },
	'task-list-ltr': { 20: [flTaskListLtr20Regular, flTaskListLtr20Filled] },
	'text-bullet-list-ltr': { 16: [flTextBulletListLtr16Regular, flTextBulletListLtr16Filled], 20: [flTextBulletListLtr20Regular, flTextBulletListLtr20Filled] },
	'timeline': { 20: [flTimeline20Regular, flTimeline20Filled] },
	'weather-sunny': { 16: [flWeatherSunny16Regular, flWeatherSunny16Filled], 20: [flWeatherSunny20Regular, flWeatherSunny20Filled] },
	'window-console': { 20: [flWindowConsole20Regular, flWindowConsole20Filled] },
} as const;

type Name = keyof typeof ICONS;

for (const [name, bySize] of Object.entries(ICONS)) {
	for (const [size, [regular, filled]] of Object.entries(bySize)) {
		addIcon(`fluent:${name}-${size}-regular`, regular);
		addIcon(`fluent:${name}-${size}-filled`, filled);
	}
}

/**
 * The size of a glyph in the sidebar's rows and the window header: Fluent's
 * 20 drawing at its own size, about 1.25px of line, near the weight of the
 * 13px labels beside it.
 */
export const SHELL_GLYPH_PX = 20;

/** The shell's glyph names, as `AtlasIcon` and the sidebar's rows ask for them. */
const SHELL: Record<string, Name> = {
	'chats': 'chat',
	'pages': 'document-text',
	'projects': 'folder',
	'bookmarks': 'bookmark',
	'calendar': 'calendar-ltr',
	'wiki': 'globe',
	'drive': 'hard-drive',
	'applets': 'apps',
	'tasks': 'task-list-ltr',
	'dashboard': 'board',
	'search': 'search',
	'new-chat': 'compose',
	'device': 'phone',
	'sources': 'database',
	'timeline': 'timeline',
	'setup': 'flag',
	'home': 'home',
	'settings': 'settings',
	'back': 'arrow-left',
	'forward': 'arrow-right',
	'sidebar': 'panel-left',
	'plus': 'add',
	'close': 'dismiss',
	'profile': 'contact-card',
	'assistant': 'sparkle',
	'billing': 'payment',
	'system': 'server',
	'devices': 'phone-laptop',
	'display': 'desktop',
	'shared': 'share',
	'sql': 'table',
	'terminal': 'window-console',
	'overview': 'data-pie',
	'catalog': 'building-shop',
	'files': 'folder-open',
	'streams': 'pulse',
	'media': 'image-multiple',
	'trash': 'delete',
	'day': 'weather-sunny',
	'years': 'calendar-month',
	'chapters': 'text-bullet-list-ltr',
	'identity': 'person',
	'people': 'people',
	'places': 'location',
	'organizations': 'building',
	'stories': 'book-open',
	'history': 'history',
};

/** The Remix icons tabs and the chat carry as defaults. */
const REMIX: Record<string, Name> = {
	'ri:chat-1-line': 'chat',
	'ri:chat-history-line': 'history',
	'ri:book-2-line': 'book-open',
	'ri:bookmark-line': 'bookmark',
	'ri:building-line': 'building',
	'ri:calendar-line': 'calendar-ltr',
	'ri:database-2-line': 'database',
	'ri:delete-bin-line': 'delete',
	'ri:file-line': 'document',
	'ri:file-list-3-line': 'document-text',
	'ri:file-text-line': 'document-text',
	'ri:global-line': 'globe',
	'ri:hard-drive-2-line': 'hard-drive',
	'ri:image-2-line': 'image',
	'ri:layout-2-line': 'board',
	'ri:list-check': 'text-bullet-list-ltr',
	'ri:map-pin-line': 'location',
	'ri:quill-pen-line': 'pen',
	'ri:route-line': 'timeline',
	'ri:settings-4-line': 'settings',
	'ri:user-line': 'person',
	'ri:more-2-fill': 'more-vertical',
};

/** The drawing closest to a rendered size: 16 for small, 20 otherwise. */
function drawn(name: Name, size: number, filled: boolean): string {
	const bySize = ICONS[name] as Partial<Record<16 | 20, unknown>>;
	const want = size <= 17 ? 16 : 20;
	const sz = bySize[want] ? want : want === 16 ? 20 : 16;
	return `fluent:${name}-${sz}-${filled ? 'filled' : 'regular'}`;
}

/** The icon for a shell glyph name at a size, or null when there is none. */
export function shellIcon(name: string, filled = false, size: number = SHELL_GLYPH_PX): string | null {
	const icon = SHELL[name];
	return icon ? drawn(icon, size, filled) : null;
}

/**
 * The icon for an icon string a tab or menu carries (`atlas:<name>` or a
 * known Remix default), at tab size, or the string unchanged when it is the
 * owner's own pick.
 */
export function shellIconFor(icon: string, filled = false): string {
	const name = icon.startsWith('atlas:') ? SHELL[icon.slice('atlas:'.length)] : REMIX[icon];
	return name ? drawn(name, 16, filled) : icon;
}
