import Foundation
import SwiftUI

@MainActor
final class AppModel: ObservableObject {
    @Published var selectedSection: AppSection = .dashboard
    @Published var projects: [BridgeProject] = []
    @Published var selectedProjectIDs: Set<String> = []
    @Published var focusedProjectID: String?
    @Published var searchText = ""
    @Published var categoryFilter = "all"
    @Published var riskFilter = "all"
    @Published var isScanning = false
    @Published var scanProgress: Double = 0
    @Published var isCleaning = false
    @Published var cleanupMode: CleanupMode = .trash
    @Published var cleanupLog: [String] = []
    @Published var cleanupProgress: Double = 0
    @Published var lastSummary: CleanupSummary?
    @Published var recommendation: RecommendationPreview?
    @Published var trashBatches: [TrashBatch] = []
    @Published var trashEntries: [TrashEntry] = []
    @Published var auditRuns: [AuditRun] = []
    @Published var preferences = GuiPreferences()
    @Published var configSnapshot: ConfigSnapshot?
    @Published var presentedCleanupMode: CleanupMode?
    @Published var alert: AppAlert?

    private let bridge: BridgeRunning
    private var cancelFile: URL?

    init(bridge: BridgeRunning? = nil) {
        if let bridge {
            self.bridge = bridge
        } else {
            self.bridge = (try? BridgeClient()) ?? MockBridgeRunner()
        }
    }

    var selectedProjects: [BridgeProject] {
        projects.filter { selectedProjectIDs.contains($0.id) }
    }

    var focusedProject: BridgeProject? {
        guard let focusedProjectID else { return projects.first }
        return projects.first { $0.id == focusedProjectID }
    }

    var visibleProjects: [BridgeProject] {
        projects.filter { project in
            let matchesSearch = searchText.isEmpty
                || project.displayName.localizedCaseInsensitiveContains(searchText)
                || project.cleanableDir.localizedCaseInsensitiveContains(searchText)
            let matchesCategory = categoryFilter == "all" || project.category == categoryFilter
            let matchesRisk = riskFilter == "all" || project.riskLevel == riskFilter
            return matchesSearch && matchesCategory && matchesRisk
        }
    }

    var totalCleanableBytes: UInt64 {
        projects.reduce(0) { $0 + $1.size }
    }

    var selectedBytes: UInt64 {
        selectedProjects.reduce(0) { $0 + $1.size }
    }

    func bootstrap() async {
        await loadConfig()
        await refreshTrash()
        await refreshAudit()
    }

    func smartScan() async {
        isScanning = true
        scanProgress = 0
        cleanupLog.removeAll()
        projects.removeAll()
        selectedProjectIDs.removeAll()
        focusedProjectID = nil
        selectedSection = .scanResults

        do {
            try await bridge.run(arguments: ["scan", FileManager.default.homeDirectoryForCurrentUser.path, "--depth", "4", "--max-risk", "all"]) { [weak self] event in
                self?.handle(event)
            }
        } catch {
            alert = AppAlert(title: "Scan Failed", message: error.localizedDescription)
        }
        isScanning = false
    }

    func generateRecommendation(strategy: String = "balanced", target: String = "10GB") async {
        selectedSection = .recommendations
        do {
            try await bridge.run(arguments: ["recommend", FileManager.default.homeDirectoryForCurrentUser.path, "--cleanup", target, "--strategy", strategy, "--max-risk", "all"]) { [weak self] event in
                self?.handle(event)
            }
        } catch {
            alert = AppAlert(title: "Recommendation Failed", message: error.localizedDescription)
        }
    }

    func presentCleanupPlan(mode: CleanupMode) {
        cleanupMode = mode
        presentedCleanupMode = mode
    }

    func runSelectedCleanup(mode: CleanupMode) async {
        presentedCleanupMode = nil
        guard !selectedProjects.isEmpty else { return }
        do {
            let plan = try writeTemporaryPlan(projects: selectedProjects)
            try await runApply(plan: plan, mode: mode)
        } catch {
            alert = AppAlert(title: "Cleanup Failed", message: error.localizedDescription)
        }
    }

    func runRecommendationCleanup(mode: CleanupMode) async {
        guard let recommendation, !recommendation.projects.isEmpty else { return }
        do {
            let plan = try writeTemporaryPlan(projects: recommendation.projects)
            try await runApply(plan: plan, mode: mode)
        } catch {
            alert = AppAlert(title: "Cleanup Failed", message: error.localizedDescription)
        }
    }

    func stopAfterCurrentItem() {
        guard let cancelFile else { return }
        FileManager.default.createFile(atPath: cancelFile.path, contents: Data())
        cleanupLog.append("> Stop requested. Current item will finish first.")
    }

