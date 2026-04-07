# Baseline Seed Data Guide — 12-Week Character Pattern

## Character
- **Name:** (unnamed UX designer, female, early 30s)
- **Lives:** Mueller, East Austin, TX
- **Works:** Senior UX Designer at Canopy (B2B SaaS), downtown Austin office
- **Timezone:** America/Chicago (UTC-6 in winter, UTC-5 in summer — all baseline is winter/CST)

## Entity IDs

### People
| ID | Name | Frequency | Pattern |
|----|------|-----------|---------|
| `person_demo_maya` | Maya Chen | Daily weekdays | Design team lead, lunch buddy |
| `person_demo_david` | David Okafor | 3-4x/week | Design engineer, in standups + reviews |
| `person_demo_jess` | Jess Landry | 1x/week (Fri/Sat) | Close friend, game nights |
| `person_demo_priya` | Priya Mehta | 1x/week (with Jess) | Friend, part of game night crew |
| `person_demo_rachel` | Rachel Torres | RARE (3x total in 12 weeks) | Realtor, house hunting |
| `person_demo_mom` | Linda (Mom) | 1x/week (Fri or Sat) | Weekly phone calls |

### Places
| ID | Name | Frequency | Pattern |
|----|------|-----------|---------|
| `place_demo_home` | Home | Daily | Mueller, East Austin |
| `place_demo_office` | Office | Weekdays (4-5x/week, occasional WFH) | Downtown Austin |
| `place_demo_ramen` | Ramen Tatsu-ya | 1x/week (lunch with Maya) | Go-to lunch spot |
| `place_demo_jos` | Jo's Coffee | 1-2x/month | South Congress |
| `place_demo_house` | 1847 S 3rd St | ONLY on Feb 13 | Bouldin Creek house showing |
| `place_demo_jess` | Jess's Place | 1x/week | South Lamar, game nights |
| `place_demo_ladybird` | Lady Bird Lake | 1-2x/week (weekends) | Walks/runs |
| `place_demo_mueller_trails` | Mueller Trails | 2-3x/week | Regular running route |

### Organizations
| ID | Name | Pattern |
|----|------|---------|
| `org_demo_employer` | Canopy | Every work event |
| `org_demo_realty` | Torres Realty | ONLY with Rachel |

## Topic Vocabulary (use these consistently)

### Work topics
- `"work"` — general work
- `"design"` — design tasks (Figma, reviews, wireframes)
- `"meeting"` — standups, reviews, 1:1s
- `"onboarding"` — the onboarding funnel redesign (active project weeks 8-12)
- `"research"` — user research sessions
- `"focus"` — deep work / flow states

### Life topics
- `"routine"` — morning routine, getting ready
- `"commute"` — bike ride, drives
- `"exercise"` — running, walking
- `"running"` — specifically running
- `"outdoors"` — Lady Bird Lake walks, parks
- `"social"` — hanging out with friends
- `"games"` — game nights (Catan, etc.)
- `"food"` — meals, cooking, restaurants
- `"coffee"` — coffee shops
- `"family"` — Mom calls, family stuff
- `"phone-call"` — phone conversations
- `"leisure"` — reading, movies, TV
- `"browsing"` — web browsing
- `"messaging"` — Slack, texts
- `"sleep"` — sleep events
- `"house-hunting"` — RARE, only weeks 10-12 (3 appearances)
- `"real-estate"` — RARE, same as house-hunting window
- `"reflection"` — journaling, voice memos

## Weekly Rhythm

### Typical Weekday (Mon-Fri)
~10 events per day:
1. Sleep (00:00-06:30, ~6.5h) — `is_sleep=1, novelty_z=NULL`
2. Morning routine (06:30-07:15) — `["routine", "messaging"]`, entities: `[place_demo_home]`
3. Bike commute (07:15-07:45) — `["commute", "cycling"]`, transit=1
4. Office morning block (07:45-12:00) — 2-3 events:
   - Coffee + Slack (07:45-08:15) — `["messaging", "work"]`
   - Standup (08:15-09:00, if weekday) — `["meeting", "design"]`, entities: `[person_demo_maya, person_demo_david, place_demo_office]`
   - Focused work (09:00-11:30) — `["design", "focus"]` or `["work", "focus"]`
