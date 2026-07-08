# Location Probe — the one spike that de-risks the Tauri mobile pivot

**Question being answered:** does iOS run our native background location collection
_through Tauri_ — writing rows to SQLite while the webview is suspended and after
the OS has terminated and cold-relaunched the app, with no Mac attached?

If this passes, every other collector is mechanical and we can collapse the Swift
app into a Tauri-wrapped `apps/web` for iOS + Android. If it fails, we learn it
here in ~50 lines instead of after porting the whole app.

## What got built

| Piece | Path |
|---|---|
| Native collector (CLLocationManager + SQLite, singleton) | [plugins/location-probe/ios/Sources/LocationProbe.swift](plugins/location-probe/ios/Sources/LocationProbe.swift) |
| Tauri plugin class (JS ↔ native bridge) | [plugins/location-probe/ios/Sources/LocationProbePlugin.swift](plugins/location-probe/ios/Sources/LocationProbePlugin.swift) |
| Rust command layer | [plugins/location-probe/src/](plugins/location-probe/src/) |
| **Early-install hook** (the thing under test) | [src-tauri/src/lib.rs](src-tauri/src/lib.rs) — `setup()` calls `start_probe()` |
| Viewer UI | [src-tauri/ui/probe.html](src-tauri/ui/probe.html) |
| iOS background keys | `src-tauri/gen/apple/project.yml` `info.properties` + [src-tauri/Info.ios.plist](src-tauri/Info.ios.plist) |

### Why the hook lives in Rust `setup()` (not an AppDelegate)

Tauri's generated iOS entry is `gen/apple/Sources/virtues/main.mm`, which is just
`ffi::start_app()` → the Rust `run()`. There is **no Swift AppDelegate to patch**.
The earliest code Tauri runs on _every_ launch — including a cold background
relaunch by the OS — is the Builder `setup()` closure. So that is where we install
the `CLLocationManager` delegate. The Swift side checks whether the process came up
straight into `.background` and tags those rows `launchReason = background-launch`.

## Build & install for the test (physical iPhone required)

A simulator cannot be rebooted-into-background or fed a real significant-location
change, so the real test needs a device. The app must be a **standalone release
build** (frontend bundled) — a dev build loads the frontend from the Mac's dev
server, which is gone after a reboot.

```bash
cd apps/web
pnpm exec tauri ios build --open        # release build, then opens Xcode
```

In Xcode:
1. Target **virtues_iOS → Signing & Capabilities** → select your **Team** (a free
   personal team is fine). Bundle id `com.virtues.desktop` is fine for dev, or change it.
2. Scheme build configuration = **Release** (Product → Scheme → Edit Scheme → Run →
   Build Configuration → Release).
3. Select your connected **iPhone** as the run destination → **Run (⌘R)**.
4. First launch: trust the dev cert on the phone (Settings → General → VPN & Device
   Management → your Apple ID → Trust).

> Do **not** use `tauri ios dev` for the reboot test — it needs the Mac's dev server
> alive, which defeats the whole point. `dev` is only for iterating on the UI.

## The test (this is the actual protocol)

1. Launch the app. Tap **Start background collection**. Grant **Always** (if it only
   offers "While Using", grant that, then Settings → Virtues → Location → **Always**).
   Also confirm **Background App Refresh** is ON.
   → You should see a `start` row, and if you move, `update` rows with state `active`.
2. Background the app — swipe to the home screen. **Do NOT swipe it away in the app
   switcher** (that is a user force-quit; iOS will never relaunch it — expected, not a bug).
3. **Power the iPhone fully off, then on.** After it boots, **do not open the app.**
   (Reboot is the deterministic way to terminate-without-force-quit.)
4. **Move ~500 m** — drive or walk a few blocks. Significant-location-change needs
   real distance + a cell-tower change; pacing your room will fire nothing.
5. Reopen the app and read the table.

### PASS

Rows exist that were written while you were not in the app. Specifically:
- the green **cold relaunch** counter > 0 (rows tagged `background-launch`), and/or
- rows timestamped **after the reboot** and **before** you reopened, with state
  `background`.

That proves: OS cold-relaunched the Tauri app into the background → Rust `setup()`
ran with no webview → the delegate installed → SQLite got written. Pivot de-risked.

### Also useful

- **Live logs:** Console.app (or Xcode console), filter process = `Virtues`, search
  `[LocationProbe]` — you'll see writes happen in real time, including while backgrounded.
- **Raw DB:** Xcode → Window → Devices & Simulators → your device → Virtues →
  "Download Container" → the SQLite is at `AppData/Library/Application Support/
  location_probe.sqlite`, table `rows`.

## Interpreting a failure

| Symptom | Meaning / next step |
|---|---|
| `active` rows only, nothing while backgrounded | Level-1 fail — check Always auth + `allowsBackgroundLocationUpdates`; likely a permission issue, not Tauri. |
| Background rows while app stayed alive, but **nothing after reboot** | The load-bearing risk: Tauri `setup()` isn't reached on cold relaunch. Next step: a real `UIApplicationDelegate` hook (register one via the Xcode project / `application(_:didFinishLaunchingWithOptions:)`), which `main.mm` currently doesn't provide. |
| No rows at all | `start_probe()` didn't run — check the `[location-probe] start failed` log in Xcode console. |

## Known sharp edge (from the research)

`tauri ios build`/`dev` regenerate `project.yml` → `Info.plist` via xcodegen and may
drop the background keys. **Before each device test, verify:**

```bash
/usr/libexec/PlistBuddy -c "Print :UIBackgroundModes" \
  apps/web/src-tauri/gen/apple/virtues_iOS/Info.plist
```

Must list `location`, `processing`, `fetch`. If missing, they're in
`project.yml` `info.properties` and `Info.ios.plist`; re-run `xcodegen generate`
inside `gen/apple`, or re-add. Without these keys, `CLLocationManager` background
updates silently do nothing.
