-- =============================================================================
-- Demo Bookmarks Seed — the designer's saves, February 2026
-- =============================================================================
--
-- Same character as demo_day.sql: a UX designer in East Austin, mid house-hunt,
-- with a reno taking shape. Her saves are what that looks like — a facade she
-- screenshotted, a listing, a materials article, some type references, and the
-- city permit page she keeps meaning to read.
--
-- Populates `data_content_bookmark` (11 rows) across every state the UI has to
-- render, because a room built against uniformly-happy data lies:
--
--   * 7 enriched     — extraction record + extraction_text, so facets over
--                      medium/style/source_platform have real values and
--                      search has real text
--   * 2 pending      — a fetchable page still queued, so "N awaiting
--                      enrichment" is a live number rather than always zero
--   * 1 held         — asset-backed (a screenshot), which the sweep holds back
--                      until the pixel pass exists: pending, zero attempts,
--                      counted as "held for the image pass"
--   * 1 tombstoned   — removed at its source, so the list's hide-but-keep
--                      behavior is exercised (it must NOT appear in /bookmarks,
--                      and the note on it must survive)
--
-- Every door is represented: safari, chrome, arc, github star, in-app save,
-- iOS share. Notes are on some rows and not others, because that is the real
-- distribution and the "3 of yesterday's 7 saves have no why yet" prompt has
-- to have something to count.
--
-- ids are uuid v5 over source_stream_id under NAMESPACE_OID — the same
-- derivation `virtues_helpers::bookmarks` uses, so these rows are shaped
-- exactly like synced ones. source_stream_ids carry a `seed`/`SEED` marker so
-- they can never collide with a real sync.
--
-- extraction_text matches what `ExtractionRecord::to_embed_text` emits
-- (description, then labelled aspects, then likely_queries) — if that
-- rendering changes, this file should follow it.
--
-- Usage: psql "$DATABASE_URL" -f virtues-core/seeds/demo_bookmarks.sql
-- =============================================================================

INSERT INTO data_content_bookmark (
    id, url, title, description, source_platform, bookmark_type, author,
    tags, thumbnail_url, note, timestamp, source_stream_id, source_table,
    source_provider, deleted_at_source, metadata,
    enrichment_status, enriched_at, enrichment_model, extraction, extraction_text
) VALUES

-- 1. THE SCREENSHOT. A facade she saw on Instagram and kept. No page to fetch,
--    so `url` is the viewer route and the artifact is the image itself; the
--    sweep holds it for the pixel pass. Her note is the whole point of the row.
(
    'a4e275e6-719a-5b7d-9aa9-a738f14b0fb6',
    '/drive/file_seedhouse01',
    NULL, NULL,
    'instagram', 'screenshot', NULL,
    '["Reno"]'::jsonb,
    NULL,
    'the green door — and those shutters. for the front of the house',
    '2026-02-11 23:14:00+00', 'ios:share:sha256:9f2c1ab4e7d05c3861bb27a4f0e9d7c2',
    'ios_share', 'ios', NULL,
    '{"asset_id": "file_seedhouse01", "source_app": "com.burbn.instagram"}'::jsonb,
    'pending', NULL, NULL, NULL, NULL
),

-- 2. An Are.na channel of facades — saved by hand in the app, with a why.
(
    '9e7fd63a-325f-5879-add6-ab26636eb2b5',
    'https://www.are.na/channels/facades-and-frontages',
    'Facades and Frontages',
    'A channel collecting exterior details: stucco, timber, tile, and paint.',
    'web', 'save', NULL,
    '["Reno", "Reference"]'::jsonb,
    NULL,
    'colour references for the exterior — keep coming back to the cream + green',
    '2026-02-10 18:42:00+00', 'app:url:seed-arena-facades',
    'app_saves', 'app', NULL,
    '{}'::jsonb,
    'done', '2026-02-10 19:05:00+00', 'zai/glm-4.7-flash',
    '{"description":"An Are.na channel collecting exterior architectural details — stucco textures, timber cladding, tilework, and paint colours — gathered as visual reference.","medium":"reference","subject":["facades","exterior materials","colour reference","architecture","stucco"],"entities":["Are.na"],"style":"warm minimal, image-led, sparse text","likely_queries":["arena channel facades exterior references","cream and green house exterior ideas","stucco and timber facade reference"]}'::jsonb,
    E'An Are.na channel collecting exterior architectural details — stucco textures, timber cladding, tilework, and paint colours — gathered as visual reference.\nMedium: reference\nSubject: facades, exterior materials, colour reference, architecture, stucco\nMentions: Are.na\nStyle: warm minimal, image-led, sparse text\narena channel facades exterior references. cream and green house exterior ideas. stucco and timber facade reference'
),

