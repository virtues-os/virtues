"""Casting: real tokens -> the demo persona's people, services, apps, domains.

Everything on the right-hand side is fiction (the demo3y cast, @example.com,
+1512555xxxx) or a public product name. Ranks come from shape.json; no real
name, number, address or domain is written to fiction/.
"""
import json, os, random
from collections import Counter, defaultdict

from work import SRC
R = random.Random(7)
s = json.load(open(f"{SRC}/shape.json"))

ME = {"name": "Jules Arden", "first": "Jules", "email": "jules@example.com", "phone": "+15125550199"}

def p(pid, name, phone, email, rel):
    return {"id": pid, "name": name, "phone": phone, "email": email, "rel": rel}

PEOPLE = {k: p(*v) for k, v in {
    "bea": ("p3y_bea", "Bea Lindqvist", "+15125552084", "bea@example.com", "partner"),
    "mara": ("person_demo_maya", "Mara Vance", "+15125552000", "mara@example.com", "former Canopy design lead, close friend"),
    "david": ("person_demo_david", "David Okafor", "+15125552007", "david@example.com", "old Chicago friend"),
    "priya": ("person_demo_priya", "Priya Haddad", "+15125552028", "priya@example.com", "collaborator, ships fast"),
    "rosa": ("person_demo_mom", "Rosa", "+15125552035", "rosa@example.com", "mom, Sunday calls"),
    "nell": ("person_demo_jess", "Nell Kovac", "+15125552021", "nell@example.com", "hosts the Thursday table"),
    "cora": ("person_demo_rachel", "Cora Delgado", "+15125552014", "cora@example.com", "realtor"),
    "sofia": ("p3y_sofia", "Sofia Marchetti", "+15125552182", "sofia@example.com", "Bea's oldest friend, cooks"),
    "ines": ("p3y_ines", "Ines Moreau", "+15125552098", "ines@example.com", "friend via Bea"),
    "stef": ("p3y_stef", "Stef Nowak", "+15125552273", "stef@example.com", "Austin friend"),
    "tam": ("p3y_tam", "Tam Nguyen", "+15125552161", "tam@example.com", "neighbor two doors down"),
    "kofi": ("p3y_kofi", "Kofi Mensah", "+15125552119", "kofi@example.com", "Sunday basketball"),
    "sam_o": ("p3y_sam_ortiz", "Sam Ortiz", "+15125552070", "sam@example.com", "cycling friend"),
    "gus": ("p3y_gus", "Gus Pettit", "+15125552140", "gus@example.com", "brother-in-law"),
    "maeve": ("p3y_maeve", "Maeve Sullivan", "+15125552210", "maeve@example.com", "cousin"),
    "hal": ("p3y_hal", "Hal Vitorino", "+15125552091", "hal@example.com", "uncle, odd hours"),
    "ray": ("p3y_ray", "Ray Okonjo", "+15125552259", "ray@example.com", "uncle's brother"),
    "birdie": ("p3y_bird", "Birdie Lowell", "+15125552168", "birdie@example.com", "grandmother"),
    "lou": ("wk_lou", "Lou Arden", "+15125552301", "lou@example.com", "younger sibling, in Rockford"),
    "mateo": ("wk_mateo", "Mateo Pettit", "+15125552302", "mateo@example.com", "nephew, 16"),
    "nadia": ("p3y_nadia", "Nadia Saleh", "+15125552147", "nadia@example.com", "runs the beta thread"),
    "lin": ("p3y_lin", "Lin Ashworth", "+15125552245", "lin@example.com", "beta tester, files real bugs"),
    "wren": ("p3y_wren", "Wren Calloway", "+15125552196", "wren@example.com", "writes the docs"),
    "tobias": ("p3y_tobias", "Tobias Renn", "+15125552224", "tobias@example.com", "hardware, firmware"),
    "juno": ("p3y_juno", "Juno Ellery", "+15125552266", "juno@example.com", "illustrator"),
    "rue": ("p3y_rue", "Rue Sandoval", "+15125552126", "rue@example.com", "design contractor"),
    "orla": ("p3y_orla", "Orla Byrne", "+15125552133", "orla@example.com", "Denver friend, hosting this weekend"),
    "theo": ("p3y_theo", "Theo Brandt", "+15125552042", "theo@example.com", "ex-roommate, flying into Denver too"),
    "greta": ("p3y_greta", "Greta Voss", "+15125552252", "greta@example.com", "climbing partner, also in Denver"),
    "desmond": ("p3y_desmond", "Desmond Achebe", "+15125552105", "desmond@example.com", "client turned friend"),
    "junie": ("p3y_junie", "Junie Ferrar", "+15125552049", "junie@example.com", "Chicago climbing crew"),
    "hector": ("p3y_hector", "Hector Salas", "+15125552238", "hector@example.com", "bike shop"),
    "otis": ("p3y_otis", "Otis Bramble", "+15125552203", "otis@example.com", "neighbour, loud truck, good tomatoes"),
    "pia": ("p3y_pia", "Pia Halloran", "+15125552231", "pia@example.com", "book club"),
    "walt": ("p3y_walt", "Walt Osei", "+15125552056", "walt@example.com", "old agency creative director"),
    "nyla": ("p3y_nyla", "Nyla Osei", "+15125552287", "nyla@example.com", "Walt's sister"),
    "abe": ("p3y_abe", "Abe Ferreira", "+15125552189", "abe@example.com", "plumber"),
    "lucia": ("p3y_lucia", "Lucia Fenn", "+15125552112", "lucia@example.com", "dentist's office"),
    "sam_w": ("p3y_sam_whitlock", "Sam Whitlock", "+15125552077", "samw@example.com", "accountant"),
    "emil": ("p3y_emil", "Emil Kovács", "+15125552154", "emil@example.com", "hardware supplier, Prague"),
    "marisol": ("p3y_marisol", "Marisol Reyes", "+15125552063", "marisol@example.com", "old agency account lead"),
}.items()}

