-- 0088_add_page_layout
--
-- How a page is laid out. `paper` is the screen or a printable sheet (A4 or
-- US Letter), `column_count` is one column or two, and `masthead` is a
-- report's header: the label above the title, the byline, the date its
-- records run through, its subjects as app routes, and its abstract. A
-- page with no masthead has none drawn.
ALTER TABLE app_pages
    ADD COLUMN paper text NOT NULL DEFAULT 'screen'
        CHECK (paper IN ('screen', 'a4', 'letter')),
    ADD COLUMN column_count smallint NOT NULL DEFAULT 1
        CHECK (column_count IN (1, 2)),
    ADD COLUMN masthead jsonb;
