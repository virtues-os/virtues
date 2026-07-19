# iOS App - Production Documentation

## Overview

The Virtues iOS app has a single purpose: **Reliable raw data collection**.

### Architecture

- **Direct SQLite writes** - No in-memory buffers, SQLite is the single queue
- **Six data streams**: location, audio (microphone), healthkit, contacts, eventkit, financekit
- **One backend action**: every stream POSTs to `/webhook/{action_id}` for the single `ios_ingest` action; the body's `stream` field tells the backend which ontology to fan the records into
- **Batched uploads** - Groups SQLite entries by stream type (one request per stream that has pending data)
- **15-minute upload cadence** (battery); HealthKit *collects* every 5 min. Data is
  durable in SQLite between cycles. NOTE: iOS's ~30s background budget means an
  upload started in the background may not finish — orphaned `uploading` rows are
  reset to `pending` at the start of the next cycle (see SQLiteManager)
- **Dependency injection** - All managers are unit testable
- **Centralized health monitoring** - Automated recovery from failures
- **Generic stream processing** - Extensible architecture for new data types

### Core Principles

- Raw data only - no on-device processing
- Background resilience is paramount
- User privacy and control
- Simple, reliable architecture
- **Production-ready code quality** - January 2025 comprehensive refactoring

## Background Resilience

The iOS app is designed to maintain continuous data collection even through system interruptions:

### Audio Interruption Handling

- **Phone Calls**: Automatically pauses and resumes recording after calls end
- **System Interruptions**: Handles Siri, alarms, and other audio interruptions
- **Recovery Mechanism**: Falls back to full restart if resume fails
- **Foreground Recovery**: Checks and restarts recording when app returns to foreground

### Background Execution Strategy

- **DispatchSourceTimer**: More reliable than Timer for background execution
- **Audio Session Configuration**: Uses `.mixWithOthers` for continuous background recording
- **Background Tasks**: Registered for `fetch` and `processing` modes
- **Keepalive Tasks**: Background task wrappers around critical operations

### Timer Synchronization

- **Aligned Intervals**: HealthKit (5 min) synced with upload timer (5 min)
- **Prevents Empty Uploads**: Ensures data is collected before sync attempts
- **Reliable Scheduling**: ReliableTimer (DispatchSourceTimer wrapper) survives background transitions
- **Thread-Safe**: NSLock protection prevents race conditions
- **Centralized**: Single HealthCheckCoordinator manages all manager health

## Modern Architecture (January 2025 Refactoring)

### Overview

The iOS app underwent a comprehensive architectural refactoring in January 2025 to improve reliability, testability, and maintainability.

### Key Improvements

#### 1. ReliableTimer Infrastructure

**Problem Solved**: Mixed timer implementations (`Timer.scheduledTimer()` + `DispatchSourceTimer`) caused unreliable background execution.

**Solution**: Unified `ReliableTimer` class:

- Wraps `DispatchSourceTimer` for consistent background reliability
- Thread-safe with `NSLock` protection
- Builder pattern for easy configuration
- Automatic weak self capture support
- Used by: AudioManager, LocationManager, HealthKitManager, BatchUploadCoordinator

**Location**: `Utilities/ReliableTimer.swift`

**Benefits**:

- ✅ 100% background reliability (no more failed timers)
- ✅ Zero race conditions in timer cleanup
- ✅ Consistent API across all managers

#### 2. Unified Error Handling & Retry Logic

**Problem Solved**: Silent data loss when encoding or storage failed. No visibility into errors.

**Solution**: Comprehensive error handling system:

- `DataCollectionError` protocol with 5 error types:
  - `DataEncodingError` - Data serialization failures
  - `StorageError` - SQLite write failures
  - `PermissionError` - Authorization issues
  - `CollectionError` - System API failures
  - `ConfigurationError` - Setup problems
- `ErrorLogger` - Centralized error tracking with statistics
- 3-attempt retry with exponential backoff (0.5s, 1.0s, 1.5s)
- Structured error context for debugging

**Location**:

- `Core/ErrorHandling/DataCollectionError.swift`
- `Core/ErrorHandling/ErrorLogger.swift`

**Benefits**:

- ✅ Zero silent data loss
- ✅ Automatic recovery from transient failures
- ✅ Error telemetry for debugging production issues
- ✅ Distinguishes recoverable vs non-recoverable errors

#### 3. Dependency Injection

**Problem Solved**: Tight coupling to singletons made unit testing impossible. Hard to mock dependencies.

**Solution**: Protocol-based dependency injection:

