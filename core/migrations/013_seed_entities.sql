-- Seed initial entities for development
-- Person: Adam Jace, Place: Borghese Museum, Org: Virtues

-- Adam Jace (person)
INSERT OR IGNORE INTO wiki_people (
    id,
    canonical_name,
    emails,
    phones,
    relationship_category,
    notes
) VALUES (
    'person_' || lower(hex(randomblob(8))),
    'Adam Jace',
    '["adam@virtues.com"]',
    '[]',
    NULL,
    'Founder of Virtues'
);

-- Borghese Museum (place)
INSERT OR IGNORE INTO wiki_places (
    id,
    name,
    category,
    address,
    latitude,
    longitude
) VALUES (
    'place_' || lower(hex(randomblob(8))),
    'Galleria Borghese',
    'museum',
    'Piazzale Scipione Borghese, 5, 00197 Roma RM, Italy',
    41.9142,
    12.4922
);

-- Virtues (organization)
INSERT OR IGNORE INTO wiki_orgs (
    id,
    canonical_name,
    organization_type,
    relationship_type,
    content
) VALUES (
    'org_' || lower(hex(randomblob(8))),
    'Virtues',
    'company',
    'founder',
    'Personal data platform for the examined life.'
);
