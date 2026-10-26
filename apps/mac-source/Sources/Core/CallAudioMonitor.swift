import AVFoundation
import CollectorPolicy
import CoreAudio
import Foundation

/// Records both sides of a call: the owner's mic and the call app's own output.
///
/// The phone hears only the owner's side of a call, because the far side plays
/// into headphones. Here the call app's audio is tapped before it reaches any
/// output device (a Core Audio process tap), so headphones do not matter.
///
/// A call is detected from which process has the mic open (`CallPolicy` holds
/// the list of call apps and the start/end rule). Every input device is
/// watched with a listener, because a call app can use a mic that is not the
/// system default. The process list is polled once a second only while some
/// input device is running or a call is still inside its end grace, so an idle
/// Mac does no polling.
///
/// Recordings are stereo `.m4a` chunks in `~/.virtues/call-audio/`, owner on
/// the left channel and far side on the right, each with a JSON sidecar.
/// Uploading them is not built yet (`agents/plan/call-audio-plan.md`).
@available(macOS 14.4, *)
final class CallAudioMonitor {
    static let directory = Config.configDir.appendingPathComponent("call-audio")

    /// Stop starting new recordings past this much on disk. Nothing uploads
    /// these chunks yet, so without a cap a Mac that takes calls all day would
    /// fill its disk.
    private static let maxDirectoryBytes: Int64 = 2 * 1024 * 1024 * 1024

    private let directory: URL
    private let queue = DispatchQueue(label: "com.virtues.collector.call-audio")
    private var tracker = CallPolicy.Tracker()
    private var poll: DispatchSourceTimer?
    private var watchedInputs: [AudioObjectID] = []
    private var recorder: CallRecorder?
    private var callId: String?

