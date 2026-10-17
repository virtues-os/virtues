"""Where the week pipeline keeps its working files: OUTSIDE the repo.

shape.json, cast_private.json and the model cache hold real timing and real
domains from the source record, so they live in $DEMO_WEEK_WORK (default
~/virtues-demo-src) and never in git. Only fiction leaves it, as week.sql.gz.
"""
import os
SRC = os.environ.get("DEMO_WEEK_WORK", os.path.expanduser("~/virtues-demo-src"))
os.makedirs(f"{SRC}/fiction/content", exist_ok=True)
