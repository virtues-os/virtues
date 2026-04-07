-- 027: Sidebar Reorganization
--
-- Restructures system sidebar: Chats / Pages / Wiki / Files / Connections
-- Developer section removed from DB (rendered client-side in footer).
-- Data renamed to Connections, /drive promoted to its own Files section.

-- Move /drive out of Data section
DELETE FROM app_space_items WHERE view_id = 'view_sys_sec_data' AND url = '/drive';

-- Create new "Files" section (sort 350 = between Wiki 300 and Data/Connections 400)
INSERT OR IGNORE INTO app_views (id, space_id, parent_view_id, name, icon, sort_order, view_type, query_config, is_system)
VALUES ('view_sys_sec_files', 'space_system', NULL, 'Files', 'ri:folder-line', 350, 'manual', NULL, TRUE);

INSERT OR IGNORE INTO app_space_items (view_id, url, sort_order)
VALUES ('view_sys_sec_files', '/drive', 0);

-- Rename Data → Connections with new icon
UPDATE app_views SET name = 'Connections', icon = 'ri:link' WHERE id = 'view_sys_sec_data';

-- Remove Developer section from DB (moves to frontend footer)
DELETE FROM app_space_items WHERE view_id = 'view_sys_sec_developer';
DELETE FROM app_views WHERE id = 'view_sys_sec_developer';