    private lazy var inputRunningListener: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
        self?.evaluate()
    }
    private lazy var defaultDeviceListener: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
        self?.defaultDeviceChanged()
    }
    private lazy var deviceListListener: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
        self?.watchInputs()
        self?.evaluate()
    }

    /// The selectors on the system object this monitor listens to, and the
    /// handler for each.
    private var systemListeners: [(AudioObjectPropertySelector, AudioObjectPropertyListenerBlock)] {
        [(kAudioHardwarePropertyDefaultInputDevice, defaultDeviceListener),
         (kAudioHardwarePropertyDefaultOutputDevice, defaultDeviceListener),
         (kAudioHardwarePropertyDevices, deviceListListener)]
    }

    init(directory: URL = CallAudioMonitor.directory) {
        self.directory = directory
    }

    func start() {
        print("Starting call audio monitor (\(directory.path))...")
        queue.async { [self] in
            for (selector, listener) in systemListeners {
                var address = CoreAudioProperty.address(selector)
                AudioObjectAddPropertyListenerBlock(
                    AudioObjectID(kAudioObjectSystemObject), &address, queue, listener)
            }
            watchInputs()
            evaluate()
        }
    }

    func stop() {
        queue.sync {
            poll?.cancel()
            poll = nil
            unwatchInputs()
            for (selector, listener) in systemListeners {
                var address = CoreAudioProperty.address(selector)
                AudioObjectRemovePropertyListenerBlock(
                    AudioObjectID(kAudioObjectSystemObject), &address, queue, listener)
            }
            recorder?.stop()
            recorder = nil
        }
    }

    // MARK: - Detection (on `queue`)

    /// Listen for "running" on every device with an input, including ones
    /// plugged in later (the device list listener calls this again).
    private func watchInputs() {
        unwatchInputs()
        watchedInputs = CoreAudioProperty.objects(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDevices)
            .filter { CoreAudioProperty.inputChannels($0) > 0 }
        for device in watchedInputs {
            var address = CoreAudioProperty.address(kAudioDevicePropertyDeviceIsRunningSomewhere)
            AudioObjectAddPropertyListenerBlock(device, &address, queue, inputRunningListener)
        }
    }

    private func unwatchInputs() {
        for device in watchedInputs {
            var address = CoreAudioProperty.address(kAudioDevicePropertyDeviceIsRunningSomewhere)
            AudioObjectRemovePropertyListenerBlock(device, &address, queue, inputRunningListener)
        }
        watchedInputs = []
    }

    /// A different mic or output mid-call changes the tap's format and the
    /// aggregate's clock, so the recorder is rebuilt and the call continues in
    /// a new chunk.
    private func defaultDeviceChanged() {
        if let recorder, let callId {
            print("· call audio: audio device changed, restarting the recording")
            recorder.stop()
            self.recorder = CallRecorder.start(app: recorder.app, callId: callId, directory: directory)
        }
        evaluate()
    }

    /// Poll once a second while the mic is running or a call is ending.
    private func evaluate() {
        let inputRunning = watchedInputs.contains {
            CoreAudioProperty.get($0, kAudioDevicePropertyDeviceIsRunningSomewhere, UInt32(0)) != 0
        }
        if inputRunning || tracker.current != nil {
            if poll == nil {
                print("· call audio: a mic is in use by \(Self.processesHoldingMic().joined(separator: ", "))")
                let timer = DispatchSource.makeTimerSource(queue: queue)
                timer.schedule(deadline: .now(), repeating: 1)
                timer.setEventHandler { [weak self] in self?.tick() }
                timer.resume()
                poll = timer
            }
        } else {
            poll?.cancel()
            poll = nil
        }
    }

    private func tick() {
        for event in tracker.observe(Self.callAppsHoldingMic(), at: Date()) {
            switch event {
            case .started(let app): callStarted(app)
            case .ended(let app): callEnded(app)
            }
        }
        evaluate()
    }

    private func callStarted(_ app: CallPolicy.App) {
        print("📞 call started: \(app.name)")
        if isPaused() {
            print("· call audio: collector is paused, not recording")
            return
        }
        if Self.directoryBytes(directory) > Self.maxDirectoryBytes {
            print("⚠️ call audio: \(directory.path) is over its size cap, not recording")
            return
        }
        let id = UUID().uuidString.lowercased()
        callId = id
        recorder = CallRecorder.start(app: app, callId: id, directory: directory)
    }

    private func callEnded(_ app: CallPolicy.App) {
        print("📞 call ended: \(app.name)")
        recorder?.stop()
        recorder = nil
        callId = nil
    }

    /// Bundle ids of every process with an input running, for the log, so a
    /// call app missing from `CallPolicy.apps` shows up by name.
    private static func processesHoldingMic() -> [String] {
        CoreAudioProperty.processObjects()
            .filter { CoreAudioProperty.get($0, kAudioProcessPropertyIsRunningInput, UInt32(0)) != 0 }
            .map { CoreAudioProperty.string($0, kAudioProcessPropertyBundleID) }
    }

    /// The call apps whose processes have an input running right now.
    private static func callAppsHoldingMic() -> Set<CallPolicy.App> {
        let me = getpid()
        var apps = Set<CallPolicy.App>()
        for process in CoreAudioProperty.processObjects() {
            guard CoreAudioProperty.get(process, kAudioProcessPropertyIsRunningInput, UInt32(0)) != 0,
                  CoreAudioProperty.get(process, kAudioProcessPropertyPID, pid_t(-1)) != me,
                  let app = CallPolicy.app(forBundleID: CoreAudioProperty.string(process, kAudioProcessPropertyBundleID))
            else { continue }
            apps.insert(app)
        }
        return apps
    }

    private func isPaused() -> Bool {
        FileManager.default.fileExists(atPath: Config.configDir.appendingPathComponent("paused").path)
    }

    private static func directoryBytes(_ directory: URL) -> Int64 {
        guard let files = FileManager.default.enumerator(
            at: directory, includingPropertiesForKeys: [.fileSizeKey]) else { return 0 }
        var total: Int64 = 0
        for case let url as URL in files {
            total += Int64((try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)
        }
        return total
    }
}