- `ConfigurationProvider` - Device configuration access
- `StorageProvider` - SQLite operations
- `DataUploader` - Upload coordination
- All managers accept dependencies via constructor (the single supported initializer)

**Location**: `Core/Protocols/`

**Example**:

```swift
// Old (singleton coupling)
let deviceId = DeviceManager.shared.configuration.deviceId

// New (dependency injection)
let deviceId = configProvider.deviceId

// Manager initialization
AudioManager(
    configProvider: DeviceManager.shared,
    storageProvider: SQLiteManager.shared,
    dataUploader: BatchUploadCoordinator.shared
)
```

**Benefits**:

- ✅ 100% unit testable with mocked dependencies
- ✅ 45+ singleton references eliminated
- ✅ Clearer dependencies and data flow
- ✅ Constructor injection is the single way to create managers

#### 4. Centralized Health Monitoring

**Problem Solved**: Each manager had its own 30-second health check timer (60 checks/minute on main thread). HealthKit had no health monitoring at all.

**Solution**: `HealthCheckCoordinator`:

- Single 30-second timer checks all managers
- Managers implement `HealthCheckable` protocol
- Returns `HealthStatus`: `.healthy`, `.unhealthy(reason)`, or `.disabled`
- Automatic recovery: unhealthy managers restart automatically
- Aggregate health reporting

**Location**: `Core/HealthCheck/`

**Example**:

```swift
// Manager implementation
extension AudioManager: HealthCheckable {
    var healthCheckName: String { "AudioManager" }

    func performHealthCheck() -> HealthStatus {
        guard hasPermission else {
            return .disabled
        }

        if !isRecording {
            startRecording() // Auto-recovery
            return .unhealthy(reason: "Recording stopped, restarting")
        }

        return .healthy
    }
}
```

**Benefits**:

- ✅ 50% reduction in main thread work (60→30 checks/min)
- ✅ 3 separate timers → 1 coordinated timer
- ✅ HealthKit now monitored (was missing!)
- ✅ Unified health status across all managers
- ✅ Automated recovery from failures

#### 5. Generic Stream Processing

**Problem Solved**: BatchUploadCoordinator had 120+ lines of duplicated code for each stream type (HealthKit, Location, Audio). Adding new streams required copy-pasting.

**Solution**: Generic `StreamDataProcessor` protocol:

- Protocol with associated types for type safety
- Factory pattern for stream processor creation
- Single generic upload method handles all stream types
- Concrete implementations: `HealthKitStreamProcessor`, `LocationStreamProcessor`, `AudioStreamProcessor`

**Location**: `Core/Streaming/`

**Example**:

```swift
// Old: 120 lines of duplicated switch cases
switch streamName {
case "ios_healthkit":
    var allMetrics: [HealthKitMetric] = []
    for event in events { ... }
    // 40 lines per case
case "ios_location":
    // Another 40 lines of identical logic
case "ios_mic":
    // Another 40 lines of identical logic
}

// New: Single generic method (30 lines)
let processor = StreamProcessorFactory.processor(for: streamName)
return await uploadWithProcessor(processor: processor, events: events)
```

**Benefits**:

- ✅ 43% code reduction (120→75 lines)
- ✅ Adding new streams: 10-line processor vs 40-line switch case
- ✅ Bug fixes: 1 place vs 3 places
- ✅ Type-safe with Swift generics

### Architecture Statistics

| Metric | Before (2024) | After (2025) | Improvement |
|--------|---------------|--------------|-------------|
| Singleton Dependencies | 45+ | 0 (injectable) | 100% |
| Timer Implementations | 4 different | 1 unified | 75% |
| Health Check Timers | 3 separate | 1 coordinated | 67% |
| Stream Upload Code | 120 lines | 75 lines | 43% |
| Unit Testability | 0% | high (DI in place) | — |
| Background Reliability | ~60% | improved (not 100% — 30s budget still bounds) | — |
| Silent Data Loss | Common | reduced (orphaned-row reset; box dedupes retries) | — |

> Honesty note: earlier revisions of this doc claimed "100% background reliability"
> and "zero silent data loss". Those are aspirations, not guarantees — iOS's ~30s
> background budget can interrupt an upload, and durability rests on the
> orphaned-`uploading` reset + the box's idempotent `ON CONFLICT DO NOTHING` ingest.
> The refactor's real wins are DI, one timer abstraction, and one health coordinator.

### File Structure