-- 3. Safari bookmark, filed in a folder — the folder path IS the harvested why.
(
    '25cdf3ed-72cd-5413-8286-f3f95e734bf5',
    'https://en.wikipedia.org/wiki/Stucco',
    'Stucco - Wikipedia',
    'Construction material made of aggregates, a binder, and water.',
    'safari', 'bookmark', NULL,
    '["Reno", "Materials"]'::jsonb,
    'https://upload.wikimedia.org/wikipedia/commons/thumb/1/19/stucco.jpg',
    NULL,
    '2026-02-09 15:20:00+00', 'mac:seed-mbp:safari:BM-STUCCO-001',
    'mac_bookmarks', 'mac', NULL,
    '{"folder_path": ["Reno", "Materials"]}'::jsonb,
    'done', '2026-02-09 15:38:00+00', 'zai/glm-4.7-flash',
    '{"description":"Stucco is a construction material of aggregates, a binder and water, applied wet and hardening to a dense solid. Used as a coating for exterior walls and as a sculptural material.","medium":"reference","subject":["stucco","render","construction materials","wall coatings","architectural finishes"],"entities":["Wikipedia"],"style":null,"likely_queries":["what is stucco made of","stucco vs plaster difference","stucco on exterior walls","how to patch stucco"]}'::jsonb,
    E'Stucco is a construction material of aggregates, a binder and water, applied wet and hardening to a dense solid. Used as a coating for exterior walls and as a sculptural material.\nMedium: reference\nSubject: stucco, render, construction materials, wall coatings, architectural finishes\nMentions: Wikipedia\nwhat is stucco made of. stucco vs plaster difference. stucco on exterior walls. how to patch stucco'
),

-- 4. A GitHub star. Container signal is repo topics, not a folder.
(
    '72bb5721-1ca4-53e4-b4ab-1e289a19b5fe',
    'https://github.com/seed-org/design-tokens',
    'seed-org/design-tokens',
    'A small, opinionated design token pipeline for multi-platform UI.',
    'github', 'star', 'seed-org',
    '["design-systems", "typescript", "tokens"]'::jsonb,
    NULL, NULL,
    '2026-02-06 21:03:00+00', 'github:star:SEED_R_kgDOLd7design',
    'github_stars', 'github', NULL,
    '{"language": "TypeScript"}'::jsonb,
    'done', '2026-02-06 21:30:00+00', 'zai/glm-4.7-flash',
    '{"description":"A design token pipeline that compiles a single source of truth into platform-specific outputs for web, iOS and Android.","medium":"repository","subject":["design tokens","design systems","build tooling","cross-platform theming"],"entities":["TypeScript","Style Dictionary"],"style":null,"likely_queries":["design token pipeline repo","multi platform design tokens tool","design system tokens typescript"]}'::jsonb,
    E'A design token pipeline that compiles a single source of truth into platform-specific outputs for web, iOS and Android.\nMedium: repository\nSubject: design tokens, design systems, build tooling, cross-platform theming\nMentions: TypeScript, Style Dictionary\ndesign token pipeline repo. multi platform design tokens tool. design system tokens typescript'
),