# Threads, by rank of real volume. Group sizes match the real "others" counts.
THREADS = {
    "T01": ("Bea", ["bea"]),
    "T02": ("Thursday table", ["nell", "sofia", "ines", "stef", "tam", "kofi", "sam_o"]),
    "T03": ("Arden family", ["rosa", "lou", "gus", "maeve", "hal", "ray", "mateo", "birdie"]),
    "T04": ("Virtues beta", ["nadia", "lin", "wren", "tobias", "juno", "rue"]),
    "T05": ("Mara", ["mara"]),
    "T06": ("David", ["david"]),
    "T07": ("Priya", ["priya"]),
    "T08": ("Denver weekend", ["orla", "theo", "greta"]),
    "T09": ("Mom", ["rosa"]),
    "T10": ("Short codes", ["svc"]),
    "T11": ("Kofi", ["kofi"]),
    "T12": ("Sofia", ["sofia"]),
    "T13": ("Saturday ride", ["sam_o", "hector", "stef", "otis", "pia"]),
    "T14": ("Tam", ["tam"]),
    "T15": ("Orla", ["orla"]),
    "T16": ("Desmond", ["desmond"]),
    "T17": ("Logan Sq climbing", ["junie", "theo", "greta", "walt", "nyla"]),
    "T18": ("Ines", ["ines"]),
    "T19": ("Theo", ["theo"]),
    "T20": ("Stef", ["stef"]),
    "T21": ("Bea + Sofia", ["bea", "sofia"]),
}
tail = ["walt", "hector", "abe", "cora", "nyla", "pia", "otis", "lucia", "gus", "maeve", "hal", "birdie",
        "junie", "marisol", "lin", "juno", "rue", "sam_w", "greta", "lou"]
for i, t in enumerate(s["threads"][21:]):
    who = tail[i % len(tail)]
    THREADS[t["tok"]] = (PEOPLE[who]["name"].split()[0], [who])

SERVICES_SMS = [("United", "26266"), ("H-E-B Curbside", "43212"), ("Eastside Barbell", "+15125552900"),
                ("Hyde Park Dental", "+15125552112"), ("Third Coast CU", "72166"), ("USPS", "28777"),
                ("Lyft", "46080"), ("Austin Energy", "28932"), ("Rover", "76837"), ("CVS Pharmacy", "28796")]

# Email senders. Personal (CATEGORY_PERSONAL) -> cast; the rest -> fictional services.
EMAIL_PEOPLE = ["priya", "tobias", "emil", "nadia", "mara", "wren", "sam_w", "desmond", "orla", "cora",
                "lin", "rue", "juno", "marisol", "walt", "david", "theo", "rosa"]
EMAIL_SERVICES = [
    ("Figma", "no-reply@figma.example.com", "updates"), ("GitHub", "notifications@github.example.com", "updates"),
    ("Linear", "notifications@linear.example.com", "updates"), ("Third Coast Credit Union", "alerts@thirdcoastcu.example.com", "updates"),
    ("United Airlines", "receipts@united.example.com", "updates"), ("Sidebar", "hello@sidebar.example.com", "promotions"),
    ("Dense Discovery", "issue@densediscovery.example.com", "updates"), ("Are.na", "digest@are.na.example.com", "updates"),
    ("Lumen Cloud Storage", "billing@lumen.example.com", "updates"), ("Vercel", "notifications@vercel.example.com", "updates"),
    ("Stripe", "receipts@stripe.example.com", "updates"), ("Google Calendar", "calendar@google.example.com", "updates"),
    ("The Austin Chronicle", "newsletter@austinchronicle.example.com", "promotions"), ("Eastside Barbell", "frontdesk@eastsidebarbell.example.com", "updates"),
    ("H-E-B", "orders@heb.example.com", "updates"), ("Bell & Marrow Books", "events@bellandmarrow.example.com", "promotions"),
    ("Lyft", "receipts@lyft.example.com", "updates"), ("Austin Energy", "billing@austinenergy.example.com", "updates"),
    ("Readymag", "news@readymag.example.com", "promotions"), ("It's Nice That", "newsletter@itsnicethat.example.com", "promotions"),
    ("Typewolf", "jeremiah@typewolf.example.com", "promotions"), ("Hyde Park Dental", "reminders@hydeparkdental.example.com", "updates"),
    ("Notion", "team@notion.example.com", "updates"), ("Slack", "feedback@slack.example.com", "updates"),
    ("Apple", "no_reply@apple.example.com", "updates"), ("Patagonia", "news@patagonia.example.com", "promotions"),
    ("Kickstarter", "no-reply@kickstarter.example.com", "promotions"), ("Substack", "noreply@substack.example.com", "updates"),
    ("Denver Union Station", "hello@unionstationdenver.example.com", "promotions"), ("Airbnb", "automated@airbnb.example.com", "updates"),
    ("Rover", "updates@rover.example.com", "updates"), ("TestFlight", "no_reply@testflight.example.com", "updates"),
]