    func refreshTrash() async {
        do {
            try await bridge.run(arguments: ["trash", "list"]) { [weak self] event in
                self?.handle(event)
            }
        } catch {
            alert = AppAlert(title: "Trash Refresh Failed", message: error.localizedDescription)
        }
    }

    func restore(batch: TrashBatch) async {
        do {
            try await bridge.run(arguments: ["trash", "restore", "--batch", batch.batchId]) { [weak self] event in
                self?.handle(event)
            }
            await refreshTrash()
        } catch {
            alert = AppAlert(title: "Restore Failed", message: error.localizedDescription)
        }
    }

    func purge(batch: TrashBatch) async {
        do {
            try await bridge.run(arguments: ["trash", "purge", "--batch", batch.batchId]) { [weak self] event in
                self?.handle(event)
            }
            await refreshTrash()
        } catch {
            alert = AppAlert(title: "Purge Failed", message: error.localizedDescription)
        }
    }

    func refreshAudit() async {
        do {
            try await bridge.run(arguments: ["audit", "list"]) { [weak self] event in
                self?.handle(event)
            }
        } catch {
            alert = AppAlert(title: "Audit Refresh Failed", message: error.localizedDescription)
        }
    }

    func loadConfig() async {
        do {
            try await bridge.run(arguments: ["config", "get"]) { [weak self] event in
                self?.handle(event)
            }
        } catch {
            preferences = GuiPreferences()
        }
    }

    func saveConfig() async {
        guard var snapshot = configSnapshot else { return }
        snapshot.guiPreferences = preferences
        do {
            let data = try JSONEncoder.bridge.encode(snapshot)
            let url = FileManager.default.temporaryDirectory.appendingPathComponent("dev-cleaner-config-\(UUID().uuidString).json")
            try data.write(to: url)
            try await bridge.run(arguments: ["config", "save", "--input", url.path]) { [weak self] event in
                self?.handle(event)
            }
        } catch {
            alert = AppAlert(title: "Save Failed", message: error.localizedDescription)
        }
    }

    private func runApply(plan: URL, mode: CleanupMode) async throws {
        isCleaning = true
        cleanupMode = mode
        cleanupProgress = 0
        cleanupLog.removeAll()
        lastSummary = nil
        selectedSection = .scanResults
        let cancel = FileManager.default.temporaryDirectory.appendingPathComponent("dev-cleaner-cancel-\(UUID().uuidString)")
        cancelFile = cancel

        var args = ["apply", plan.path, "--cancel-file", cancel.path]
        switch mode {
        case .dryRun:
            args.append("--dry-run")
        case .trash:
            args.append("--trash")
        case .permanentDelete:
            args.append("--permanent-delete")
        }

        defer {
            isCleaning = false
            cancelFile = nil
            try? FileManager.default.removeItem(at: cancel)
        }

        try await bridge.run(arguments: args) { [weak self] event in
            self?.handle(event)
        }
    }

    private func writeTemporaryPlan(projects: [BridgeProject]) throws -> URL {
        let payload: [String: Any] = [
            "schema_version": 3,
            "tool_version": "mac",
            "created_at": ISO8601DateFormatter().string(from: Date()),
            "scan_root": FileManager.default.homeDirectoryForCurrentUser.path,
            "projects": try projects.map { try $0.asJSONObject() }
        ]
        let data = try JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted])
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("dev-cleaner-plan-\(UUID().uuidString).json")
        try data.write(to: url)
        return url
    }

    private func handle(_ event: BridgeEvent) {
        switch event {
        case .scanStarted(_, let total):
            scanProgress = total == 0 ? 0 : scanProgress
        case .scanItem(let project):
            withAnimation(.easeOut(duration: 0.18)) {
                projects.append(project)
                projects.sort { $0.size > $1.size }
                if focusedProjectID == nil { focusedProjectID = project.id }
            }
        case .scanProgress(let completed, let total):
            scanProgress = total == 0 ? 0 : Double(completed) / Double(total)
        case .scanFinished:
            scanProgress = 1
        case .recommendation(let payload):
            recommendation = RecommendationPreview(payload: payload)
        case .cleanupStarted(let totalCount, let totalBytes, let mode):
            cleanupProgress = 0
            cleanupLog.append("> \(mode) started for \(totalCount) items, \(DevCleanerFormatters.bytes(totalBytes))")
        case .cleanupProject(let path, _):
            cleanupLog.append("> Processing \(DevCleanerFormatters.shortPath(path))")
        case .cleanupDryRun(let path, let action, _):
            cleanupLog.append("[dry-run] \(action) \(DevCleanerFormatters.shortPath(path))")
        case .cleanupSkipped(let path, let reason, _):
            cleanupLog.append("[skipped:\(reason)] \(DevCleanerFormatters.shortPath(path))")
        case .cleanupCompleted(let path, _):
            cleanupLog.append("[success] \(DevCleanerFormatters.shortPath(path))")
        case .cleanupFailed(let path, let error):
            cleanupLog.append("[failed] \(DevCleanerFormatters.shortPath(path)): \(error)")
        case .cleanupCancelled:
            cleanupLog.append("> Cleanup cancelled.")
        case .cleanupFinished(let payload):
            cleanupProgress = 1
            lastSummary = CleanupSummary(payload: payload)
            selectedSection = .dashboard
            Task {
                await refreshTrash()
                await refreshAudit()
            }
        case .trashList(let payload):
            trashBatches = payload.decodePayloadArray(path: ["batches"], as: TrashBatch.self)
        case .trashEntries(let payload):
            trashEntries = payload.decodePayloadArray(path: ["entries"], as: TrashEntry.self)
        case .auditList(let payload):
            auditRuns = payload.decodePayloadArray(path: ["runs"], as: AuditRun.self)
        case .configSnapshot(let snapshot):
            configSnapshot = snapshot
            preferences = snapshot.guiPreferences
        case .configSaved:
            alert = AppAlert(title: "Settings Saved", message: "Dev Cleaner preferences were updated.")
        case .error(let message):
            alert = AppAlert(title: "Bridge Error", message: message)
        default:
            break
        }
    }
}