```
apps/ios/Virtues/
├── Core/
│   ├── ErrorHandling/
│   │   ├── DataCollectionError.swift      # Error types & protocols
│   │   └── ErrorLogger.swift              # Centralized error tracking
│   ├── HealthCheck/
│   │   ├── HealthCheckable.swift          # Health check protocol
│   │   └── HealthCheckCoordinator.swift   # Centralized monitoring
│   ├── Protocols/
│   │   ├── ConfigurationProvider.swift    # Config dependency injection
│   │   ├── StorageProvider.swift          # SQLite dependency injection
│   │   └── DataUploader.swift             # Upload dependency injection
│   └── Streaming/
│       ├── StreamDataProcessor.swift      # Generic stream protocol
│       └── StreamProcessors.swift         # Concrete implementations
├── Utilities/
│   └── ReliableTimer.swift                # Thread-safe timer wrapper
└── Managers/
    ├── Tracking/
    │   ├── AudioManager.swift             # Refactored with DI
    │   └── LocationManager.swift          # Refactored with DI
    ├── Integration/
    │   └── HealthKitManager.swift         # Refactored with DI
    └── Sync/
        └── BatchUploadCoordinator.swift   # Refactored with DI + generics
```

### Testing Strategy

With dependency injection, all managers are now unit testable:

```swift
// Example: Testing AudioManager
func testAudioManagerSavesData() {
    let mockConfig = MockConfigurationProvider()
    let mockStorage = MockStorageProvider()
    let mockUploader = MockDataUploader()

    let manager = AudioManager(
        configProvider: mockConfig,
        storageProvider: mockStorage,
        dataUploader: mockUploader
    )

    // Test manager behavior with mocked dependencies
    manager.startRecording()
    XCTAssertTrue(mockStorage.enqueueCalled)
}
```

### Migration Notes

**Construct managers via dependency injection** — this is the only supported way:

```swift
let audioManager = AudioManager(
    configProvider: configProvider,
    storageProvider: storageProvider,
    dataUploader: dataUploader
)
```

