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
/// Recordings are pairs of mono `.m4a` chunks in `~/.virtues/call-audio/`,
/// one for the owner and one for the far side, with a JSON sidecar each.
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
/// one callback, written as rotating pairs of chunks (`ChunkWriter`).
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
            "mic_device": CoreAudioProperty.string(mic, kAudioObjectPropertyName),
            "output_device": CoreAudioProperty.string(output, kAudioObjectPropertyName),
            "output_transport": CoreAudioProperty.transport(output),
        ]
        guard sampleRate > 0,
              let writer = ChunkWriter(directory: directory.appendingPathComponent(callId),
                                       inputRate: sampleRate, metadata: metadata)
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
        // tap's two.
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

/// Writes the owner and the far side from the realtime callback to rotating
/// pairs of mono `.m4a` chunks, `NNN-owner.m4a` and `NNN-remote.m4a`, with one
/// `NNN.json` sidecar.
///
/// Each side is its own file because the box labels speakers by transcribing
/// each side on its own: the transcription model ignores stereo placement
/// (`agents/plan/call-audio-plan.md`). Both are resampled to 16 kHz mono AAC
/// at 24 kbps, the phone's format, so the box's speech detector and
/// transcription see the same input from both devices.
///
/// The callback only copies samples and hands them to a serial queue; the
/// resampling and encoding run there, never on the audio thread.
final class ChunkWriter {
    /// Chunk length, matching the phone's, so the box's audio pipeline sees
    /// the same unit from both.
    static let chunkSeconds: Double = 300
    static let sampleRate: Double = 16_000
    static let bitRate = 24_000

    /// One side of the call: its resampler, its open file and its peak level.
    private final class Track {
        let role: String
        let converter: AVAudioConverter
        var file: AVAudioFile?
        var url: URL?
        var peak: Float = 0

        init(role: String, converter: AVAudioConverter) {
            self.role = role
            self.converter = converter
        }
    }

    private let directory: URL
    private let inputRate: Double
    private let metadata: [String: Any]
    private let queue = DispatchQueue(label: "com.virtues.collector.call-audio.writer")
    private let inputFormat: AVAudioFormat
    private let outputFormat: AVAudioFormat
    private let owner: Track
    private let remote: Track
    private var chunkIndex = 0
    private var chunkFrames: AVAudioFramePosition = 0

