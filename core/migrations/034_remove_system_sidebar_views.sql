-- 034: Remove system sidebar views from DB
--
-- System sections (Chats, Pages, Wiki, Files, Connections) are now defined
-- as frontend constants in apps/web/src/lib/sidebar/sections.ts.
-- This cleans up the redundant DB rows that were seeded in migrations 014/027/029.

-- Remove items belonging to system views
DELETE FROM app_space_items WHERE view_id IN (
    'view_sys_sec_chats',
    'view_sys_sec_pages',
    'view_sys_sec_wiki',
    'view_sys_sec_files',
    'view_sys_sec_data'
);

-- Remove the system views themselves
DELETE FROM app_views WHERE is_system = TRUE AND space_id = 'space_system';