The app-wide `.shared` instances are themselves built with this initializer (see each manager's `static let shared`). It is the only initializer each manager exposes.

**Health Check Coordinator**: Automatically starts when managers initialize. No manual setup required.

## Data Collection Architecture

### Evolution and Design Decisions

The iOS app evolved from using in-memory buffers to a simpler, more reliable SQLite-based architecture:

1. **Original Design**: In-memory buffers → SQLite → Upload
2. **Current Design**: Direct SQLite writes → Batched Upload

### Key Architectural Changes

#### 1. Removed Signal Processing

- The iOS app collects **streams only** - no signal processing on device
- Signals are computed server-side in the data pipeline
- Removed all `signal_id`, `signalIds`, and `activatedSignals` fields from responses

#### 2. Direct SQLite Writes

- Each manager writes directly to SQLite after collecting data
- No intermediate in-memory buffers
- SQLite serves as the single, persistent buffer

#### 3. Batched Uploads by Stream Type

- Groups all pending SQLite entries by `stream_name`
- Combines record arrays before upload
- Reduces network requests substantially vs per-record uploads (one POST per stream with pending data)

#### 4. Incremental HealthKit Sync

- Uses `HKAnchoredObjectQuery` instead of time-based queries
- Tracks what's been synced with persistent anchors
- Handles Apple Watch sync delays (10-15 minutes)
- No duplicate data - each sample synced exactly once

### Data Flow

```
1. Data Collection (every N seconds/minutes)
   ↓
2. Direct write to SQLite (no buffering)
   ↓
3. Every 5 minutes: Batch by stream type
   ↓
4. Upload batches (one request per stream with pending data) to the ios_ingest webhook
   ↓
5. Mark SQLite entries as complete
```

### Benefits of This Architecture

1. **Simplicity**: Single buffer (SQLite), no complex state management
2. **Reliability**: Data persisted immediately, survives app crashes
3. **Efficiency**: Batching reduces network overhead significantly
4. **Accuracy**: Incremental sync prevents data loss or duplication
5. **Transparency**: UI shows actual queue counts, not buffer sizes

## Production Requirements

- **iOS Version**: 18.0+
- **Storage**: ~500MB available (handles 7-day buffer)
- **Network**: Handles offline/online transitions
- **Battery Impact**: ~10-15% additional drain per day
- **Data Usage**: ~50-100MB/day typical

## Onboarding Flow

The app blocks all data collection until onboarding completes.

### Step 1: Pair with the box

1. In Settings, scan the box's QR code **or** enter its pairing code + address
2. The device generates an iroh keypair; pairing (`/api/pair/consume`) allowlists
   its EndpointId on the box and returns the reach ticket + action-id map
3. Auth is the device's iroh key — there is NO bearer token / API key
4. **Uploads can't complete until paired** (no action-ids → no webhook URL)

### Step 2: Permissions

Request ALL permissions (none are optional):

- **Location Services** - Always (not "While Using App")  
- **Microphone** - For audio recording
- **HealthKit** - All types listed below

If any permission denied:

- Show explanation why it's required
- Button to open Settings app
- Re-check on app return

### Step 3: Initial Sync

1. Capture last 7 days of HealthKit data
   - **Note**: Due to iOS 30-second background limits, large syncs may require multiple attempts
   - Progress indicator shows upload status
   - App must remain in foreground for initial sync to complete reliably
2. Start all background services
3. Begin regular collection

## Data Streams

All six streams share the same upload path: each is queued in SQLite under its
internal key (`ios_location`, `ios_mic`, `ios_healthkit`, `ios_contacts`,
`ios_eventkit`, `ios_finance`), batched, and POSTed to the single `ios_ingest`
webhook with a `stream` field in the body. The three streams below are the
high-volume sensors; contacts/eventkit/financekit follow the same pattern with
lower cadence.

### 1. Location Stream (internal key `ios_location`, body `stream: "location"`)

**Collection**: Every 10 seconds (written directly to SQLite)

**Data captured**:

```json
{
  "timestamp": "2025-01-30T10:00:00.000Z",
  "latitude": 37.7749,
  "longitude": -122.4194,
  "altitude": 10.5,
  "speed": 1.2,
  "horizontal_accuracy": 5.0,
  "vertical_accuracy": 3.0
}
```

**Configuration**:

```swift
locationManager.desiredAccuracy = kCLLocationAccuracyNearestTenMeters
locationManager.allowsBackgroundLocationUpdates = true
locationManager.pausesLocationUpdatesAutomatically = false
```

### 2. Audio Stream (internal key `ios_mic`, body `stream: "microphone"`)

**Collection**: 30-second chunks with 2-second overlap (written directly to SQLite)

**Data format**:

```json
{
  "id": "unique-chunk-id",
  "timestamp_start": "2025-01-30T10:00:00.000Z",
  "timestamp_end": "2025-01-30T10:00:30.000Z",
  "duration": 30.0,
  "audio_data": "base64-encoded-aac-audio",
  "overlap_duration": 2.0
}
```

**Configuration**:

- Sample rate: 16kHz
- Format: AAC compression (16kbps bitrate)
- Compression: ~120KB per 30-second chunk
- Background recording enabled
- Audio route: User selectable (iPhone mic, Bluetooth devices, wired headsets)
- Timer: DispatchSourceTimer for reliable background execution
- Session options: `.mixWithOthers` for continuous recording

**Audio Input Selection**:

The app allows users to select their preferred audio input device:

- **iPhone Microphone**: Uses built-in mic only, prevents Bluetooth device interference
- **Bluetooth Devices**: AirPods, headsets, car audio (when connected)
- **Wired Headsets**: Lightning/USB-C headphones with microphone

To change audio input:

1. Go to Settings > Audio Input
2. Select preferred microphone from the list
3. Selection persists across app restarts
4. Automatically falls back to iPhone mic if selected device disconnects

**Interruption Handling**:

The app automatically handles audio interruptions:

- **Phone Calls**: Recording pauses when call begins, resumes when call ends
- **Siri/Alarms**: Handles system audio interruptions gracefully
- **Recovery**: If resume fails, recording restarts completely
- **Foreground Check**: Recording state verified and restored on app foreground
- **Notification**: Logs interruption events for debugging

### 3. HealthKit Stream (internal key `ios_healthkit`, body `stream: "healthkit"`)

**Collection**: Every 5 minutes using incremental sync (aligned with upload timer)

**Incremental Sync Details**:

- Uses `HKAnchoredObjectQuery` to track only NEW samples
- Queries samples by when they were ADDED to HealthKit, not measurement time
- Stores unique anchor for each health type
- Handles Apple Watch delayed syncs (10-15 minute delays)
- No duplicates - each sample synced exactly once

**HealthKit Types**:

- `HKQuantityTypeIdentifierHeartRate`
- `HKQuantityTypeIdentifierStepCount`
- `HKQuantityTypeIdentifierActiveEnergyBurned`
- `HKQuantityTypeIdentifierHeartRateVariabilitySDNN`
- `HKQuantityTypeIdentifierDistanceWalkingRunning`
- `HKQuantityTypeIdentifierRestingHeartRate`
- `HKCategoryTypeIdentifierSleepAnalysis`

**Data format**:

```json
{
  "timestamp": "2025-01-30T10:00:00.000Z",
  "sample_type": "HKQuantityTypeIdentifierHeartRate",
  "value": 72.0,
  "unit": "bpm"
}
```

**Value Normalization**:

Normalize values before upload to avoid excessive precision:

- **Heart Rate**: Round to whole number (72 bpm)
- **Steps**: Always whole number (1234 steps)
- **Distance**: 2 decimal places (1234.57 m)
- **Active Energy**: 1 decimal place (45.7 kcal)
- **HRV**: 1 decimal place (28.5 ms)
- **Sleep**: Raw category value (0, 1, 2)

### 4. Contacts Stream (internal key `ios_contacts`, body `stream: "contacts"`)

**Collection**: On demand / periodic contact-graph sync. Records are the contact
list (name, emails, phones, organization, birthday). The backend resolves each
contact to a `wiki_people` entity by email then phone.

### 5. EventKit Stream (internal key `ios_eventkit`, body `stream: "eventkit"`)

**Collection**: Calendar events (and reminders). The push sends one wrapper
record with `events[]` / `reminders[]`; the backend writes events to
`data_calendar_event` (reminders are skipped for now).

### 6. FinanceKit Stream (internal key `ios_finance`, body `stream: "financekit"`)

**Collection**: Apple FinanceKit accounts and transactions, sent as one wrapper
record with `accounts[]` / `transactions[]`. The backend writes
`data_financial_account` and `data_financial_transaction` (amounts stored as
integer cents, deterministic UUIDv5 ids for idempotent upserts).

## Upload & Sync

### Sync Strategy

- **Primary**: 5-minute timer (foreground and background)
- **Fallback**: iOS background tasks
- **Manual**: User-triggered sync button

### Payload Structure

Each stream's pending records are batched and POSTed to the single `ios_ingest`
webhook at `/webhook/{action_id}`. The body carries a `stream` field so one
backend action can fan the records to the right ontology table:

```json
{
  "source": "ios",
  "stream": "location",
  "device_id": "uuid",
  "records": [
    // 30 location samples from 5 minutes
    {"timestamp": "...", "latitude": 37.7749, "longitude": -122.4194, ...},
    {"timestamp": "...", "latitude": 37.7750, "longitude": -122.4195, ...}
    // ... 28 more samples
  ],
  "timestamp": "2026-06-23T10:00:00Z",
  "checkpoint": "optional-cursor"
}
```

The `action_id` for `ios_ingest` is returned at pair time in the
`function_name → action_id` map and persisted in the Keychain; it can be
refetched via `GET /api/devices/action-ids`. (Previously each stream had its own
action_id; they were unified into one `ios_ingest` action — the device now posts
every stream to that one URL.)

**Batching Strategy**:

- Groups all pending SQLite entries by stream type
- Combines record arrays before upload
- One POST per stream that has pending data (all to the same `ios_ingest` URL)
- Large reduction in network requests vs per-record uploads

### Network Resilience

- **Timeouts**: 30 seconds per request
- **Retries**: Exponential backoff: 30s → 60s → 120s → 240s → 300s
- **Batch size**: byte-bounded sub-batches (~512KB) per request so a large backlog
  drains incrementally and fits the request/background budget (see BatchUploadCoordinator)
- **Auth**: none at the HTTP layer — the device's allowlisted **iroh key IS the
  credential**. `BoxTransport` dials the box by its EndpointId; there is no bearer token
- **Transport**: all requests ride `BoxTransport` (iroh/QUIC), which reaches the box
  by EndpointId over LAN-direct / hole-punched / relay — works off-LAN, no typed URL

## Sync Monitoring

The app tracks sync health to help diagnose issues:

### Success Tracking

- **Last Upload Attempt**: Timestamp of most recent sync attempt
- **Last Successful Sync**: Only updated after confirmed uploads
- **Stream-Level Success**: Each stream upload tracked independently
- **Batch Return Values**: Upload functions return success boolean

### Monitoring Properties

- `lastUploadDate`: Most recent sync attempt (successful or not)
- `lastSuccessfulSyncDate`: Last time data was actually uploaded
- `uploadStats`: Pending, failed, and total counts
- `streamCounts`: Per-stream queue counts (healthkit, location, audio)

### Success Validation

1. Each stream batch upload returns true/false
2. Only marks `lastSuccessfulSyncDate` if any uploads succeeded
3. Failed uploads increment retry counters
4. Success logs include data size and stream key

## Error Handling

### Error Codes

- `E001` - Network timeout
- `E002` - Invalid API key
- `E003` - Server error (5xx)
- `E004` - Storage full
- `E005` - Permission denied

### SQLite Retry Logic

- Max attempts: 5 per record
- Tracks `upload_attempts` and `last_attempt_date`
- Failed records retained for 3 days

### Storage Management

- Auto-cleanup: Uploaded data after 3 days
- Critical threshold: < 100MB available
- Priority: Keep most recent data
- User notification when < 50MB

## Quick Reference

| Setting | Value | Purpose |
|---------|-------|---------|
| Location interval | 10 seconds | GPS sampling rate |
| Location accuracy | kCLLocationAccuracyNearestTenMeters | Battery vs accuracy balance |
| Audio chunk size | 30 seconds | Transcription segments |
| Audio overlap | 2 seconds | Prevent word cutoff |
| Audio sample rate | 16 kHz | Optimal for speech |
| Audio format | AAC 16kbps | ~120KB per chunk |
| HealthKit interval | 5 minutes | Incremental sync with anchors |
| Sync interval | 5 minutes | Upload frequency |
| Batch uploads | ≤6 requests | One per stream with pending data |
| Network timeout | 30 seconds | Request timeout |
| Max retries | 5 | Upload attempts |
| Backoff max | 300 seconds | Network retry ceiling |
| Data retention | 3 days | All local data cleanup |
| Storage warning | 100 MB | Critical storage threshold |
| Background limit | 30 seconds | iOS background task limit |

## Troubleshooting

### HealthKit Not Syncing

1. Check Settings > Privacy > Health > Virtues
2. All categories must show checkmarks
3. Force quit and restart app
4. Check for iOS health database corruption
5. **Apple Watch sync delay**: Data syncs every 10-15 minutes
6. **Measurement vs sync time**: Data timestamps reflect when measured, not when synced
7. **Reset anchors**: Delete and reinstall app for fresh incremental sync

### Audio Not Recording

1. Check Settings > Privacy > Microphone > Virtues
2. Verify no other app is using microphone
3. Check audio input device in Settings > Audio Input
4. Select "iPhone Microphone" to prevent Bluetooth interference
5. Reset audio session by toggling recording off/on

### Location Gaps

1. Ensure "Always" permission (not "While Using")
2. Check Settings > General > Background App Refresh
3. Disable Low Power Mode
4. Check for location services system toggle

### Upload Failures

1. Verify network connectivity
2. Check API endpoint URL format
3. Validate API key in settings
4. Monitor SQLite retry count
5. Check available storage

### Battery Drain

1. Normal: 10-15% additional per day
2. High drain: Check location accuracy setting
3. Consider disabling audio if not needed
4. Reduce HealthKit collection frequency

### Audio Stops After Phone Call

1. Check logs for "Audio interruption began/ended" messages
2. Verify AVAudioSession interruption handling is working
3. Force restart recording: Settings > toggle Audio Recording off/on
4. Check if other apps are claiming exclusive audio
5. Restart the app to reset audio session

### Long Gaps in Data

1. Compare `lastSuccessfulSyncDate` to current time
2. Check if gap is exactly 2+ hours (background execution limit)
3. Verify background modes are enabled in iOS Settings
4. Check for iOS Low Power Mode (disables background refresh)
5. Look for timer cancellation in logs
6. Ensure app wasn't force-quit (prevents background execution)

## iOS Background Limits

### The 30-Second Constraint

iOS allows background tasks only 30 seconds to complete before forcefully terminating them. This affects:

- **Initial sync**: 7 days of HealthKit data may contain thousands of records
- **Large uploads**: Audio chunks or accumulated offline data
- **Poor network**: Slow connections may not complete within time limit

### How It's Handled

- **Chunked uploads**: Large datasets automatically split into smaller batches
- **Resume capability**: Failed uploads retry on next sync cycle
- **Progress tracking**: SQLite tracks partial upload progress
- **Foreground priority**: Initial sync recommends keeping app open

## Background Modes

Required in Info.plist:

- `location` - Location updates
- `audio` - Audio recording  
- `fetch` - Background fetch
- `processing` - Background tasks

Background task identifiers:

- `com.virtues.ios.refresh`
- `com.virtues.ios.processing`
- `com.virtues.ios.sync`
