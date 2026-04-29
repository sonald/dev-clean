import Foundation

enum BridgeClientError: LocalizedError {
    case helperNotFound
    case launchFailed(String)
    case decodeFailed(String)
    case failedExit(Int32, String)

    var errorDescription: String? {
        switch self {
        case .helperNotFound:
            "The dev-cleaner helper could not be found in the app bundle or repository build output."
        case .launchFailed(let message):
            message
        case .decodeFailed(let message):
            message
        case .failedExit(let code, let stderr):
            "Helper exited with code \(code). \(stderr)"
        }
    }
}

enum BridgeEvent {
    case ready(version: String)
    case scanStarted(roots: [String], total: Int)
    case scanItem(BridgeProject)
    case scanProgress(completed: Int, total: Int)
    case scanFinished(totalCount: Int, totalBytes: UInt64)
    case recommendation(JSONValue)
    case cleanupStarted(totalCount: Int, totalBytes: UInt64, mode: String)
    case cleanupProject(path: String, size: UInt64)
    case cleanupDryRun(path: String, action: String, size: UInt64)
    case cleanupSkipped(path: String, reason: String, size: UInt64)
    case cleanupCompleted(path: String, size: UInt64)
    case cleanupFailed(path: String, error: String)
    case cleanupCancelled(remaining: Int)
    case cleanupFinished(JSONValue)
    case trashList(JSONValue)
    case trashEntries(JSONValue)
    case trashOperationFinished(JSONValue)
    case auditList(JSONValue)
    case auditRecords(JSONValue)
    case auditExport(JSONValue)
    case configSnapshot(ConfigSnapshot)
    case configSaved(path: String)
    case error(String)
    case unknown(type: String)
}

protocol BridgeRunning {
    func run(arguments: [String], onEvent: @escaping @MainActor (BridgeEvent) -> Void) async throws
}

final class BridgeClient: BridgeRunning {
    private let helperURL: URL

    init(helperURL: URL? = nil) throws {
        if let helperURL {
            self.helperURL = helperURL
        } else if let bundled = Bundle.main.url(forResource: "dev-cleaner-helper", withExtension: nil) {
            self.helperURL = bundled
        } else {
            let repoHelper = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
                .deletingLastPathComponent()
                .deletingLastPathComponent()
                .appendingPathComponent("target/debug/dev-cleaner")
            guard FileManager.default.isExecutableFile(atPath: repoHelper.path) else {
                throw BridgeClientError.helperNotFound
            }
            self.helperURL = repoHelper
        }
    }

    func run(arguments: [String], onEvent: @escaping @MainActor (BridgeEvent) -> Void) async throws {
        let processBox = RunningProcessBox()
        return try await withTaskCancellationHandler {
            try await runProcess(arguments: arguments, processBox: processBox, onEvent: onEvent)
        } onCancel: {
            processBox.terminate()
        }
    }

    private func runProcess(
        arguments: [String],
        processBox: RunningProcessBox,
        onEvent: @escaping @MainActor (BridgeEvent) -> Void
    ) async throws {
        try Task.checkCancellation()

        let process = Process()
        process.executableURL = helperURL
        process.arguments = ["bridge"] + arguments
        processBox.set(process)

        let output = Pipe()
        let error = Pipe()
        process.standardOutput = output
        process.standardError = error

        do {
            try process.run()
        } catch {
            throw BridgeClientError.launchFailed(error.localizedDescription)
        }

        do {
            for try await line in output.fileHandleForReading.bytes.lines {
                try Task.checkCancellation()
                guard let data = line.data(using: .utf8) else { continue }
                do {
                    let event = try BridgeEventDecoder.decode(data)
                    await onEvent(event)
                } catch {
                    throw BridgeClientError.decodeFailed("Failed to decode helper event: \(error.localizedDescription)")
                }
            }
        } catch is CancellationError {
            processBox.terminate()
            throw CancellationError()
        } catch {
            if process.isRunning {
                process.terminate()
                process.waitUntilExit()
            }
            throw error
        }

        process.waitUntilExit()
        if Task.isCancelled {
            throw CancellationError()
        }
        if process.terminationStatus != 0 {
            let stderr = String(data: error.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            throw BridgeClientError.failedExit(process.terminationStatus, stderr)
        }
    }
}

private final class RunningProcessBox: @unchecked Sendable {
    private let lock = NSLock()
    private var process: Process?

    func set(_ process: Process) {
        lock.lock()
        self.process = process
        lock.unlock()
    }

