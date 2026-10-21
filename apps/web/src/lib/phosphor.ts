/**
 * The shell's icons: Phosphor, Regular at rest and Fill for the selected
 * state (a selected row, the active tab), the way a native Mac sidebar marks
 * where you are. Bundled here rather than fetched, like the Remix set in
 * icons.ts, so nothing shifts while an icon loads.
 *
 * Two maps lead in: the shell's own glyph names (what `AtlasIcon` is asked
 * for) and the Remix names tabs carry as their defaults. Anything outside
 * both, such as an icon or emoji the owner picked for a page, is left as it
 * is: that is their content.
 */
import { addIcon } from '@iconify/svelte';

import phArrowLeft from '@iconify-icons/ph/arrow-left';
import phArrowLeftFill from '@iconify-icons/ph/arrow-left-fill';
import phArrowRight from '@iconify-icons/ph/arrow-right';
import phArrowRightFill from '@iconify-icons/ph/arrow-right-fill';
import phBookOpen from '@iconify-icons/ph/book-open';
import phBookOpenFill from '@iconify-icons/ph/book-open-fill';
import phBookmarkSimple from '@iconify-icons/ph/bookmark-simple';
import phBookmarkSimpleFill from '@iconify-icons/ph/bookmark-simple-fill';
import phBuildings from '@iconify-icons/ph/buildings';
import phBuildingsFill from '@iconify-icons/ph/buildings-fill';
import phCalendar from '@iconify-icons/ph/calendar';
import phCalendarFill from '@iconify-icons/ph/calendar-fill';
import phCalendarBlank from '@iconify-icons/ph/calendar-blank';
import phCalendarBlankFill from '@iconify-icons/ph/calendar-blank-fill';
import phChartPieSlice from '@iconify-icons/ph/chart-pie-slice';
import phChartPieSliceFill from '@iconify-icons/ph/chart-pie-slice-fill';
import phChatCircle from '@iconify-icons/ph/chat-circle';
import phChatCircleFill from '@iconify-icons/ph/chat-circle-fill';
import phCheckSquare from '@iconify-icons/ph/check-square';
import phCheckSquareFill from '@iconify-icons/ph/check-square-fill';
import phClockCounterClockwise from '@iconify-icons/ph/clock-counter-clockwise';
import phClockCounterClockwiseFill from '@iconify-icons/ph/clock-counter-clockwise-fill';
import phCpu from '@iconify-icons/ph/cpu';
import phCpuFill from '@iconify-icons/ph/cpu-fill';
import phCreditCard from '@iconify-icons/ph/credit-card';
import phCreditCardFill from '@iconify-icons/ph/credit-card-fill';
import phDatabase from '@iconify-icons/ph/database';
import phDatabaseFill from '@iconify-icons/ph/database-fill';
import phDeviceMobile from '@iconify-icons/ph/device-mobile';
import phDeviceMobileFill from '@iconify-icons/ph/device-mobile-fill';
import phDevices from '@iconify-icons/ph/devices';
import phDevicesFill from '@iconify-icons/ph/devices-fill';
import phDotsThreeVertical from '@iconify-icons/ph/dots-three-vertical';
import phDotsThreeVerticalFill from '@iconify-icons/ph/dots-three-vertical-fill';
import phFile from '@iconify-icons/ph/file';
import phFileFill from '@iconify-icons/ph/file-fill';
import phFileText from '@iconify-icons/ph/file-text';
import phFileTextFill from '@iconify-icons/ph/file-text-fill';
import phFlag from '@iconify-icons/ph/flag';
import phFlagFill from '@iconify-icons/ph/flag-fill';
import phFolder from '@iconify-icons/ph/folder';
import phFolderFill from '@iconify-icons/ph/folder-fill';
import phFolderSimple from '@iconify-icons/ph/folder-simple';
import phFolderSimpleFill from '@iconify-icons/ph/folder-simple-fill';
import phGearSix from '@iconify-icons/ph/gear-six';
import phGearSixFill from '@iconify-icons/ph/gear-six-fill';
import phGhost from '@iconify-icons/ph/ghost';
import phGhostFill from '@iconify-icons/ph/ghost-fill';
import phGlobeSimple from '@iconify-icons/ph/globe-simple';
import phGlobeSimpleFill from '@iconify-icons/ph/globe-simple-fill';
import phHardDrives from '@iconify-icons/ph/hard-drives';
import phHardDrivesFill from '@iconify-icons/ph/hard-drives-fill';
import phHouse from '@iconify-icons/ph/house';
import phHouseFill from '@iconify-icons/ph/house-fill';
import phIdentificationCard from '@iconify-icons/ph/identification-card';
import phIdentificationCardFill from '@iconify-icons/ph/identification-card-fill';
import phImages from '@iconify-icons/ph/images';
import phImagesFill from '@iconify-icons/ph/images-fill';
import phLayout from '@iconify-icons/ph/layout';
import phLayoutFill from '@iconify-icons/ph/layout-fill';
import phListBullets from '@iconify-icons/ph/list-bullets';
import phListBulletsFill from '@iconify-icons/ph/list-bullets-fill';
import phMagnifyingGlass from '@iconify-icons/ph/magnifying-glass';
import phMagnifyingGlassFill from '@iconify-icons/ph/magnifying-glass-fill';
import phMapPin from '@iconify-icons/ph/map-pin';
import phMapPinFill from '@iconify-icons/ph/map-pin-fill';
import phMonitor from '@iconify-icons/ph/monitor';
import phMonitorFill from '@iconify-icons/ph/monitor-fill';
import phNotePencil from '@iconify-icons/ph/note-pencil';
import phNotePencilFill from '@iconify-icons/ph/note-pencil-fill';
import phPath from '@iconify-icons/ph/path';
import phPathFill from '@iconify-icons/ph/path-fill';
import phPenNib from '@iconify-icons/ph/pen-nib';
import phPenNibFill from '@iconify-icons/ph/pen-nib-fill';
import phPlus from '@iconify-icons/ph/plus';
import phPlusFill from '@iconify-icons/ph/plus-fill';
import phShareNetwork from '@iconify-icons/ph/share-network';
import phShareNetworkFill from '@iconify-icons/ph/share-network-fill';
import phSidebarSimple from '@iconify-icons/ph/sidebar-simple';
import phSidebarSimpleFill from '@iconify-icons/ph/sidebar-simple-fill';
import phSparkle from '@iconify-icons/ph/sparkle';
import phSparkleFill from '@iconify-icons/ph/sparkle-fill';
import phSquaresFour from '@iconify-icons/ph/squares-four';
import phSquaresFourFill from '@iconify-icons/ph/squares-four-fill';
import phStorefront from '@iconify-icons/ph/storefront';
import phStorefrontFill from '@iconify-icons/ph/storefront-fill';
import phSunHorizon from '@iconify-icons/ph/sun-horizon';
import phSunHorizonFill from '@iconify-icons/ph/sun-horizon-fill';
import phTable from '@iconify-icons/ph/table';
import phTableFill from '@iconify-icons/ph/table-fill';
import phTerminalWindow from '@iconify-icons/ph/terminal-window';
import phTerminalWindowFill from '@iconify-icons/ph/terminal-window-fill';
import phTrash from '@iconify-icons/ph/trash';
import phTrashFill from '@iconify-icons/ph/trash-fill';
import phUser from '@iconify-icons/ph/user';
import phUserFill from '@iconify-icons/ph/user-fill';
import phUserCircle from '@iconify-icons/ph/user-circle';
import phUserCircleFill from '@iconify-icons/ph/user-circle-fill';
import phUsers from '@iconify-icons/ph/users';
import phUsersFill from '@iconify-icons/ph/users-fill';
import phWaveSine from '@iconify-icons/ph/wave-sine';
import phWaveSineFill from '@iconify-icons/ph/wave-sine-fill';
import phX from '@iconify-icons/ph/x';
import phXFill from '@iconify-icons/ph/x-fill';

