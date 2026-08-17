-- =============================================================================
-- Demo Bookmarks Seed — the designer's saves, February 2026
-- =============================================================================
--
-- Same character as demo_day.sql: a UX designer in East Austin, mid house-hunt,
-- with a reno taking shape. Her saves are what that looks like — a facade she
-- screenshotted, a listing, a materials article, some type references, and the
-- city permit page she keeps meaning to read.
--
-- Populates `data_content_bookmark` (16 rows) across every state the room has
-- to render, because a room built against uniformly-happy data lies:
--
--   *  12 enriched   — extraction record + extraction_text, so the Wall has
--                      text tiles and the medium/style facets carry values.
--                      SIX of these carry a real og:image, which is what makes
--                      the Wall's picture tiles picture tiles.
--   *   1 queued     — a fetchable page not yet swept, so "N still to read" is
--                      a live number rather than permanently zero
--   *   2 held       — asset-backed saves the sweep holds back until the image
--                      pass exists: pending, zero attempts, counted apart
--   *   1 tombstoned — removed at its source, so the hide-but-keep behavior is
--                      exercised (must NOT appear, and its note must survive)
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

-- THE SCREENSHOT. A facade she saw on Instagram and kept. No page to fetch,
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

-- An Are.na channel of facades — saved by hand in the app, with a why.
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

-- A GitHub star. Container signal is repo topics, not a folder.
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

-- Chrome bookmark — a type specimen. `style` is populated here, which is
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

-- Safari Reading List — the listing itself. A different bookmark_type from
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

-- PENDING — saved minutes ago, not yet swept. Keeps the status line honest.
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

-- Arc bookmark — a layout reference, enriched with style vocabulary.
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

-- The civic page she keeps meaning to read. Note is a todo, not a reason —
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

-- A second share, this one carrying a real source URL AND a stored image —
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

-- TOMBSTONED — she deleted this bookmark in Safari, so it must not appear
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

-- ---------------------------------------------------------------------------
-- Real pages, real pipeline output (generated 2026-08-05)
-- ---------------------------------------------------------------------------
--
-- Everything below was produced by actually RUNNING the enrichment sweep
-- against these live URLs — the thumbnails are each page's own og:image and
-- the extraction records are what the Lite slot returned. None of it is
-- invented, which is the point: the Wall renders real pictures, and the facets
-- carry the values the pipeline genuinely emits rather than the values I
-- imagined it would.
--
-- To regenerate: re-queue these rows (set enrichment_status='pending',
-- extraction=NULL, thumbnail_url=NULL) and run the bookmark_enrichment applet.
-- ---------------------------------------------------------------------------

