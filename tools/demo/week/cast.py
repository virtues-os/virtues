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

ME = {"name": "Oriel Akerlund", "first": "Oriel", "email": "oriel@example.com", "phone": "+15125550199"}

def p(pid, name, phone, email, rel):
    return {"id": pid, "name": name, "phone": phone, "email": email, "rel": rel}

PEOPLE = {k: p(*v) for k, v in {
    "ylva": ("p3y_ylva", "Ylva Lindgren", "+15125552084", "ylva@example.com", "partner"),
    "tove": ("person_demo_maya", "Tove Hallberg", "+15125552000", "tove@example.com", "former Saltbox design lead, close friend"),
    "chidi": ("person_demo_david", "Chidi Okoro", "+15125552007", "chidi@example.com", "old Chicago friend"),
    "aiko": ("person_demo_priya", "Aiko Fujimoto", "+15125552028", "aiko@example.com", "collaborator, ships fast"),
    "ingeborg": ("person_demo_mom", "Ingeborg", "+15125552035", "ingeborg@example.com", "mom, Sunday calls"),
    "agneta": ("person_demo_jess", "Agneta Norlin", "+15125552021", "agneta@example.com", "hosts the Thursday table"),
    "marit": ("person_demo_rachel", "Marit Novotny", "+15125552014", "marit@example.com", "realtor"),
    "zora": ("p3y_zora", "Zora Dvorak", "+15125552182", "zora@example.com", "Ylva's oldest friend, cooks"),
    "femke": ("p3y_femke", "Femke Wiersma", "+15125552098", "femke@example.com", "friend via Ylva"),
    "rune": ("p3y_rune", "Rune Kowalczyk", "+15125552273", "rune@example.com", "Austin friend"),
    "tam": ("p3y_tam", "Haruto Yamada", "+15125552161", "tam@example.com", "neighbor two doors down"),
    "ifeanyi": ("p3y_ifeanyi", "Ifeanyi Chukwu", "+15125552119", "ifeanyi@example.com", "Sunday basketball"),
    "sam_o": ("p3y_thijs", "Thijs Veenstra", "+15125552070", "thijs@example.com", "cycling friend"),
    "sten": ("p3y_sten", "Sten Corrigan", "+15125552140", "sten@example.com", "brother-in-law"),
    "briony": ("p3y_briony", "Briony Madigan", "+15125552210", "briony@example.com", "cousin"),
    "arvid": ("p3y_arvid", "Arvid Rafferty", "+15125552091", "arvid@example.com", "uncle, odd hours"),
    "ray": ("p3y_ray", "Joost Rietveld", "+15125552259", "ray@example.com", "uncle's brother"),
    "birdie": ("p3y_honora", "Honora Prendergast", "+15125552168", "birdie@example.com", "grandmother"),
    "corentin": ("wk_corentin", "Corentin Akerlund", "+15125552301", "corentin@example.com", "younger sibling, in Bemidji"),
    "mateo": ("wk_mateo", "Mateo Corrigan", "+15125552302", "mateo@example.com", "nephew, 16"),
    "yetunde": ("p3y_yetunde", "Yetunde Adeyemi", "+15125552147", "yetunde@example.com", "runs the beta thread"),
    "hina": ("p3y_hina", "Hina Morikawa", "+15125552245", "hina@example.com", "beta tester, files real bugs"),
    "wren": ("p3y_wren", "Romilly Ketelaar", "+15125552196", "wren@example.com", "writes the docs"),
    "henrik": ("p3y_henrik", "Henrik Ostrom", "+15125552224", "henrik@example.com", "hardware, firmware"),
    "imogen": ("p3y_imogen", "Imogen Scanlon", "+15125552266", "imogen@example.com", "illustrator"),
    "rue": ("p3y_rue", "Vesna Kral", "+15125552126", "rue@example.com", "design contractor"),
    "tamsin": ("p3y_tamsin", "Tamsin Donnelly", "+15125552133", "tamsin@example.com", "Denver friend, hosting this weekend"),
    "willem": ("p3y_willem", "Willem Haverkamp", "+15125552042", "willem@example.com", "ex-roommate, flying into Denver too"),
    "signe": ("p3y_signe", "Signe Halvorsen", "+15125552252", "signe@example.com", "disc golf partner, also in Denver"),
    "tunde": ("p3y_tunde", "Tunde Afolabi", "+15125552105", "tunde@example.com", "client turned friend"),
    "sanne": ("p3y_sanne", "Sanne Terpstra", "+15125552049", "sanne@example.com", "Chicago disc golf crew"),
    "hector": ("p3y_hector", "Ruben Fiala", "+15125552238", "hector@example.com", "bike shop"),
    "osric": ("p3y_osric", "Osric Hennigan", "+15125552203", "osric@example.com", "neighbour, loud truck, good tomatoes"),
    "ottilie": ("p3y_ottilie", "Ottilie Fennessy", "+15125552231", "ottilie@example.com", "book club"),
    "daan": ("p3y_daan", "Daan Achterberg", "+15125552056", "daan@example.com", "old agency creative director"),
    "maaike": ("p3y_maaike", "Maaike Achterberg", "+15125552287", "maaike@example.com", "Daan's sister"),
    "gijs": ("p3y_gijs", "Gijs Postma", "+15125552189", "gijs@example.com", "plumber"),
    "mirela": ("p3y_mirela", "Mirela Benes", "+15125552112", "mirela@example.com", "dentist's office"),
    "sam_w": ("p3y_maarten", "Maarten Schipper", "+15125552077", "maarten@example.com", "accountant"),
    "oskar": ("p3y_oskar", "Oskar Moravec", "+15125552154", "oskar@example.com", "hardware supplier, Brno"),
    "odalys": ("p3y_odalys", "Odalys Prochazka", "+15125552063", "odalys@example.com", "old agency account lead"),
}.items()}