struct RecommendationPreview {
    var targetBytes: UInt64 = 0
    var selectedBytes: UInt64 = 0
    var selectedCount: Int = 0
    var strategy: String = "balanced"
    var projects: [BridgeProject] = []

    init(payload: JSONValue) {
        targetBytes = payload.uint64(path: ["target_bytes"]) ?? 0
        selectedBytes = payload.uint64(path: ["selected_bytes"]) ?? 0
        selectedCount = payload.int(path: ["selected_count"]) ?? 0
        strategy = payload.string(path: ["strategy"]) ?? "balanced"
        projects = payload.decodePayloadArray(path: ["projects"], as: BridgeProject.self)
    }
}

struct CleanupSummary {
    var cleanedCount: Int = 0
    var skippedCount: Int = 0
    var failedCount: Int = 0
    var bytesFreed: UInt64 = 0
    var trashBatchID: String?
    var cancelled = false

    init(payload: JSONValue) {
        cleanedCount = payload.int(path: ["cleaned_count"]) ?? 0
        skippedCount = payload.int(path: ["skipped_count"]) ?? 0
        failedCount = payload.int(path: ["failed_count"]) ?? 0
        bytesFreed = payload.uint64(path: ["bytes_freed"]) ?? 0
        trashBatchID = payload.string(path: ["trash_batch_id"])
        cancelled = payload.bool(path: ["cancelled"]) ?? false
    }
}

struct AppAlert: Identifiable {
    var id = UUID()
    var title: String
    var message: String
}

private final class MockBridgeRunner: BridgeRunning {
    func run(arguments: [String], onEvent: @escaping @MainActor (BridgeEvent) -> Void) async throws {
        await onEvent(.error("Bridge helper is unavailable. Build the Rust helper first."))
    }
}

extension Encodable {
    fileprivate func asJSONObject() throws -> Any {
        let data = try JSONEncoder.bridge.encode(self)
        return try JSONSerialization.jsonObject(with: data)
    }
}

extension JSONEncoder {
    static var bridge: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        return encoder
    }
}

extension JSONValue {
    func value(at path: [String]) -> JSONValue? {
        guard let first = path.first else { return self }
        guard case .object(let object) = self, let next = object[first] else { return nil }
        return next.value(at: Array(path.dropFirst()))
    }

    func string(path: [String]) -> String? {
        if case .string(let value) = value(at: path) { return value }
        return nil
    }

    func int(path: [String]) -> Int? {
        if case .number(let value) = value(at: path) { return Int(value) }
        return nil
    }

    func uint64(path: [String]) -> UInt64? {
        if case .number(let value) = value(at: path) { return UInt64(value) }
        return nil
    }

    func bool(path: [String]) -> Bool? {
        if case .bool(let value) = value(at: path) { return value }
        return nil
    }

    func decodePayloadArray<T: Decodable>(path: [String], as type: T.Type) -> [T] {
        guard let value = value(at: path),
              let data = try? JSONEncoder.bridge.encode(value),
              let decoded = try? JSONDecoder.bridge.decode([T].self, from: data) else {
            return []
        }
        return decoded
    }
}