const ICONS = {
	'arrow-left': [phArrowLeft, phArrowLeftFill],
	'arrow-right': [phArrowRight, phArrowRightFill],
	'book-open': [phBookOpen, phBookOpenFill],
	'bookmark-simple': [phBookmarkSimple, phBookmarkSimpleFill],
	'buildings': [phBuildings, phBuildingsFill],
	'calendar': [phCalendar, phCalendarFill],
	'calendar-blank': [phCalendarBlank, phCalendarBlankFill],
	'chart-pie-slice': [phChartPieSlice, phChartPieSliceFill],
	'chat-circle': [phChatCircle, phChatCircleFill],
	'check-square': [phCheckSquare, phCheckSquareFill],
	'clock-counter-clockwise': [phClockCounterClockwise, phClockCounterClockwiseFill],
	'cpu': [phCpu, phCpuFill],
	'credit-card': [phCreditCard, phCreditCardFill],
	'database': [phDatabase, phDatabaseFill],
	'device-mobile': [phDeviceMobile, phDeviceMobileFill],
	'devices': [phDevices, phDevicesFill],
	'dots-three-vertical': [phDotsThreeVertical, phDotsThreeVerticalFill],
	'file': [phFile, phFileFill],
	'file-text': [phFileText, phFileTextFill],
	'flag': [phFlag, phFlagFill],
	'folder': [phFolder, phFolderFill],
	'folder-simple': [phFolderSimple, phFolderSimpleFill],
	'gear-six': [phGearSix, phGearSixFill],
	'ghost': [phGhost, phGhostFill],
	'globe-simple': [phGlobeSimple, phGlobeSimpleFill],
	'hard-drives': [phHardDrives, phHardDrivesFill],
	'house': [phHouse, phHouseFill],
	'identification-card': [phIdentificationCard, phIdentificationCardFill],
	'images': [phImages, phImagesFill],
	'layout': [phLayout, phLayoutFill],
	'list-bullets': [phListBullets, phListBulletsFill],
	'magnifying-glass': [phMagnifyingGlass, phMagnifyingGlassFill],
	'map-pin': [phMapPin, phMapPinFill],
	'monitor': [phMonitor, phMonitorFill],
	'note-pencil': [phNotePencil, phNotePencilFill],
	'path': [phPath, phPathFill],
	'pen-nib': [phPenNib, phPenNibFill],
	'plus': [phPlus, phPlusFill],
	'share-network': [phShareNetwork, phShareNetworkFill],
	'sidebar-simple': [phSidebarSimple, phSidebarSimpleFill],
	'sparkle': [phSparkle, phSparkleFill],
	'squares-four': [phSquaresFour, phSquaresFourFill],
	'storefront': [phStorefront, phStorefrontFill],
	'sun-horizon': [phSunHorizon, phSunHorizonFill],
	'table': [phTable, phTableFill],
	'terminal-window': [phTerminalWindow, phTerminalWindowFill],
	'trash': [phTrash, phTrashFill],
	'user': [phUser, phUserFill],
	'user-circle': [phUserCircle, phUserCircleFill],
	'users': [phUsers, phUsersFill],
	'wave-sine': [phWaveSine, phWaveSineFill],
	'x': [phX, phXFill],
} as const;