-- 5. Chrome bookmark — a type specimen. `style` is populated here, which is
--    what the design-vocabulary facet needs to be worth showing.
(
    'd5adef9a-1a6d-59e5-878d-214139bd3dc6',
    'https://www.typespecimens.xyz/specimens/seed-grotesk',
    'Seed Grotesk — Specimen',
    'A neo-grotesque with a tall x-height and a narrow set.',
    'chrome', 'bookmark', NULL,
    '["Type"]'::jsonb,
    NULL,
    'for the house numbers / signage',
    '2026-02-05 11:15:00+00', 'mac:seed-mbp:chrome:BM-TYPE-014',
    'mac_bookmarks', 'mac', NULL,
    '{"folder_path": ["Type"]}'::jsonb,
    'done', '2026-02-05 11:40:00+00', 'zai/glm-4.7-flash',
    '{"description":"A digital type specimen for a neo-grotesque family, showing weights, optical sizes and sample settings at display and text sizes.","medium":"reference","subject":["typography","type specimen","neo-grotesque","signage"],"entities":["Seed Grotesk"],"style":"swiss, high-contrast, grid-led, black on white","likely_queries":["neo grotesque specimen tall x height","typeface for house numbers signage","grotesk specimen site"]}'::jsonb,
    E'A digital type specimen for a neo-grotesque family, showing weights, optical sizes and sample settings at display and text sizes.\nMedium: reference\nSubject: typography, type specimen, neo-grotesque, signage\nMentions: Seed Grotesk\nStyle: swiss, high-contrast, grid-led, black on white\nneo grotesque specimen tall x height. typeface for house numbers signage. grotesk specimen site'
),

-- 6. Safari Reading List — the listing itself. A different bookmark_type from
--    the same platform, which the type facet needs in order to mean anything.
(
    '23f80cfb-3259-5af8-bfd2-4b328a57f81c',
    'https://www.example-realty.com/listing/cherrywood-1912',
    '1912 Cherrywood Rd — 3 bed, 2 bath',
    'Bungalow, 1,480 sq ft, original stucco, needs work.',
    'safari', 'reading_list', NULL,
    NULL,
    NULL,
    'the one with the porch. showing on Friday',
    '2026-02-12 09:05:00+00', 'mac:seed-mbp:safari:RL-CHERRYWOOD-003',
    'mac_bookmarks', 'mac', NULL,
    '{}'::jsonb,
    'done', '2026-02-12 09:22:00+00', 'zai/glm-4.7-flash',
    '{"description":"A property listing for a 1,480 sq ft bungalow on Cherrywood Road with original stucco and a covered porch, described as needing renovation.","medium":"product","subject":["property listing","bungalow","East Austin","renovation","real estate"],"entities":["Cherrywood Road","Austin"],"style":null,"likely_queries":["cherrywood bungalow listing austin","house with the porch showing friday","1912 cherrywood road"]}'::jsonb,
    E'A property listing for a 1,480 sq ft bungalow on Cherrywood Road with original stucco and a covered porch, described as needing renovation.\nMedium: product\nSubject: property listing, bungalow, East Austin, renovation, real estate\nMentions: Cherrywood Road, Austin\ncherrywood bungalow listing austin. house with the porch showing friday. 1912 cherrywood road'
),

-- 7. PENDING — saved minutes ago, not yet swept. Keeps the status line honest.
(
    '27678610-ab44-5754-9f40-27f100983148',
    'https://www.youtube.com/watch?v=seedplaster',
    NULL, NULL,
    'web', 'save', NULL,
    NULL, NULL, NULL,
    '2026-02-13 08:47:00+00', 'app:url:seed-yt-plaster',
    'app_saves', 'app', NULL,
    '{}'::jsonb,
    'pending', NULL, NULL, NULL, NULL
),

-- 8. Arc bookmark — a layout reference, enriched with style vocabulary.
(
    'f57eb9b2-e3f7-5c8e-b4e7-6dd5f64c902c',
    'https://seed.gridsystems.dev/baseline',
    'Baseline grids on the web',
    'Getting vertical rhythm to survive real content.',
    'arc', 'bookmark', NULL,
    '["Craft"]'::jsonb,
    NULL, NULL,
    '2026-02-03 16:30:00+00', 'mac:seed-mbp:arc:BM-GRID-021',
    'mac_bookmarks', 'mac', NULL,
    '{"folder_path": ["Craft"]}'::jsonb,
    'done', '2026-02-03 16:52:00+00', 'zai/glm-4.7-flash',
    '{"description":"An article on maintaining a baseline grid and vertical rhythm in web layout when content length and image sizes are unpredictable.","medium":"article","subject":["baseline grid","vertical rhythm","web layout","typography","CSS"],"entities":["CSS"],"style":"editorial, generous whitespace, restrained palette","likely_queries":["baseline grid web vertical rhythm","keep vertical rhythm with real content","css baseline grid article"]}'::jsonb,
    E'An article on maintaining a baseline grid and vertical rhythm in web layout when content length and image sizes are unpredictable.\nMedium: article\nSubject: baseline grid, vertical rhythm, web layout, typography, CSS\nMentions: CSS\nStyle: editorial, generous whitespace, restrained palette\nbaseline grid web vertical rhythm. keep vertical rhythm with real content. css baseline grid article'
),