INSERT INTO data_content_bookmark (
    id, url, title, description, source_platform, bookmark_type, tags,
    thumbnail_url, note, timestamp, source_stream_id, source_table,
    source_provider, metadata, enrichment_status, enriched_at,
    enrichment_model, extraction, extraction_text
) VALUES
-- A photo reference. Unsplash renders its page in JS so the fetch gets NO
--     body text — but its og:description carries the alt text, which turned out
--     to be enough for a real record. The seed's evidence that image-led saves
--     can enrich from metadata alone.
(
    '5318d8a4-d4f1-53b9-b49b-2284739504e8',
    'https://unsplash.com/photos/white-stucco-house-with-lush-green-foliage-and-fence-FAa-KyRCnCE',
    'Photo by Joseph Kellner on Unsplash',
    'Beach home in Bermuda – Download this photo by Joseph Kellner on Unsplash',
    'web', 'save', '["Reno"]'::jsonb,
    'https://images.unsplash.com/photo-1771529173150-c3d266df0c0d?mark=https%3A%2F%2Fimages.unsplash.com%2Fopengraph%2Flogo.png&mark-w=64&mark-align=top%2Cleft&mark-pad=50&h=630&w=1200&crop=faces%2Cedges&blend-w=1&blend=000000&blend-mode=normal&blend-alpha=10&auto=format&fit=crop&q=60&ixid=M3wxMjA3fDB8MXxhbGx8fHx8fHx8fHwxNzg1OTc2ODEwfA&ixlib=rb-4.1.0',
    'this is the render colour I keep meaning to describe',
    '2026-02-11T14:05:00-06:00', 'app:url:seed-uns-house',
    'app_saves', 'app', '{}'::jsonb,
    'done', '2026-02-11T14:05:00-06:00', 'zai/glm-4.7-flash',
    '{"style": "minimalist photography, bright natural lighting, clean lines", "medium": "image", "subject": ["architecture", "beach house", "Bermuda", "fence", "foliage", "stucco"], "entities": ["Joseph Kellner", "Unsplash", "Bermuda"], "description": "A photograph of a white stucco house with lush green foliage and a fence, described as a beach home in Bermuda.", "likely_queries": ["white stucco house with green fence photo", "Bermuda beach house photo", "Joseph Kellner Unsplash house", "white house with green leaves photo", "Bermuda architecture photo", "stucco house with foliage"]}'::jsonb,
    'A photograph of a white stucco house with lush green foliage and a fence, described as a beach home in Bermuda.
Medium: image
Subject: architecture, beach house, Bermuda, fence, foliage, stucco
Mentions: Joseph Kellner, Unsplash, Bermuda
Style: minimalist photography, bright natural lighting, clean lines
white stucco house with green fence photo. Bermuda beach house photo. Joseph Kellner Unsplash house. white house with green leaves photo. Bermuda architecture photo. stucco house with foliage'
),
-- The article she keeps open. Real Wikipedia lead image.
(
    '25cdf3ed-72cd-5413-8286-f3f95e734bf5',
    'https://en.wikipedia.org/wiki/Stucco',
    'Stucco - Wikipedia',
    'Construction material made of aggregates, a binder, and water.',
    'safari', 'bookmark', '["Reno", "Materials"]'::jsonb,
    'https://upload.wikimedia.org/wikipedia/commons/thumb/1/19/Ceilings_of_the_rotonde_de_Mars_%28509%29.jpg/1280px-Ceilings_of_the_rotonde_de_Mars_%28509%29.jpg',
    NULL,
    '2026-02-09T09:20:00-06:00', 'mac:seed-mbp:safari:BM-STUCCO-001',
    'mac_bookmarks', 'mac', '{"folder_path": ["Reno", "Materials"]}'::jsonb,
    'done', '2026-02-09T09:20:00-06:00', 'zai/glm-4.7-flash',
    '{"medium": "reference", "subject": ["stucco composition and materials", "stucco application methods", "stucco history and evolution", "stucco artistic and sculptural uses", "stucco types and finishes", "stucco terminology and language distinctions"], "entities": ["Wikipedia", "Louvre Palace", "Gaspard and Balthazard Marsy", "Portland cement", "lime", "sand", "Jasna G\u00f3ra Monastery", "Wessobrunner School", "Bridges Hall of Music"], "description": "A Wikipedia article defining stucco as a construction material made of aggregates, a binder, and water, covering its composition, history, application methods, and artistic uses.", "likely_queries": ["what is stucco made of", "stucco vs plaster difference", "traditional stucco three coat method", "stucco application on wood frame", "stucco decorative relief art", "stucco rock dash finish"]}'::jsonb,
    'A Wikipedia article defining stucco as a construction material made of aggregates, a binder, and water, covering its composition, history, application methods, and artistic uses.
Medium: reference
Subject: stucco composition and materials, stucco application methods, stucco history and evolution, stucco artistic and sculptural uses, stucco types and finishes, stucco terminology and language distinctions
Mentions: Wikipedia, Louvre Palace, Gaspard and Balthazard Marsy, Portland cement, lime, sand, Jasna Góra Monastery, Wessobrunner School, Bridges Hall of Music
what is stucco made of. stucco vs plaster difference. traditional stucco three coat method. stucco application on wood frame. stucco decorative relief art. stucco rock dash finish'
),
-- Materials research, with a note.
(
    'f959c5e6-45ba-5ce5-8ece-3678203bd909',
    'https://en.wikipedia.org/wiki/Bungalow',
    'Bungalow - Wikipedia',
    'A bungalow is a small, typically single-storey house or cottage, with origins in the Bengal style and popularity in the early 20th century, particularly in the United States and United Kingdom. The article covers the history, design, and regional variations of the bungalow style across countries including Australia, India, Canada, Germany, Ireland, Singapore, Malaysia, South Africa, and the UK.',
    'safari', 'bookmark', '["Reno", "Materials"]'::jsonb,
    'https://upload.wikimedia.org/wikipedia/commons/thumb/2/2f/149MyrtleReedsburgWI.JPG/1280px-149MyrtleReedsburgWI.JPG',
    'what we are actually buying',
    '2026-02-07T08:00:00-06:00', 'mac:seed-mbp:safari:BM-BUNGALOW-002',
    'mac_bookmarks', 'mac', '{"folder_path": ["Reno", "Materials"]}'::jsonb,
    'done', '2026-02-07T08:00:00-06:00', 'zai/glm-4.7-flash',
    '{"medium": "reference", "subject": ["bungalow architecture", "single-storey houses", "Arts and Crafts movement", "mail-order house kits", "Federation Bungalow", "California bungalow", "Lutyens'' Bungalow Zone", "dak bungalow"], "entities": ["Sears, Roebuck & Co.", "Kansas City Bungalow Club", "Twin Cities Bungalow Club", "Richard Stanton", "Sep Ruf", "Manale Tea Bungalow", "Jim Thompson cottage", "Boulton & Paul Ltd", "Landmark Trust", "Lutyens'' Bungalow Zone", "Good Class Bungalows"], "description": "A bungalow is a small, typically single-storey house or cottage, with origins in the Bengal style and popularity in the early 20th century, particularly in the United States and United Kingdom. The article covers the history, design, and regional variations of the bungalow style across countries including Australia, India, Canada, Germany, Ireland, Singapore, Malaysia, South Africa, and the UK.", "likely_queries": ["definition of a bungalow house", "history of the bungalow style", "Sears Roebuck bungalow kits", "Federation Bungalow Australia", "California bungalow style", "Lutyens bungalow zone New Delhi", "dak bungalow meaning", "bungalow vs cottage", "bungalow design features", "bungalow vs chalet bungalow", "bungalow vs townhouse", "bungalow vs ranch house"]}'::jsonb,
    'A bungalow is a small, typically single-storey house or cottage, with origins in the Bengal style and popularity in the early 20th century, particularly in the United States and United Kingdom. The article covers the history, design, and regional variations of the bungalow style across countries including Australia, India, Canada, Germany, Ireland, Singapore, Malaysia, South Africa, and the UK.
Medium: reference
Subject: bungalow architecture, single-storey houses, Arts and Crafts movement, mail-order house kits, Federation Bungalow, California bungalow, Lutyens'' Bungalow Zone, dak bungalow
Mentions: Sears, Roebuck & Co., Kansas City Bungalow Club, Twin Cities Bungalow Club, Richard Stanton, Sep Ruf, Manale Tea Bungalow, Jim Thompson cottage, Boulton & Paul Ltd, Landmark Trust, Lutyens'' Bungalow Zone, Good Class Bungalows
definition of a bungalow house. history of the bungalow style. Sears Roebuck bungalow kits. Federation Bungalow Australia. California bungalow style. Lutyens bungalow zone New Delhi. dak bungalow meaning. bungalow vs cottage. bungalow design features. bungalow vs chalet bungalow. bungalow vs townhouse. bungalow vs ranch house'
),
-- No note — the majority case, and what the review prompt counts.
(
    '8b127b09-fdc4-59b0-b192-ee49a0db666a',
    'https://en.wikipedia.org/wiki/Terrazzo',
    'Terrazzo - Wikipedia',
    'Terrazzo is a composite material made of chips of stone, glass, or other aggregate bound with cement or resin, used for flooring and wall treatments. The page covers its history, production methods, types of systems, and deterioration issues.',
    'chrome', 'bookmark', '["Reno", "Materials"]'::jsonb,
    'https://upload.wikimedia.org/wikipedia/commons/thumb/8/89/Terrazzo_entryway.jpg/1280px-Terrazzo_entryway.jpg?utm_source=en.wikipedia.org&utm_campaign=index&utm_content=thumbnail',
    NULL,
    '2026-02-04T04:20:00-06:00', 'mac:seed-mbp:chrome:BM-TERRAZZO-011',
    'mac_bookmarks', 'mac', '{"folder_path": ["Reno", "Materials"]}'::jsonb,
    'done', '2026-02-04T04:20:00-06:00', 'zai/glm-4.7-flash',
    '{"medium": "reference", "subject": ["composite materials", "flooring", "cementitious materials", "epoxy terrazzo", "marble chips", "archaeology", "Hollywood Walk of Fame"], "entities": ["Hollywood Walk of Fame", "L. Del Turco and Bros.", "National Terrazzo and Mosaic Organization", "Vox", "McGraw Hill", "Bureau of Labor Statistics", "archtoolbox.com", "Wikimedia Commons"], "description": "Terrazzo is a composite material made of chips of stone, glass, or other aggregate bound with cement or resin, used for flooring and wall treatments. The page covers its history, production methods, types of systems, and deterioration issues.", "likely_queries": ["how terrazzo floors are made", "history of terrazzo flooring", "epoxy terrazzo vs cement terrazzo", "how to repair cracked terrazzo", "what is terrazzo made of", "terrazzo floor installation process"]}'::jsonb,
    'Terrazzo is a composite material made of chips of stone, glass, or other aggregate bound with cement or resin, used for flooring and wall treatments. The page covers its history, production methods, types of systems, and deterioration issues.
Medium: reference
Subject: composite materials, flooring, cementitious materials, epoxy terrazzo, marble chips, archaeology, Hollywood Walk of Fame
Mentions: Hollywood Walk of Fame, L. Del Turco and Bros., National Terrazzo and Mosaic Organization, Vox, McGraw Hill, Bureau of Labor Statistics, archtoolbox.com, Wikimedia Commons
how terrazzo floors are made. history of terrazzo flooring. epoxy terrazzo vs cement terrazzo. how to repair cracked terrazzo. what is terrazzo made of. terrazzo floor installation process'
),
-- One folder rather than two: folder paths are the harvested why for
--     browser bookmarks.
(
    '68484eaa-751c-53f3-8833-58a924781a54',
    'https://en.wikipedia.org/wiki/Adobe',
    'Adobe - Wikipedia',
    'Adobe is a building material made from loam and organic materials, such as straw or dung, used to create sun-dried bricks and structures. The page covers its history, etymology, composition, material properties, and construction methods.',
    'safari', 'bookmark', '["Materials"]'::jsonb,
    'https://upload.wikimedia.org/wikipedia/commons/thumb/f/f2/Adobe_wall_%28detail%29_1.jpg/960px-Adobe_wall_%28detail%29_1.jpg?utm_source=en.wikipedia.org&utm_campaign=index&utm_content=thumbnail',
    NULL,
    '2026-02-02T03:40:00-06:00', 'mac:seed-mbp:safari:BM-ADOBE-007',
    'mac_bookmarks', 'mac', '{"folder_path": ["Materials"]}'::jsonb,
    'done', '2026-02-02T03:40:00-06:00', 'zai/glm-4.7-flash',
    '{"medium": "reference", "subject": ["adobe bricks", "earthen construction", "rammed earth", "thermal mass", "building materials", "mudbrick", "architecture"], "entities": ["New Mexico", "Mali", "Chile", "Spain", "Guatemala", "Bam", "New Mexico State University", "Hugh W. Comstock", "Franklin & Kump Associates", "Carmel High School", "Cooperative State Research, Education, and Extension Service"], "description": "Adobe is a building material made from loam and organic materials, such as straw or dung, used to create sun-dried bricks and structures. The page covers its history, etymology, composition, material properties, and construction methods.", "likely_queries": ["how to make adobe bricks", "adobe building material composition", "adobe wall construction techniques", "history of adobe architecture", "adobe vs rammed earth", "thermal mass of adobe walls"]}'::jsonb,
    'Adobe is a building material made from loam and organic materials, such as straw or dung, used to create sun-dried bricks and structures. The page covers its history, etymology, composition, material properties, and construction methods.
Medium: reference
Subject: adobe bricks, earthen construction, rammed earth, thermal mass, building materials, mudbrick, architecture
Mentions: New Mexico, Mali, Chile, Spain, Guatemala, Bam, New Mexico State University, Hugh W. Comstock, Franklin & Kump Associates, Carmel High School, Cooperative State Research, Education, and Extension Service
how to make adobe bricks. adobe building material composition. adobe wall construction techniques. history of adobe architecture. adobe vs rammed earth. thermal mass of adobe walls'
),
-- The other photo. `medium: image` plus a populated `style` are what make
--     the Kind and style facets worth showing at all.
(
    '7aa04ee7-f05a-500b-acf5-9e8a70364679',
    'https://unsplash.com/photos/a-close-up-of-a-tiled-floor-with-a-red-and-white-checkerboard-pattern-7pr9GJfdWM8',
    'Photo by Vladislav Glukhotko on Unsplash',
    'Tile rhombus pattern – Download this photo by Vladislav Glukhotko on Unsplash',
    'web', 'save', NULL,
    'https://images.unsplash.com/photo-1678742755904-6c3fc8ba6602?mark=https%3A%2F%2Fimages.unsplash.com%2Fopengraph%2Flogo.png&mark-w=64&mark-align=top%2Cleft&mark-pad=50&h=630&w=1200&crop=faces%2Cedges&blend-w=1&blend=000000&blend-mode=normal&blend-alpha=10&auto=format&fit=crop&q=60&ixid=M3wxMjA3fDB8MXxhbGx8fHx8fHx8fHwxNzg1ODQwNjQ2fA&ixlib=rb-4.1.0',
    NULL,
    '2026-02-01T11:12:00-06:00', 'app:url:seed-uns-tile',
    'app_saves', 'app', '{}'::jsonb,
    'done', '2026-02-01T11:12:00-06:00', 'zai/glm-4.7-flash',
    '{"style": "clean, high-resolution photography", "medium": "image", "subject": ["flooring", "tile patterns", "checkerboard", "red and white", "interior design"], "entities": ["Vladislav Glukhotko", "Unsplash"], "description": "A close-up photograph of a tiled floor featuring a red and white checkerboard pattern.", "likely_queries": ["red and white checkerboard floor tile pattern", "close up of checkerboard floor", "rhombus tile pattern photo", "red white checkerboard floor image", "interior design floor tile pattern"]}'::jsonb,
    'A close-up photograph of a tiled floor featuring a red and white checkerboard pattern.
Medium: image
Subject: flooring, tile patterns, checkerboard, red and white, interior design
Mentions: Vladislav Glukhotko, Unsplash
Style: clean, high-resolution photography
red and white checkerboard floor tile pattern. close up of checkerboard floor. rhombus tile pattern photo. red white checkerboard floor image. interior design floor tile pattern'
)

ON CONFLICT DO NOTHING;