5. Lunch (11:30-12:30) — `["food", "social"]`, entities vary (Maya 1x/week at Tatsu-ya, otherwise solo/other)
6. Afternoon block (12:30-16:30) — 1-2 events: meetings, more focused work, research sessions
7. Bike commute home (16:30-17:00) — `["commute"]`, transit=1
8. Evening (17:00-22:00) — exercise, dinner, social/leisure
9. Wind down (22:00-00:00) — `["leisure", "browsing"]`

### Typical Weekend (Sat-Sun)
~6-8 events per day:
1. Sleep (00:00-07:30, longer) — `is_sleep=1`
2. Slow morning (07:30-09:00) — `["routine"]`
3. Activity (09:00-12:00) — Lady Bird Lake walk, errands, brunch
4. Afternoon (12:00-17:00) — varies: reading, projects, social
5. Evening (17:00-22:00) — dinner, friends (Fri/Sat game night), movie
6. Wind down (22:00-00:00)

### Weekly Specials
- **Monday:** Regular standup, longer focus block. No special social.
- **Tuesday:** Standup + design review with David. Run in evening (Mueller trails).
- **Wednesday:** Standup. Lunch at Tatsu-ya with Maya (~weekly).
- **Thursday:** Standup. Sometimes WFH afternoon. Run or walk.
- **Friday:** Standup. Sometimes shorter day. Game night at Jess's (most Fridays). Mom call (evening).
- **Saturday:** Lady Bird Lake walk. Errands. Sometimes half-day office. Movie night.
- **Sunday:** Slow day. Reading, cooking, prep for the week.

## Date Range
- Baseline: **November 24, 2025 (Monday) through February 14, 2026 (Saturday)**
- That's exactly 12 weeks (84 days)
- Feb 12-14 already have detailed seed data — DON'T duplicate those days

## SQL Format

Use `INSERT OR IGNORE` for idempotency. Each event:

```sql
INSERT OR IGNORE INTO wiki_events (
    id, day_id, start_time, end_time,
    auto_label, auto_location, source_ontologies,
    is_unknown, is_transit, is_sleep, is_user_added, is_user_edited,
    event_summary, topics, entities,
    novelty_z, agent_action
) VALUES (
    'ev_bNNNN', 'day_YYYY-MM-DD',
    'YYYY-MM-DDThh:mm:00Z', 'YYYY-MM-DDThh:mm:00Z',
    'Short Label', 'Location Name', '["source1", "source2"]',
    0, 0, 0, 0, 0,
    'One sentence factual summary.', '["topic1", "topic2"]', '["entity_id_1", "entity_id_2"]',
    NULL, 'NEW'
);
```

- Event IDs: `ev_bNNNN` where NNNN is sequential (ev_b0001, ev_b0002, ...)
- Day IDs: `day_YYYY-MM-DD` format
- All times in UTC (CST + 6 hours)
- `novelty_z` = NULL for ALL baseline events (will be computed at runtime)
- `agent_action` = 'NEW' for all
- `topic_novelty` and `entity_novelty` = NULL (computed at runtime)
- Sleep events: `is_sleep=1`, no topics/entities
- Transit events: `is_transit=1`
- Keep summaries short (1 sentence)

## Wiki Days
Each day needs a wiki_days row:
```sql
INSERT OR IGNORE INTO wiki_days (id, date, start_timezone, end_timezone, morning_baseline)
VALUES ('day_YYYY-MM-DD', 'YYYY-MM-DD', 'America/Chicago', 'America/Chicago', 0.50);
```
- `morning_baseline`: vary between 0.40-0.60 (no real computation yet)
- No autobiography for baseline days (NULL)

## Important Notes
- Be CONSISTENT with topic strings — use exactly the vocabulary above
- Entity IDs must match the IDs in the existing seed data (person_demo_maya, etc.)
- Don't create new entity IDs — only use the ones listed above
- Vary the day slightly — not every Tuesday is identical. Some days she WFH, some lunches are solo, some evenings are quiet reading vs social.
- The onboarding project ramps up in weeks 8-12 (mid-January onwards). Before that, work topics are more generic ("design", "work").
- Rachel appears ONLY on: ~Jan 8 (first contact), ~Jan 25 (second showing), Feb 13 (the main day). House-hunting topics only appear on/near those dates.
- Jess game night is most Fridays but not all — skip ~2 of the 12 Fridays.
- Mom call is weekly but sometimes Saturday instead of Friday.