for (const [name, [regular, fill]] of Object.entries(ICONS)) {
	addIcon(`ph:${name}`, regular);
	addIcon(`ph:${name}-fill`, fill);
}

/** The shell's glyph names, as `AtlasIcon` and the sidebar's rows ask for them. */
const SHELL: Record<string, keyof typeof ICONS> = {
	'chats': 'chat-circle',
	'pages': 'file-text',
	'projects': 'folder-simple',
	'bookmarks': 'bookmark-simple',
	'calendar': 'calendar-blank',
	'wiki': 'globe-simple',
	'drive': 'hard-drives',
	'applets': 'squares-four',
	'tasks': 'check-square',
	'dashboard': 'layout',
	'search': 'magnifying-glass',
	'new-chat': 'note-pencil',
	'device': 'device-mobile',
	'sources': 'database',
	'timeline': 'path',
	'setup': 'flag',
	'home': 'house',
	'settings': 'gear-six',
	'back': 'arrow-left',
	'forward': 'arrow-right',
	'sidebar': 'sidebar-simple',
	'plus': 'plus',
	'close': 'x',
	'profile': 'identification-card',
	'assistant': 'sparkle',
	'billing': 'credit-card',
	'system': 'cpu',
	'devices': 'devices',
	'display': 'monitor',
	'shared': 'share-network',
	'sql': 'table',
	'terminal': 'terminal-window',
	'overview': 'chart-pie-slice',
	'catalog': 'storefront',
	'files': 'folder',
	'streams': 'wave-sine',
	'media': 'images',
	'trash': 'trash',
	'day': 'sun-horizon',
	'years': 'calendar',
	'chapters': 'list-bullets',
	'identity': 'user-circle',
	'people': 'users',
	'places': 'map-pin',
	'organizations': 'buildings',
	'stories': 'book-open',
	'history': 'clock-counter-clockwise',
};

/** The Remix icons tabs and the chat carry as defaults. */
const REMIX: Record<string, keyof typeof ICONS> = {
	'ri:chat-1-line': 'chat-circle',
	'ri:chat-history-line': 'clock-counter-clockwise',
	'ri:book-2-line': 'book-open',
	'ri:bookmark-line': 'bookmark-simple',
	'ri:building-line': 'buildings',
	'ri:calendar-line': 'calendar-blank',
	'ri:database-2-line': 'database',
	'ri:delete-bin-line': 'trash',
	'ri:file-line': 'file',
	'ri:file-list-3-line': 'file-text',
	'ri:file-text-line': 'file-text',
	'ri:global-line': 'globe-simple',
	'ri:hard-drive-2-line': 'hard-drives',
	'ri:image-2-line': 'images',
	'ri:layout-2-line': 'layout',
	'ri:list-check': 'list-bullets',
	'ri:map-pin-line': 'map-pin',
	'ri:quill-pen-line': 'pen-nib',
	'ri:route-line': 'path',
	'ri:settings-4-line': 'gear-six',
	'ri:user-line': 'user',
	'ri:ghost-line': 'ghost',
	'ri:more-2-fill': 'dots-three-vertical',
};

/** The Phosphor icon for a shell glyph name, or null when there is none. */
export function shellIcon(name: string, filled = false): string | null {
	const ph = SHELL[name];
	return ph ? `ph:${ph}${filled ? '-fill' : ''}` : null;
}

/**
 * The Phosphor icon for an icon string a tab or menu carries (`atlas:<name>`
 * or a known Remix default), or the string unchanged when it is the owner's
 * own pick.
 */
export function shellIconFor(icon: string, filled = false): string {
	const ph = icon.startsWith('atlas:') ? SHELL[icon.slice('atlas:'.length)] : REMIX[icon];
	return ph ? `ph:${ph}${filled ? '-fill' : ''}` : icon;
}