-- 9. The civic page she keeps meaning to read. Note is a todo, not a reason —
--    which is exactly why the column is `note` and not `why`.
(
    'ca1c96ee-ddd6-506f-af80-50187f45bdc9',
    'https://www.austintexas.gov/page/residential-permits',
    'Residential Permits — City of Austin',
    'Permitting requirements for residential alterations and additions.',
    'chrome', 'bookmark', NULL,
    '["Reno", "Admin"]'::jsonb,
    NULL,
    'check the setback rules before drawing anything',
    '2026-02-08 13:10:00+00', 'mac:seed-mbp:chrome:BM-PERMIT-009',
    'mac_bookmarks', 'mac', NULL,
    '{"folder_path": ["Reno", "Admin"]}'::jsonb,
    'done', '2026-02-08 13:31:00+00', 'zai/glm-4.7-flash',
    '{"description":"City of Austin guidance on residential permitting: which alterations and additions require a permit, what to submit, and how review works.","medium":"documentation","subject":["building permits","residential renovation","setbacks","city regulations","Austin"],"entities":["City of Austin"],"style":null,"likely_queries":["austin residential permit requirements","do i need a permit for an addition austin","austin setback rules renovation"]}'::jsonb,
    E'City of Austin guidance on residential permitting: which alterations and additions require a permit, what to submit, and how review works.\nMedium: documentation\nSubject: building permits, residential renovation, setbacks, city regulations, Austin\nMentions: City of Austin\naustin residential permit requirements. do i need a permit for an addition austin. austin setback rules renovation'
),

-- 10. A second share, this one carrying a real source URL AND a stored image —
--     the both-not-either case. Still held for the pixel pass.
(
    '3e76bc29-3ab0-597c-bbeb-f01c5bc3cbd2',
    'https://www.instagram.com/p/seedtilework/',
    NULL, NULL,
    'instagram', 'screenshot', NULL,
    NULL, NULL,
    'tile pattern for the entry',
    '2026-02-12 21:38:00+00', 'ios:share:sha256:4b81dd60c2a94e17ac35f8b6019e73aa',
    'ios_share', 'ios', NULL,
    '{"asset_id": "file_seedtile02", "source_app": "com.burbn.instagram"}'::jsonb,
    'pending', NULL, NULL, NULL, NULL
),

-- 11. TOMBSTONED — she deleted this bookmark in Safari, so it must not appear
--     in the room. The row stays because the note is hers and a re-add should
--     restore rather than duplicate.
(
    'cecf7892-97bc-54bf-8655-bd9101803066',
    'https://www.example-realty.com/listing/manor-rd-404',
    '404 Manor Rd — sold',
    NULL,
    'safari', 'bookmark', NULL,
    NULL, NULL,
    'passed on this one — kitchen was too dark',
    '2026-01-28 19:00:00+00', 'mac:seed-mbp:safari:BM-DEAD-099',
    'mac_bookmarks', 'mac', '2026-02-07 08:00:00+00',
    '{}'::jsonb,
    'done', '2026-01-28 19:20:00+00', 'zai/glm-4.7-flash',
    '{"description":"A property listing for a house on Manor Road, since marked sold.","medium":"product","subject":["property listing","East Austin","real estate"],"entities":["Manor Road"],"style":null,"likely_queries":["manor road listing austin","the one with the dark kitchen"]}'::jsonb,
    E'A property listing for a house on Manor Road, since marked sold.\nMedium: product\nSubject: property listing, East Austin, real estate\nMentions: Manor Road\nmanor road listing austin. the one with the dark kitchen'
)

ON CONFLICT DO NOTHING;