/// One recording of one call: a process tap on the call app and the mic it
/// uses, in a single private aggregate device so both arrive on one clock in
/// one callback, written as rotating stereo chunks.
///
/// The mic is the aggregate's main device and the tap is drift-compensated to
/// it. The output device is never added: screenpipe found that putting the real
/// output device in an aggregate can stop Zoom and Slack from starting.
@available(macOS 14.4, *)
final class CallRecorder {
    let app: CallPolicy.App
    private let callId: String
    private let directory: URL
    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var ioProc: AudioDeviceIOProcID?
    private let writer: ChunkWriter

    private init(app: CallPolicy.App, callId: String, directory: URL, writer: ChunkWriter) {
        self.app = app
        self.callId = callId
        self.directory = directory
        self.writer = writer
    }

    /// Build the tap and aggregate and start recording. nil when the call app
    /// has no audio process to tap or Core Audio refuses; the reason is logged.
    static func start(app: CallPolicy.App, callId: String, directory: URL) -> CallRecorder? {
        let processes = CoreAudioProperty.processObjects().filter { process in
            let bundleID = CoreAudioProperty.string(process, kAudioProcessPropertyBundleID)
            return app.bundlePrefixes.contains { bundleID.hasPrefix($0) }
        }
        guard !processes.isEmpty else {
            print("⚠️ call audio: \(app.name) has no audio process to tap")
            return nil
        }
        let mic = micDevice(for: processes)
        let micUID = CoreAudioProperty.string(mic, kAudioDevicePropertyDeviceUID)
        let micChannels = CoreAudioProperty.inputChannels(mic)
        guard !micUID.isEmpty, micChannels > 0 else {
            print("⚠️ call audio: no usable input device")
            return nil
        }

        let description = CATapDescription(stereoMixdownOfProcesses: processes)
        description.uuid = UUID()
        description.name = "Virtues call audio"
        description.isPrivate = true
        description.muteBehavior = .unmuted
        var tap = AudioObjectID(kAudioObjectUnknown)
        var status = AudioHardwareCreateProcessTap(description, &tap)
        guard status == noErr else {
            print("⚠️ call audio: could not create the tap (\(status))")
            return nil
        }

        let aggregateDescription: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Virtues call audio",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceMainSubDeviceKey: micUID,
            kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: micUID]],
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceTapListKey: [[
                kAudioSubTapUIDKey: description.uuid.uuidString,
                kAudioSubTapDriftCompensationKey: true,
            ]],
        ]
        var aggregate = AudioObjectID(kAudioObjectUnknown)
        status = AudioHardwareCreateAggregateDevice(aggregateDescription as CFDictionary, &aggregate)
        guard status == noErr else {
            print("⚠️ call audio: could not create the aggregate device (\(status))")
            AudioHardwareDestroyProcessTap(tap)
            return nil
        }

        let sampleRate = CoreAudioProperty.get(aggregate, kAudioDevicePropertyNominalSampleRate, Float64(0))
        let output: AudioObjectID = CoreAudioProperty.get(
            AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultOutputDevice,
            AudioObjectID(kAudioObjectUnknown))
        let metadata: [String: Any] = [
            "call_id": callId,
            "app": app.name,
            "channel_layout": "owner_left_remote_right",
            "mic_device": CoreAudioProperty.string(mic, kAudioObjectPropertyName),
            "output_device": CoreAudioProperty.string(output, kAudioObjectPropertyName),
            "output_transport": CoreAudioProperty.transport(output),
        ]
        guard sampleRate > 0,
              let writer = ChunkWriter(directory: directory.appendingPathComponent(callId),
                                       sampleRate: sampleRate, metadata: metadata)
        else {
            print("⚠️ call audio: could not open a chunk at \(sampleRate) Hz")
            AudioHardwareDestroyAggregateDevice(aggregate)
            AudioHardwareDestroyProcessTap(tap)
            return nil
        }

        let recorder = CallRecorder(app: app, callId: callId, directory: directory, writer: writer)
        recorder.tapID = tap
        recorder.aggregateID = aggregate

        // The aggregate's input buffers: the mic's channels first, then the
        // tap's two. Owner = the mic's first channel; far side = the tap's
        // channels averaged to one.
        status = AudioDeviceCreateIOProcIDWithBlock(&recorder.ioProc, aggregate, nil) { _, input, _, _, _ in
            writer.append(UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: input)),
                          micChannels: micChannels)
        }
        guard status == noErr, let ioProc = recorder.ioProc else {
            print("⚠️ call audio: could not attach to the aggregate device (\(status))")
            recorder.teardown()
            return nil
        }
        status = AudioDeviceStart(aggregate, ioProc)
        guard status == noErr else {
            print("⚠️ call audio: could not start recording (\(status))")
            recorder.teardown()
            return nil
        }
        print("· call audio: recording \(app.name) at \(Int(sampleRate)) Hz, "
              + "mic \(metadata["mic_device"] ?? "?"), \(processes.count) process(es) tapped")
        return recorder
    }

    func stop() {
        teardown()
    }

    private func teardown() {
        if let ioProc {
            AudioDeviceStop(aggregateID, ioProc)
            AudioDeviceDestroyIOProcID(aggregateID, ioProc)
            self.ioProc = nil
        }
        if aggregateID != kAudioObjectUnknown {
            AudioHardwareDestroyAggregateDevice(aggregateID)
            aggregateID = AudioObjectID(kAudioObjectUnknown)
        }
        if tapID != kAudioObjectUnknown {
            AudioHardwareDestroyProcessTap(tapID)
            tapID = AudioObjectID(kAudioObjectUnknown)
        }
        writer.close()
    }

    /// The input device the call app is actually using, which is not always
    /// the system default. Devices with no input streams are skipped: with
    /// voice processing on, the output device is listed here as its echo
    /// reference (screenpipe). Falls back to the default input.
    private static func micDevice(for processes: [AudioObjectID]) -> AudioObjectID {
        for process in processes
        where CoreAudioProperty.get(process, kAudioProcessPropertyIsRunningInput, UInt32(0)) != 0 {
            let devices = CoreAudioProperty.objects(process, kAudioProcessPropertyDevices, scope: kAudioObjectPropertyScopeInput)
            if let device = devices.first(where: { CoreAudioProperty.inputChannels($0) > 0 }) {
                return device
            }
        }
        return CoreAudioProperty.get(
            AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultInputDevice,
            AudioObjectID(kAudioObjectUnknown))
    }
}