    init?(directory: URL, inputRate: Double, metadata: [String: Any]) {
        guard let inputFormat = AVAudioFormat(standardFormatWithSampleRate: inputRate, channels: 1),
              let outputFormat = AVAudioFormat(standardFormatWithSampleRate: Self.sampleRate, channels: 1),
              let ownerConverter = AVAudioConverter(from: inputFormat, to: outputFormat),
              let remoteConverter = AVAudioConverter(from: inputFormat, to: outputFormat)
        else { return nil }
        self.directory = directory
        self.inputRate = inputRate
        self.metadata = metadata
        self.inputFormat = inputFormat
        self.outputFormat = outputFormat
        owner = Track(role: "owner", converter: ownerConverter)
        remote = Track(role: "remote", converter: remoteConverter)
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        } catch {
            print("⚠️ call audio: could not create \(directory.path): \(error.localizedDescription)")
            return nil
        }
        queue.sync { openChunk() }
        if owner.file == nil || remote.file == nil { return nil }
    }

    /// Called on the audio thread.
    func append(_ buffers: UnsafeMutableAudioBufferListPointer, micChannels: Int) {
        guard let first = buffers.first, first.mNumberChannels > 0 else { return }
        let frames = Int(first.mDataByteSize) / (MemoryLayout<Float>.size * Int(first.mNumberChannels))
        guard frames > 0,
              let ownerPCM = AVAudioPCMBuffer(pcmFormat: inputFormat, frameCapacity: AVAudioFrameCount(frames)),
              let remotePCM = AVAudioPCMBuffer(pcmFormat: inputFormat, frameCapacity: AVAudioFrameCount(frames)),
              let ownerOut = ownerPCM.floatChannelData?[0],
              let remoteOut = remotePCM.floatChannelData?[0]
        else { return }
        ownerPCM.frameLength = AVAudioFrameCount(frames)
        remotePCM.frameLength = AVAudioFrameCount(frames)

        // Walk the buffers channel by channel: channels [0, micChannels) are
        // the mic, the rest are the tap. Owner = the mic's first channel; far
        // side = the tap's channels averaged to one.
        var channel = 0
        var remoteChannels = 0
        for i in 0..<frames {
            ownerOut[i] = 0
            remoteOut[i] = 0
        }
        for buffer in buffers {
            let count = Int(buffer.mNumberChannels)
            guard let data = buffer.mData?.assumingMemoryBound(to: Float.self) else {
                channel += count
                continue
            }
            for c in 0..<count {
                if channel == 0 {
                    for i in 0..<frames { ownerOut[i] = data[i * count + c] }
                } else if channel >= micChannels {
                    for i in 0..<frames { remoteOut[i] += data[i * count + c] }
                    remoteChannels += 1
                }
                channel += 1
            }
        }
        if remoteChannels > 1 {
            let scale = 1 / Float(remoteChannels)
            for i in 0..<frames { remoteOut[i] *= scale }
        }
        queue.async { [self] in write(owner: ownerPCM, remote: remotePCM) }
    }

    func close() {
        queue.sync { closeChunk() }
    }

    // MARK: - On `queue`

    private func write(owner ownerPCM: AVAudioPCMBuffer, remote remotePCM: AVAudioPCMBuffer) {
        guard owner.file != nil, remote.file != nil else { return }
        write(ownerPCM, to: owner)
        write(remotePCM, to: remote)
        chunkFrames += AVAudioFramePosition(ownerPCM.frameLength)
        if Double(chunkFrames) >= Self.chunkSeconds * inputRate {
            closeChunk()
            openChunk()
        }
    }

    /// Resample one buffer to 16 kHz and append it to the track's file. The
    /// converter keeps its filter state between calls, so consecutive buffers
    /// join without a click.
    private func write(_ input: AVAudioPCMBuffer, to track: Track) {
        let capacity = AVAudioFrameCount(Double(input.frameLength) * Self.sampleRate / inputRate) + 32
        guard let out = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: capacity) else { return }
        var fed = false
        var error: NSError?
        track.converter.convert(to: out, error: &error) { _, status in
            if fed {
                status.pointee = .noDataNow
                return nil
            }
            fed = true
            status.pointee = .haveData
            return input
        }
        if let error {
            print("⚠️ call audio: resampling the \(track.role) side failed: \(error.localizedDescription)")
            return
        }
        guard out.frameLength > 0, let samples = out.floatChannelData?[0] else { return }
        for i in 0..<Int(out.frameLength) { track.peak = max(track.peak, abs(samples[i])) }
        do {
            try track.file?.write(from: out)
        } catch {
            print("⚠️ call audio: writing the \(track.role) side failed: \(error.localizedDescription)")
        }
    }

    private func openChunk() {
        chunkFrames = 0
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: Self.sampleRate,
            AVNumberOfChannelsKey: 1,
            AVEncoderBitRateKey: Self.bitRate,
        ]
        for track in [owner, remote] {
            track.peak = 0
            let url = directory.appendingPathComponent(String(format: "%03d-%@.m4a", chunkIndex, track.role))
            do {
                track.file = try AVAudioFile(forWriting: url, settings: settings,
                                             commonFormat: .pcmFormatFloat32, interleaved: false)
                track.url = url
            } catch {
                print("⚠️ call audio: could not open \(url.path): \(error.localizedDescription)")
                track.file = nil
                track.url = nil
            }
        }
    }

    /// Finish the current pair of chunks and write their sidecar. Dropping the
    /// `AVAudioFile` is what finalizes an m4a.
    private func closeChunk() {
        let urls = [owner, remote].compactMap(\.url)
        guard !urls.isEmpty else { return }
        for track in [owner, remote] {
            track.file = nil
            track.url = nil
        }
        let end = Date()
        let duration = Double(chunkFrames) / inputRate
        let index = chunkIndex
        chunkIndex += 1
        guard chunkFrames > 0 else {
            for url in urls { try? FileManager.default.removeItem(at: url) }
            return
        }
        // A whole chunk of exact zeros on the far side is not a quiet call: it
        // is a denied grant or a tap that stopped delivering.
        if remote.peak == 0 {
            print("⚠️ call audio: the other side was silent for a whole chunk (\(index)); "
                  + "check System Audio Recording")
        }
        let iso = ISO8601DateFormatter()
        var sidecar = metadata
        sidecar["timestamp_start"] = iso.string(from: end.addingTimeInterval(-duration))
        sidecar["timestamp_end"] = iso.string(from: end)
        sidecar["duration_seconds"] = duration
        sidecar["sample_rate"] = Self.sampleRate
        sidecar["files"] = [
            "owner": String(format: "%03d-owner.m4a", index),
            "remote": String(format: "%03d-remote.m4a", index),
        ]
        sidecar["owner_peak"] = Double(owner.peak)
        sidecar["remote_peak"] = Double(remote.peak)
        let sidecarURL = directory.appendingPathComponent(String(format: "%03d.json", index))
        do {
            let data = try JSONSerialization.data(withJSONObject: sidecar, options: [.prettyPrinted, .sortedKeys])
            try data.write(to: sidecarURL, options: .atomic)
        } catch {
            print("⚠️ call audio: could not write \(sidecarURL.lastPathComponent): \(error.localizedDescription)")
        }
        print("· call audio: wrote chunk \(index), \(Int(duration)) s")
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