# Threads, by rank of real volume. Group sizes match the real "others" counts.
THREADS = {
    "T01": ("Ylva", ["ylva"]),
    "T02": ("Thursday table", ["agneta", "zora", "femke", "rune", "tam", "ifeanyi", "sam_o"]),
    "T03": ("Akerlund family", ["ingeborg", "corentin", "sten", "briony", "arvid", "ray", "mateo", "birdie"]),
    "T04": ("Virtues beta", ["yetunde", "hina", "wren", "henrik", "imogen", "rue"]),
    "T05": ("Tove", ["tove"]),
    "T06": ("Chidi", ["chidi"]),
    "T07": ("Aiko", ["aiko"]),
    "T08": ("Denver weekend", ["tamsin", "willem", "signe"]),
    "T09": ("Mom", ["ingeborg"]),
    "T10": ("Short codes", ["svc"]),
    "T11": ("Ifeanyi", ["ifeanyi"]),
    "T12": ("Zora", ["zora"]),
    "T13": ("Saturday ride", ["sam_o", "hector", "rune", "osric", "ottilie"]),
    "T14": ("Haruto", ["tam"]),
    "T15": ("Tamsin", ["tamsin"]),
    "T16": ("Tunde", ["tunde"]),
    "T17": ("Andersonville disc golf", ["sanne", "willem", "signe", "daan", "maaike"]),
    "T18": ("Femke", ["femke"]),
    "T19": ("Willem", ["willem"]),
    "T20": ("Rune", ["rune"]),
    "T21": ("Ylva + Zora", ["ylva", "zora"]),
}
tail = ["daan", "hector", "gijs", "marit", "maaike", "ottilie", "osric", "mirela", "sten", "briony", "arvid", "birdie",
        "sanne", "odalys", "hina", "imogen", "rue", "sam_w", "signe", "corentin"]
for i, t in enumerate(s["threads"][21:]):
    who = tail[i % len(tail)]
    THREADS[t["tok"]] = (PEOPLE[who]["name"].split()[0], [who])

SERVICES_SMS = [("United", "26266"), ("H-E-B Curbside", "43212"), ("Ironside Barbell", "+15125552900"),
                ("Larkspur Dental", "+15125552112"), ("Third Coast CU", "72166"), ("USPS", "28777"),
                ("Lyft", "46080"), ("Austin Energy", "28932"), ("Fetchly", "76837"), ("CVS Pharmacy", "28796")]

# Email senders. Personal (CATEGORY_PERSONAL) -> cast; the rest -> fictional services.
EMAIL_PEOPLE = ["aiko", "henrik", "oskar", "yetunde", "tove", "wren", "sam_w", "tunde", "tamsin", "marit",
                "hina", "rue", "imogen", "odalys", "daan", "chidi", "willem", "ingeborg"]
EMAIL_SERVICES = [
    ("Figma", "no-reply@figma.example.com", "updates"), ("GitHub", "notifications@github.example.com", "updates"),
    ("Linear", "notifications@linear.example.com", "updates"), ("Third Coast Credit Union", "alerts@thirdcoastcu.example.com", "updates"),
    ("United Airlines", "receipts@united.example.com", "updates"), ("Sidebar", "hello@sidebar.example.com", "promotions"),
    ("Dense Discovery", "issue@densediscovery.example.com", "updates"), ("Are.na", "digest@are.na.example.com", "updates"),
    ("Lumen Cloud Storage", "billing@lumen.example.com", "updates"), ("Vercel", "notifications@vercel.example.com", "updates"),
    ("Stripe", "receipts@stripe.example.com", "updates"), ("Google Calendar", "calendar@google.example.com", "updates"),
    ("The Austin Chronicle", "newsletter@austinchronicle.example.com", "promotions"), ("Ironside Barbell", "frontdesk@ironsidebarbell.example.com", "updates"),
    ("H-E-B", "orders@heb.example.com", "updates"), ("Bell & Marrow Books", "events@bellandmarrow.example.com", "promotions"),
    ("Lyft", "receipts@lyft.example.com", "updates"), ("Austin Energy", "billing@austinenergy.example.com", "updates"),
    ("Readymag", "news@readymag.example.com", "promotions"), ("It's Nice That", "newsletter@itsnicethat.example.com", "promotions"),
    ("Typewolf", "jeremiah@typewolf.example.com", "promotions"), ("Larkspur Dental", "reminders@hydeparkdental.example.com", "updates"),
    ("Notion", "team@notion.example.com", "updates"), ("Slack", "feedback@slack.example.com", "updates"),
    ("Apple", "no_reply@apple.example.com", "updates"), ("Patagonia", "news@patagonia.example.com", "promotions"),
    ("Kickstarter", "no-reply@kickstarter.example.com", "promotions"), ("Substack", "noreply@substack.example.com", "updates"),
    ("Denver Union Station", "hello@unionstationdenver.example.com", "promotions"), ("Airbnb", "automated@airbnb.example.com", "updates"),
    ("Fetchly", "updates@rover.example.com", "updates"), ("TestFlight", "no_reply@testflight.example.com", "updates"),
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