/// Writes stereo frames from the realtime callback to rotating `.m4a` chunks.
///
/// The callback only copies samples and hands them to a serial queue; the
/// encoder runs there, never on the audio thread.
final class ChunkWriter {
    /// Chunk length, matching the phone's, so the box's audio pipeline sees
    /// the same unit from both.
    static let chunkSeconds: Double = 300

    private let directory: URL
    private let sampleRate: Double
    private let metadata: [String: Any]
    private let queue = DispatchQueue(label: "com.virtues.collector.call-audio.writer")
    private let format: AVAudioFormat
    private var file: AVAudioFile?
    private var fileURL: URL?
    private var chunkIndex = 0
    private var chunkStart = Date()
    private var chunkFrames: AVAudioFramePosition = 0
    private var ownerPeak: Float = 0
    private var remotePeak: Float = 0

    init?(directory: URL, sampleRate: Double, metadata: [String: Any]) {
        guard let format = AVAudioFormat(standardFormatWithSampleRate: sampleRate, channels: 2) else { return nil }
        self.directory = directory
        self.sampleRate = sampleRate
        self.metadata = metadata
        self.format = format
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        } catch {
            print("⚠️ call audio: could not create \(directory.path): \(error.localizedDescription)")
            return nil
        }
        queue.sync { openChunk() }
        if file == nil { return nil }
    }

    /// Called on the audio thread.
    func append(_ buffers: UnsafeMutableAudioBufferListPointer, micChannels: Int) {
        guard let first = buffers.first, first.mNumberChannels > 0 else { return }
        let frames = Int(first.mDataByteSize) / (MemoryLayout<Float>.size * Int(first.mNumberChannels))
        guard frames > 0, let pcm = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(frames)),
              let out = pcm.floatChannelData
        else { return }
        pcm.frameLength = AVAudioFrameCount(frames)

        // Walk the buffers channel by channel: channels [0, micChannels) are
        // the mic, the rest are the tap.
        var channel = 0
        var remoteChannels = 0
        for i in 0..<frames { out[1][i] = 0 }
        for buffer in buffers {
            let count = Int(buffer.mNumberChannels)
            guard let data = buffer.mData?.assumingMemoryBound(to: Float.self) else {
                channel += count
                continue
            }
            for c in 0..<count {
                if channel == 0 {
                    for i in 0..<frames { out[0][i] = data[i * count + c] }
                } else if channel >= micChannels {
                    for i in 0..<frames { out[1][i] += data[i * count + c] }
                    remoteChannels += 1
                }
                channel += 1
            }
        }
        if remoteChannels > 1 {
            let scale = 1 / Float(remoteChannels)
            for i in 0..<frames { out[1][i] *= scale }
        }
        queue.async { [self] in write(pcm) }
    }

    func close() {
        queue.sync { closeChunk() }
    }

    // MARK: - On `queue`

    private func write(_ pcm: AVAudioPCMBuffer) {
        guard let file else { return }
        for ch in 0..<2 {
            let samples = pcm.floatChannelData![ch]
            var peak: Float = 0
            for i in 0..<Int(pcm.frameLength) { peak = max(peak, abs(samples[i])) }
            if ch == 0 { ownerPeak = max(ownerPeak, peak) } else { remotePeak = max(remotePeak, peak) }
        }
        do {
            try file.write(from: pcm)
        } catch {
            print("⚠️ call audio: write failed: \(error.localizedDescription)")
            return
        }
        chunkFrames += AVAudioFramePosition(pcm.frameLength)
        if Double(chunkFrames) >= Self.chunkSeconds * sampleRate {
            closeChunk()
            openChunk()
        }
    }

    private func openChunk() {
        chunkStart = Date()
        chunkFrames = 0
        ownerPeak = 0
        remotePeak = 0
        let url = directory.appendingPathComponent(String(format: "%03d.m4a", chunkIndex))
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: sampleRate,
            AVNumberOfChannelsKey: 2,
            AVEncoderBitRateKey: 64_000,
        ]
        do {
            file = try AVAudioFile(forWriting: url, settings: settings,
                                   commonFormat: .pcmFormatFloat32, interleaved: false)
            fileURL = url
        } catch {
            print("⚠️ call audio: could not open \(url.path): \(error.localizedDescription)")
            file = nil
            fileURL = nil
        }
    }

    /// Finish the current chunk and write its sidecar. Dropping the
    /// `AVAudioFile` is what finalizes the m4a.
    private func closeChunk() {
        guard let url = fileURL else { return }
        file = nil
        fileURL = nil
        let end = Date()
        let duration = Double(chunkFrames) / sampleRate
        chunkIndex += 1
        guard chunkFrames > 0 else {
            try? FileManager.default.removeItem(at: url)
            return
        }
        // A whole chunk of exact zeros on the far side is not a quiet call: it
        // is a denied grant or a tap that stopped delivering.
        if remotePeak == 0 {
            print("⚠️ call audio: the other side was silent for a whole chunk (\(url.lastPathComponent)); "
                  + "check System Audio Recording")
        }
        let iso = ISO8601DateFormatter()
        var sidecar = metadata
        sidecar["timestamp_start"] = iso.string(from: end.addingTimeInterval(-duration))
        sidecar["timestamp_end"] = iso.string(from: end)
        sidecar["duration_seconds"] = duration
        sidecar["sample_rate"] = sampleRate
        sidecar["owner_peak"] = Double(ownerPeak)
        sidecar["remote_peak"] = Double(remotePeak)
        do {
            let data = try JSONSerialization.data(withJSONObject: sidecar, options: [.prettyPrinted, .sortedKeys])
            try data.write(to: url.deletingPathExtension().appendingPathExtension("json"), options: .atomic)
        } catch {
            print("⚠️ call audio: could not write the sidecar for \(url.lastPathComponent): \(error.localizedDescription)")
        }
        print("· call audio: wrote \(url.lastPathComponent), \(Int(duration)) s")
    }
}