APP_MAP = {"Dia": "Dia", "Claude": "Claude", "Messages": "Messages", "Virtues": "Virtues", "Slack": "Slack",
           "Preview": "Preview", "Podcasts": "Podcasts", "Spotify": "Spotify", "ChatGPT": "Figma",
           "FaceTime": "FaceTime", "Finder": "Finder", "Photos": "Photos", "Phone": "Phone", "Flash Studio": "Figma",
           "Captive Network Assistant": "Captive Network Assistant", "System Settings": "System Settings",
           "Find My": "Find My", "Terminal": "Terminal", "Maps": "Maps", "TextEdit": "TextEdit",
           "QuickTime Player": "QuickTime Player", "Simulator": "Xcode", "Logic Pro": "Notes", "Gemini": "Notes",
           "Grok Bot": "Notes", "Transporter": "Xcode", "Postgres": "Terminal", "Pass Viewer": "Wallet"}
DROP_APPS = {"UserNotificationCenter", "universalAccessAuthWarn", "PostgresMenuHelper"}
BUNDLES = {"Dia": "company.thebrowser.dia", "Claude": "com.anthropic.claudefordesktop", "Messages": "com.apple.MobileSMS",
           "Virtues": "com.virtues.app", "Slack": "com.tinyspeck.slackmacgap", "Preview": "com.apple.Preview",
           "Podcasts": "com.apple.podcasts", "Spotify": "com.spotify.client", "Figma": "com.figma.Desktop",
           "FaceTime": "com.apple.FaceTime", "Finder": "com.apple.finder", "Photos": "com.apple.Photos",
           "Phone": "com.apple.mobilephone", "Captive Network Assistant": "com.apple.CaptiveNetworkAssistant",
           "System Settings": "com.apple.systempreferences", "Find My": "com.apple.findmy", "Terminal": "com.apple.Terminal",
           "Maps": "com.apple.Maps", "TextEdit": "com.apple.TextEdit", "QuickTime Player": "com.apple.QuickTimePlayerX",
           "Xcode": "com.apple.dt.Xcode", "Notes": "com.apple.Notes", "Wallet": "com.apple.Passbook"}

DOMAINS = ["mail.google.com", "github.com", "figma.com", "claude.ai", "x.com", "google.com", "linear.app",
           "docs.google.com", "youtube.com", "are.na", "calendar.google.com", "news.ycombinator.com", "vercel.com",
           "notion.so", "cosmos.so", "reddit.com", "en.wikipedia.org", "maps.google.com", "fonts.google.com",
           "savee.it", "united.com", "nytimes.com", "theverge.com", "fontsinuse.com", "stripe.com", "dribbble.com",
           "itsnicethat.com", "typewolf.com", "developer.apple.com", "stackoverflow.com", "readymag.com",
           "letterboxd.com", "goodreads.com", "austinchronicle.com", "weather.gov", "airbnb.com", "spotify.com",
           "amazon.com", "instagram.com", "pinterest.com", "mobbin.com", "lapa.ninja", "rauno.me", "brutalist.report",
           "kottke.org", "unionstationindenver.com", "rtd-denver.com", "heb.com", "bellandmarrow.com"]
dom_rank = [d["domain"] for d in s["domains"]]
DOMAIN_MAP = {}
for i, d in enumerate(dom_rank):
    DOMAIN_MAP[d] = DOMAINS[i] if i < len(DOMAINS) else R.choice(DOMAINS[:30])

json.dump({"me": ME, "people": PEOPLE, "threads": THREADS, "sms_services": SERVICES_SMS,
           "email_people": EMAIL_PEOPLE, "email_services": EMAIL_SERVICES, "app_map": APP_MAP,
           "drop_apps": sorted(DROP_APPS), "bundles": BUNDLES, "domain_map": DOMAIN_MAP},
          open(f"{SRC}/cast_private.json", "w"))
# The fiction-only view (no real domain keys) for prompts and emit.
json.dump({"me": ME, "people": PEOPLE, "threads": THREADS, "sms_services": SERVICES_SMS,
           "email_people": EMAIL_PEOPLE, "email_services": EMAIL_SERVICES, "app_map": APP_MAP,
           "drop_apps": sorted(DROP_APPS), "bundles": BUNDLES},
          open(f"{SRC}/fiction/cast.json", "w"))
print("threads cast", len(THREADS), "domains mapped", len(DOMAIN_MAP))