    func terminate() {
        lock.lock()
        let process = process
        lock.unlock()

        if process?.isRunning == true {
            process?.terminate()
        }
    }
}

enum BridgeEventDecoder {
    static func decode(_ data: Data) throws -> BridgeEvent {
        let decoder = JSONDecoder.bridge
        let envelope = try decoder.decode(EventEnvelope.self, from: data)
        switch envelope.type {
        case "ready":
            return .ready(version: try value(String.self, "version", data))
        case "scan_started":
            let event = try decoder.decode(ScanStarted.self, from: data)
            return .scanStarted(roots: event.roots, total: event.total)
        case "scan_item":
            return .scanItem(try value(BridgeProject.self, "project", data))
        case "scan_progress":
            let event = try decoder.decode(ScanProgress.self, from: data)
            return .scanProgress(completed: event.completed, total: event.total)
        case "scan_finished":
            let event = try decoder.decode(ScanFinished.self, from: data)
            return .scanFinished(totalCount: event.totalCount, totalBytes: event.totalBytes)
        case "recommendation_ready":
            return .recommendation(try value(JSONValue.self, "payload", data))
        case "cleanup_started":
            let event = try decoder.decode(CleanupStarted.self, from: data)
            return .cleanupStarted(totalCount: event.totalCount, totalBytes: event.totalBytes, mode: event.mode)
        case "cleanup_project":
            let event = try decoder.decode(PathSizeEvent.self, from: data)
            return .cleanupProject(path: event.path, size: event.size)
        case "cleanup_dry_run":
            let event = try decoder.decode(CleanupDryRun.self, from: data)
            return .cleanupDryRun(path: event.path, action: event.action, size: event.size)
        case "cleanup_skipped":
            let event = try decoder.decode(CleanupSkipped.self, from: data)
            return .cleanupSkipped(path: event.path, reason: event.reason, size: event.size)
        case "cleanup_completed":
            let event = try decoder.decode(PathSizeEvent.self, from: data)
            return .cleanupCompleted(path: event.path, size: event.size)
        case "cleanup_failed":
            let event = try decoder.decode(CleanupFailed.self, from: data)
            return .cleanupFailed(path: event.path, error: event.error)
        case "cleanup_cancelled":
            return .cleanupCancelled(remaining: try value(Int.self, "remaining", data))
        case "cleanup_finished":
            return .cleanupFinished(try value(JSONValue.self, "payload", data))
        case "trash_list":
            return .trashList(try value(JSONValue.self, "payload", data))
        case "trash_entries":
            return .trashEntries(try value(JSONValue.self, "payload", data))
        case "trash_operation_finished":
            return .trashOperationFinished(try value(JSONValue.self, "payload", data))
        case "audit_list":
            return .auditList(try value(JSONValue.self, "payload", data))
        case "audit_records":
            return .auditRecords(try value(JSONValue.self, "payload", data))
        case "audit_export":
            return .auditExport(try value(JSONValue.self, "payload", data))
        case "config_snapshot":
            return .configSnapshot(try value(ConfigSnapshot.self, "payload", data))
        case "config_saved":
            return .configSaved(path: try value(String.self, "path", data))
        case "error":
            return .error(try value(String.self, "message", data))
        default:
            return .unknown(type: envelope.type)
        }
    }

    private static func value<T: Decodable>(_ type: T.Type, _ key: String, _ data: Data) throws -> T {
        _ = type
        let decoder = JSONDecoder.bridge
        decoder.userInfo[.bridgeFieldKey] = key
        return try decoder.decode(BridgeField<T>.self, from: data).value
    }
}

private struct BridgeField<T: Decodable>: Decodable {
    var value: T

    init(from decoder: Decoder) throws {
        guard let fieldName = decoder.userInfo[.bridgeFieldKey] as? String,
              let fieldKey = DynamicCodingKey(stringValue: fieldName) else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(codingPath: decoder.codingPath, debugDescription: "Missing bridge field key")
            )
        }
        let container = try decoder.container(keyedBy: DynamicCodingKey.self)
        value = try container.decode(T.self, forKey: fieldKey)
    }
}

private struct DynamicCodingKey: CodingKey {
    var stringValue: String
    var intValue: Int?

    init?(stringValue: String) {
        self.stringValue = stringValue
    }

    init?(intValue: Int) {
        self.stringValue = String(intValue)
        self.intValue = intValue
    }
}

private extension CodingUserInfoKey {
    static let bridgeFieldKey = CodingUserInfoKey(rawValue: "bridgeFieldKey")!
}

private struct EventEnvelope: Decodable {
    var type: String
}

private struct ScanStarted: Decodable {
    var roots: [String]
    var total: Int
}

private struct ScanProgress: Decodable {
    var completed: Int
    var total: Int
}

private struct ScanFinished: Decodable {
    var totalCount: Int
    var totalBytes: UInt64

    enum CodingKeys: String, CodingKey {
        case totalCount = "total_count"
        case totalBytes = "total_bytes"
    }
}

private struct CleanupStarted: Decodable {
    var totalCount: Int
    var totalBytes: UInt64
    var mode: String

    enum CodingKeys: String, CodingKey {
        case totalCount = "total_count"
        case totalBytes = "total_bytes"
        case mode
    }
}

private struct PathSizeEvent: Decodable {
    var path: String
    var size: UInt64
}

private struct CleanupDryRun: Decodable {
    var path: String
    var action: String
    var size: UInt64
}

private struct CleanupSkipped: Decodable {
    var path: String
    var reason: String
    var size: UInt64
}

private struct CleanupFailed: Decodable {
    var path: String
    var error: String
}

extension JSONDecoder {
    static var bridge: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }
}