/// Small typed wrappers over `AudioObjectGetPropertyData`.
enum CoreAudioProperty {
    static func address(_ selector: AudioObjectPropertySelector,
                        scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain)
    }

    static func get<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector, _ fallback: T) -> T {
        var address = address(selector)
        var size = UInt32(MemoryLayout<T>.size)
        var value = fallback
        let status = withUnsafeMutablePointer(to: &value) {
            AudioObjectGetPropertyData(object, &address, 0, nil, &size, $0)
        }
        return status == noErr ? value : fallback
    }

    static func string(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> String {
        var address = address(selector)
        var size = UInt32(MemoryLayout<CFString?>.size)
        var value: Unmanaged<CFString>?
        guard AudioObjectGetPropertyData(object, &address, 0, nil, &size, &value) == noErr,
              let string = value?.takeRetainedValue()
        else { return "" }
        return string as String
    }

    static func objects(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector,
                        scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal) -> [AudioObjectID] {
        var address = address(selector, scope: scope)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(object, &address, 0, nil, &size) == noErr, size > 0 else { return [] }
        var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
        guard AudioObjectGetPropertyData(object, &address, 0, nil, &size, &ids) == noErr else { return [] }
        return ids
    }

    static func processObjects() -> [AudioObjectID] {
        objects(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyProcessObjectList)
    }

    /// Total input channels across a device's input streams.
    static func inputChannels(_ device: AudioObjectID) -> Int {
        var address = address(kAudioDevicePropertyStreamConfiguration, scope: kAudioObjectPropertyScopeInput)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(device, &address, 0, nil, &size) == noErr, size > 0 else { return 0 }
        let raw = UnsafeMutableRawPointer.allocate(byteCount: Int(size), alignment: MemoryLayout<AudioBufferList>.alignment)
        defer { raw.deallocate() }
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, raw) == noErr else { return 0 }
        let list = UnsafeMutableAudioBufferListPointer(raw.assumingMemoryBound(to: AudioBufferList.self))
        return list.reduce(0) { $0 + Int($1.mNumberChannels) }
    }

    /// "bluetooth", "builtin", "usb" and so on, so a recording says whether
    /// the far side could have leaked into the mic through the speakers.
    static func transport(_ device: AudioObjectID) -> String {
        switch get(device, kAudioDevicePropertyTransportType, UInt32(0)) {
        case kAudioDeviceTransportTypeBuiltIn: return "builtin"
        case kAudioDeviceTransportTypeBluetooth, kAudioDeviceTransportTypeBluetoothLE: return "bluetooth"
        case kAudioDeviceTransportTypeUSB: return "usb"
        case kAudioDeviceTransportTypeHDMI, kAudioDeviceTransportTypeDisplayPort: return "display"
        case kAudioDeviceTransportTypeAirPlay: return "airplay"
        case kAudioDeviceTransportTypeVirtual, kAudioDeviceTransportTypeAggregate: return "virtual"
        default: return "other"
        }
    }
}
