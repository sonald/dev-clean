import Foundation
import AppKit
import SwiftUI
import UniformTypeIdentifiers

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
    @Published var scanStatusMessage = "Ready to scan."
    @Published var isCleaning = false
    @Published var cleanupMode: CleanupMode = .trash
    @Published var cleanupLog: [String] = []
    @Published var cleanupProgress: Double = 0
    @Published var cleanupTotalCount = 0
    @Published var cleanupProcessedCount = 0
    @Published var cleanupSucceededCount = 0
    @Published var cleanupSkippedCount = 0
    @Published var cleanupFailedCount = 0
    @Published var cleanupCurrentPath: String?
    @Published var showCleanupSummary = false
    @Published var lastSummary: CleanupSummary?
    @Published var recommendation: RecommendationPreview?
    @Published var trashBatches: [TrashBatch] = []
    @Published var trashEntries: [TrashEntry] = []
    @Published var selectedTrashBatchID: String?
    @Published var auditRuns: [AuditRun] = []
    @Published var auditItems: [AuditItemRecord] = []
    @Published var isRecommending = false
    @Published var auditFilter: AuditFilter = .all
    @Published var preferences = GuiPreferences()
    @Published var configSnapshot: ConfigSnapshot?
    @Published var pendingCleanup: PendingCleanup?
    @Published var alert: AppAlert?
    @Published var trashRoot: String?
    @Published var volume: VolumeInfo?

    private let bridge: BridgeRunning
    private var scanTask: Task<Void, Never>?
    private var cancelFile: URL?
    /// Helper events report canonical paths; map them back to scan result ids.
    private var cleanupIDsByCanonicalPath: [String: String] = [:]
    private var preferencesSaveTask: Task<Void, Never>?

    init(bridge: BridgeRunning? = nil) {
        if let bridge {
            self.bridge = bridge
        } else {
            self.bridge = (try? BridgeClient()) ?? MockBridgeRunner()
        }
    }

    /// Only items the user can currently see and act on; hidden or in-use ids never reach a plan.
    var selectedProjects: [BridgeProject] {
        projects.filter { selectedProjectIDs.contains($0.id) && isShown($0) && $0.isSelectable }
    }

    func isShown(_ project: BridgeProject) -> Bool {
        preferences.showHighRisk || !project.isHighRisk
    }

    var hiddenHighRiskProjects: [BridgeProject] {
        preferences.showHighRisk ? [] : projects.filter(\.isHighRisk)
    }

    func toggleSelection(_ project: BridgeProject) {
        guard project.isSelectable else { return }
        if selectedProjectIDs.contains(project.id) {
            selectedProjectIDs.remove(project.id)
        } else {
            selectedProjectIDs.insert(project.id)
        }
    }

    /// Selects what the current search and filters show; never hidden, in-use or protected items.
    func selectVisible() {
        selectedProjectIDs.formUnion(visibleProjects.filter(\.isSelectable).map(\.id))
    }

    func clearSelection() {
        selectedProjectIDs.removeAll()
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
            return isShown(project) && matchesSearch && matchesCategory && matchesRisk
        }
    }

    /// Space the user could reclaim from what is shown: hidden high-risk and in-use items excluded.
    var totalCleanableBytes: UInt64 {
        projects.filter { isShown($0) && $0.isSelectable }.reduce(0) { $0 + $1.size }
    }

    var selectedBytes: UInt64 {
        selectedProjects.reduce(0) { $0 + $1.size }
    }

    var activeScanRoots: [String] {
        let roots = preferences.scanRoots.isEmpty ? [preferences.scanRootPath] : preferences.scanRoots
        let normalized = roots
            .map(normalizedScanRootPath)
            .filter { !$0.isEmpty }
        var seen = Set<String>()
        return normalized.filter { seen.insert($0).inserted }
    }

    var scanRootPath: String {
        normalizedScanRootPath(activeScanRoots.first ?? preferences.scanRootPath)
    }

    var scanRootDisplayName: String {
        if activeScanRoots.count > 1 {
            return "\(activeScanRoots.count) roots"
        }
        return DevCleanerFormatters.shortPath(scanRootPath)
    }

    var trashTotalBytes: UInt64 {
        trashBatches.reduce(0) { $0 + $1.totalSize }
    }

    var selectedTrashBatch: TrashBatch? {
        guard let selectedTrashBatchID else { return trashBatches.first }
        return trashBatches.first { $0.batchId == selectedTrashBatchID }
    }

    var filteredAuditRuns: [AuditRun] {
        switch auditFilter {
        case .all:
            return auditRuns
        case .last30Days:
            let cutoff = Date().addingTimeInterval(-30 * 24 * 60 * 60)
            return auditRuns.filter { ($0.startedDate ?? .distantPast) >= cutoff }
        case .cleanup:
            return auditRuns.filter { $0.command == "clean" || $0.command == "apply" }
        case .restore:
            return auditRuns.filter { $0.command == "undo" || $0.mode == "restore" }
        }
    }

    var protectedPaths: [String] {
        configArray(key: "keep_paths")
    }

    func bootstrap() async {
        await loadConfig()
        refreshVolume()
        await purgeExpiredTrash()
        await refreshTrash()
        await refreshAudit()
    }

    var trashOverLimit: Bool {
        trashTotalBytes > preferences.trashLimitGb * 1024 * 1024 * 1024
    }

    /// Retention-based only: a size cap would purge the batch the user just created and break undo.
    func purgeExpiredTrash() async {
        guard preferences.autoGcTrash else { return }
        do {
            try await bridge.run(arguments: ["trash", "gc", "--keep-days", "\(preferences.trashRetentionDays)"]) { _ in }
        } catch {
            alert = AppAlert(title: "Trash Cleanup Failed", message: error.localizedDescription)
        }
    }

    /// Debounced so steppers do not spawn one helper process per click.
    func schedulePreferencesSave() {
        preferencesSaveTask?.cancel()
        preferencesSaveTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 400_000_000)
            guard !Task.isCancelled else { return }
            await self?.saveConfig(showAlert: false)
        }
    }

    func refreshVolume() {
        volume = VolumeInfo.forPath(scanRootPath)
    }

    var scanArguments: [String] {
        // Scan up to high risk so hidden items can be counted; the UI hides them unless shown.
        var args = ["--depth", "\(preferences.scanDepth)", "--max-risk", "high", "--recent-days", "\(preferences.recentDays)"]
        if preferences.includeRecent {
            args.append("--include-recent")
        }
        return args
    }

    func startSmartScan() {
        guard !isScanning else {
            stopScan()
            return
        }
        scanTask = Task { [weak self] in
            await self?.smartScan()
        }
    }

    func stopScan() {
        guard isScanning else { return }
        scanStatusMessage = "Stopping scan..."
        scanTask?.cancel()
    }

    func chooseScanRoot() {
        let panel = NSOpenPanel()
        panel.title = "Choose Scan Root"
        panel.prompt = "Use Folder"
        panel.message = "Choose the folder Dev Cleaner should scan."
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = false
        panel.directoryURL = URL(fileURLWithPath: scanRootPath)

        guard panel.runModal() == .OK, let url = panel.url else { return }
        setScanRoot(path: url.path)
    }

    func resetScanRootToHome() {
        setScanRoot(path: FileManager.default.homeDirectoryForCurrentUser.path)
    }

    func setScanRoot(path: String, persist: Bool = true) {
        let normalized = normalizedScanRootPath(path)
        preferences.scanRootPath = normalized
        if preferences.scanRoots.isEmpty {
            preferences.scanRoots = [normalized]
        } else {
            preferences.scanRoots[0] = normalized
        }
        scanStatusMessage = "Ready to scan \(scanRootDisplayName)."
        if persist {
            Task { await saveConfig(showAlert: false) }
        }
    }

    func addScanRoot() {
        let panel = NSOpenPanel()
        panel.title = "Add Scan Root"
        panel.prompt = "Add Path"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = false

        guard panel.runModal() == .OK, let url = panel.url else { return }
        let path = normalizedScanRootPath(url.path)
        if !activeScanRoots.contains(path) {
            preferences.scanRoots.append(path)
            preferences.scanRootPath = activeScanRoots.first ?? path
            Task { await saveConfig(showAlert: false) }
        }
    }

    func removeScanRoot(_ path: String) {
        let normalized = normalizedScanRootPath(path)
        preferences.scanRoots = activeScanRoots.filter { $0 != normalized }
        if preferences.scanRoots.isEmpty {
            preferences.scanRoots = [FileManager.default.homeDirectoryForCurrentUser.path]
        }
        preferences.scanRootPath = preferences.scanRoots[0]
        Task { await saveConfig(showAlert: false) }
    }

    func addProtectedFolder() {
        let panel = NSOpenPanel()
        panel.title = "Add Protection Rule"
        panel.prompt = "Protect Folder"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = false

        guard panel.runModal() == .OK, let url = panel.url else { return }
        appendConfigValue(url.path, toArrayKey: "keep_paths")
        Task { await saveConfig(showAlert: false) }
    }

    func removeProtectedPath(_ path: String) {
        removeConfigValue(path, fromArrayKey: "keep_paths")
        Task { await saveConfig(showAlert: false) }
    }

    func smartScan() async {
        let root = scanRootPath
        isScanning = true
        scanProgress = 0
        scanStatusMessage = "Scanning \(DevCleanerFormatters.shortPath(root))..."
        cleanupLog.removeAll()
        projects.removeAll()
        selectedProjectIDs.removeAll()
        focusedProjectID = nil
        selectedSection = .scanResults

        do {
            let roots = activeScanRoots
            for (index, root) in roots.enumerated() {
                guard !Task.isCancelled else { throw CancellationError() }
                scanStatusMessage = roots.count > 1
                    ? "Scanning \(DevCleanerFormatters.shortPath(root)) (\(index + 1)/\(roots.count))..."
                    : "Scanning \(DevCleanerFormatters.shortPath(root))..."
                try await bridge.run(arguments: ["scan", root] + scanArguments) { [weak self] event in
                    self?.handle(event)
                }
            }
            scanStatusMessage = "Scan complete."
        } catch is CancellationError {
            scanStatusMessage = "Scan stopped. Partial results are preserved."
        } catch {
            alert = AppAlert(title: "Scan Failed", message: error.localizedDescription)
            scanStatusMessage = "Scan failed."
        }
        // Sorted once at the end: re-sorting per item made rows jump under the cursor.
        projects.sort { $0.size > $1.size }
        isScanning = false
        scanTask = nil
    }

    func generateRecommendation(strategy: String = "balanced", target: String = "10GB") async {
        guard !isScanning, !isRecommending else { return }
        isRecommending = true
        defer { isRecommending = false }
        do {
            var args = ["recommend"] + activeScanRoots + [
                "--cleanup", target, "--strategy", strategy,
                "--depth", "\(preferences.scanDepth)",
                "--max-risk", preferences.showHighRisk ? "high" : "medium",
                "--recent-days", "\(preferences.recentDays)"]
            if preferences.includeRecent {
                args.append("--include-recent")
            }
            try await bridge.run(arguments: args) { [weak self] event in
                self?.handle(event)
            }
        } catch {
            alert = AppAlert(title: "Recommendation Failed", message: error.localizedDescription)
        }
    }

    /// Every cleanup goes through the review sheet with the exact items that will be processed.
    func presentCleanup(mode: CleanupMode, items: [BridgeProject]) {
        let items = items.filter(\.isSelectable)
        guard !items.isEmpty else { return }
        cleanupMode = mode
        pendingCleanup = PendingCleanup(mode: mode, items: items)
    }

    func runSelectedCleanup(mode: CleanupMode) async {
        await runCleanup(items: selectedProjects, mode: mode)
    }

    func runCleanup(items: [BridgeProject], mode: CleanupMode) async {
        pendingCleanup = nil
        guard !items.isEmpty else { return }
        do {
            let plan = try writeTemporaryPlan(projects: items)
            defer { try? FileManager.default.removeItem(at: plan) }
            try await runApply(plan: plan, items: items, mode: mode)
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
            if selectedTrashBatchID == nil {
                selectedTrashBatchID = trashBatches.first?.batchId
            }
        } catch {
            alert = AppAlert(title: "Trash Refresh Failed", message: error.localizedDescription)
        }
    }

    func showTrashEntries(batch: TrashBatch) async {
        selectedTrashBatchID = batch.batchId
        do {
            try await bridge.run(arguments: ["trash", "show", "--batch", batch.batchId]) { [weak self] event in
                self?.handle(event)
            }
        } catch {
            alert = AppAlert(title: "Trash Details Failed", message: error.localizedDescription)
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

    func emptyTrash() async {
        for batch in trashBatches {
            await purge(batch: batch)
        }
        refreshVolume()
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

    func loadAuditRecords(runID: String?) async {
        auditItems = []
        guard let runID else { return }
        do {
            try await bridge.run(arguments: ["audit", "show", "--run", runID]) { [weak self] event in
                self?.handle(event)
            }
        } catch {
            alert = AppAlert(title: "Audit Details Failed", message: error.localizedDescription)
        }
    }

    func exportAuditCSV() async {
        let panel = NSSavePanel()
        panel.title = "Export Audit CSV"
        panel.nameFieldStringValue = "dev-cleaner-audit.csv"
        panel.allowedContentTypes = [.commaSeparatedText]

        guard panel.runModal() == .OK, let url = panel.url else { return }

        do {
            var csv = ""
            try await bridge.run(arguments: ["audit", "export", "--format", "csv"]) { event in
                if case .auditExport(let payload) = event {
                    csv = payload.string(path: ["content"]) ?? ""
                }
            }
            try csv.write(to: url, atomically: true, encoding: .utf8)
            alert = AppAlert(title: "Audit Exported", message: url.path)
        } catch {
            alert = AppAlert(title: "Export Failed", message: error.localizedDescription)
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

    func saveConfig(showAlert: Bool = true) async {
        guard var snapshot = configSnapshot else { return }
        snapshot.guiPreferences = preferences
        do {
            let data = try JSONEncoder.bridge.encode(snapshot)
            let url = FileManager.default.temporaryDirectory.appendingPathComponent("dev-cleaner-config-\(UUID().uuidString).json")
            try data.write(to: url)
            defer { try? FileManager.default.removeItem(at: url) }
            try await bridge.run(arguments: ["config", "save", "--input", url.path]) { [weak self] event in
                self?.handle(event)
            }
            if showAlert {
                alert = AppAlert(title: "Settings Saved", message: "Dev Cleaner preferences were updated.")
            }
        } catch {
            alert = AppAlert(title: "Save Failed", message: error.localizedDescription)
        }
    }

    private func runApply(plan: URL, items: [BridgeProject], mode: CleanupMode) async throws {
        isCleaning = true
        cleanupMode = mode
        cleanupProgress = 0
        cleanupLog.removeAll()
        cleanupIDsByCanonicalPath = Dictionary(
            items.map { (canonicalPath($0.cleanableDir), $0.id) },
            uniquingKeysWith: { first, _ in first }
        )
        cleanupTotalCount = items.count
        cleanupProcessedCount = 0
        cleanupSucceededCount = 0
        cleanupSkippedCount = 0
        cleanupFailedCount = 0
        cleanupCurrentPath = nil
        showCleanupSummary = false
        lastSummary = nil
        let cancel = FileManager.default.temporaryDirectory.appendingPathComponent("dev-cleaner-cancel-\(UUID().uuidString)")
        cancelFile = cancel

        // Same recency rules as the scan that showed these items, or apply would skip them as recent.
        var args = ["apply", plan.path, "--cancel-file", cancel.path, "--recent-days", "\(preferences.recentDays)"]
        if preferences.includeRecent {
            args.append("--include-recent")
        }
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

    /// Declares every active root: items from the second and later roots must not fail verification.
    var planScanRoot: String {
        commonAncestorPath(activeScanRoots)
    }

    private func writeTemporaryPlan(projects: [BridgeProject]) throws -> URL {
        let payload: [String: Any] = [
            "schema_version": 3,
            "tool_version": "mac",
            "created_at": ISO8601DateFormatter().string(from: Date()),
            "scan_root": planScanRoot,
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
            projects.append(project)
            if focusedProjectID == nil { focusedProjectID = project.id }
        case .scanProgress(let completed, let total):
            scanProgress = total == 0 ? 0 : Double(completed) / Double(total)
        case .scanFinished:
            scanProgress = 1
        case .recommendation(let payload):
            recommendation = RecommendationPreview(payload: payload)
        case .cleanupStarted(let totalCount, let totalBytes, let mode):
            cleanupTotalCount = totalCount
            cleanupProcessedCount = 0
            cleanupSucceededCount = 0
            cleanupSkippedCount = 0
            cleanupFailedCount = 0
            cleanupProgress = 0
            cleanupLog.append("> \(mode) started for \(totalCount) items, \(DevCleanerFormatters.bytes(totalBytes))")
        case .cleanupProject(let path, _):
            cleanupCurrentPath = path
            cleanupLog.append("> Processing \(DevCleanerFormatters.shortPath(path))")
        case .cleanupDryRun(let path, let action, _):
            cleanupSucceededCount += 1
            advanceCleanupProgress()
            cleanupLog.append("[dry-run] \(action) \(DevCleanerFormatters.shortPath(path))")
        case .cleanupSkipped(let path, let reason, _):
            cleanupSkippedCount += 1
            advanceCleanupProgress()
            cleanupLog.append("[skipped:\(reason)] \(DevCleanerFormatters.shortPath(path))")
        case .cleanupCompleted(let path, _):
            cleanupSucceededCount += 1
            advanceCleanupProgress()
            cleanupLog.append("[success] \(DevCleanerFormatters.shortPath(path))")
            let id = cleanupIDsByCanonicalPath[path] ?? path
            projects.removeAll { $0.id == id }
            selectedProjectIDs.remove(id)
            if focusedProjectID == id { focusedProjectID = nil }
        case .cleanupFailed(let path, let error):
            cleanupFailedCount += 1
            advanceCleanupProgress()
            cleanupLog.append("[failed] \(DevCleanerFormatters.shortPath(path)): \(error)")
        case .cleanupCancelled:
            cleanupLog.append("> Cleanup cancelled.")
        case .cleanupFinished(let payload):
            cleanupProgress = 1
            lastSummary = CleanupSummary(payload: payload, mode: cleanupMode)
            showCleanupSummary = true
            refreshVolume()
            Task {
                await purgeExpiredTrash()
                await refreshTrash()
                await refreshAudit()
            }
        case .trashList(let payload):
            trashRoot = payload.string(path: ["trash_root"])
            trashBatches = payload.decodePayloadArray(path: ["batches"], as: TrashBatch.self)
            if let selectedTrashBatchID, !trashBatches.contains(where: { $0.batchId == selectedTrashBatchID }) {
                self.selectedTrashBatchID = trashBatches.first?.batchId
            }
        case .trashEntries(let payload):
            trashEntries = payload.decodePayloadArray(path: ["entries"], as: TrashEntry.self)
        case .trashOperationFinished(let payload):
            reportTrashOperation(payload)
        case .auditList(let payload):
            auditRuns = payload.decodePayloadArray(path: ["runs"], as: AuditRun.self)
        case .auditRecords(let payload):
            if case .array(let records) = payload.value(at: ["records"]) {
                auditItems = JSONValue.array(records.filter { $0.string(path: ["type"]) == "item_action" })
                    .decodePayloadArray(path: [], as: AuditItemRecord.self)
            }
        case .configSnapshot(let snapshot):
            configSnapshot = snapshot
            preferences = snapshot.guiPreferences
        case .configSaved:
            break
        case .error(let message):
            alert = AppAlert(title: "Bridge Error", message: message)
        default:
            break
        }
    }

    /// Restores always report back; purges only when something could not be deleted.
    private func reportTrashOperation(_ payload: JSONValue) {
        var errors: [String] = []
        if case .array(let values) = payload.value(at: ["result", "errors"]) {
            errors = values.compactMap { if case .string(let text) = $0 { text } else { nil } }
        }
        switch payload.string(path: ["operation"]) {
        case "restore":
            let restored = payload.int(path: ["result", "restored_count"]) ?? 0
            let missed = (payload.int(path: ["result", "skipped_count"]) ?? 0) + (payload.int(path: ["result", "failed_count"]) ?? 0)
            if restored > 0, missed == 0 {
                alert = AppAlert(title: "Restored \(restored) Items", message: "Everything is back in its original folder. Scan again to see the items in Scan Results.")
            } else {
                alert = AppAlert(
                    title: restored + missed == 0 ? "Nothing Restored" : "Restored \(restored) of \(restored + missed) Items",
                    message: errors.isEmpty ? "Some items were no longer in the Trash." : errors.joined(separator: "\n")
                )
            }
        case "purge":
            if (payload.int(path: ["result", "failed_batches"]) ?? 0) > 0 {
                alert = AppAlert(title: "Purge Incomplete", message: errors.joined(separator: "\n"))
            }
        default:
            break
        }
    }

    private func advanceCleanupProgress() {
        cleanupProcessedCount = cleanupSucceededCount + cleanupSkippedCount + cleanupFailedCount
        cleanupProgress = cleanupTotalCount == 0 ? 0 : min(Double(cleanupProcessedCount) / Double(cleanupTotalCount), 1)
    }

    private func configArray(key: String) -> [String] {
        guard let value = configSnapshot?.config.value(at: [key]),
              case .array(let values) = value else {
            return []
        }
        return values.compactMap { value in
            if case .string(let string) = value { return string }
            return nil
        }
    }

    private func appendConfigValue(_ value: String, toArrayKey key: String) {
        guard var snapshot = configSnapshot else { return }
        var object: [String: JSONValue]
        if case .object(let existing) = snapshot.config {
            object = existing
        } else {
            object = [:]
        }
        var values = configArray(key: key)
        if !values.contains(value) {
            values.append(value)
        }
        object[key] = .array(values.map(JSONValue.string))
        snapshot.config = .object(object)
        configSnapshot = snapshot
    }

    private func removeConfigValue(_ value: String, fromArrayKey key: String) {
        guard var snapshot = configSnapshot else { return }
        var object: [String: JSONValue]
        if case .object(let existing) = snapshot.config {
            object = existing
        } else {
            object = [:]
        }
        object[key] = .array(configArray(key: key).filter { $0 != value }.map(JSONValue.string))
        snapshot.config = .object(object)
        configSnapshot = snapshot
    }
}

enum AuditFilter: String, CaseIterable, Identifiable {
    case all
    case last30Days
    case cleanup
    case restore

    var id: String { rawValue }
    var title: String {
        switch self {
        case .all: "All Actions"
        case .last30Days: "Last 30 Days"
        case .cleanup: "Cleanup"
        case .restore: "Restore"
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
    var mode: CleanupMode
    var cleanedCount: Int = 0
    var skippedCount: Int = 0
    var failedCount: Int = 0
    var bytesFreed: UInt64 = 0
    var bytesByCategory: [String: UInt64] = [:]
    var trashBatchID: String?
    var cancelled = false

    init(payload: JSONValue, mode: CleanupMode) {
        self.mode = mode
        if case .object(let categories) = payload.value(at: ["bytes_by_category"]) {
            for (category, value) in categories {
                if case .number(let bytes) = value { bytesByCategory[category] = UInt64(bytes) }
            }
        }
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

struct VolumeInfo: Equatable {
    var name: String
    var totalBytes: UInt64
    var availableBytes: UInt64

    var usedBytes: UInt64 { totalBytes > availableBytes ? totalBytes - availableBytes : 0 }

    static func forPath(_ path: String) -> VolumeInfo? {
        let keys: Set<URLResourceKey> = [.volumeNameKey, .volumeTotalCapacityKey, .volumeAvailableCapacityForImportantUsageKey, .volumeAvailableCapacityKey]
        guard let values = try? URL(fileURLWithPath: path).resourceValues(forKeys: keys),
              let total = values.volumeTotalCapacity, total > 0 else { return nil }
        let available = values.volumeAvailableCapacityForImportantUsage.map { UInt64(max($0, 0)) }
            ?? UInt64(max(values.volumeAvailableCapacity ?? 0, 0))
        return VolumeInfo(name: values.volumeName ?? "Disk", totalBytes: UInt64(total), availableBytes: available)
    }
}

/// Longest shared directory prefix of absolute paths ("/" when they only share the root).
func commonAncestorPath(_ paths: [String]) -> String {
    let split = paths.map { URL(fileURLWithPath: $0).standardizedFileURL.pathComponents }
    guard var prefix = split.first else { return "/" }
    for components in split.dropFirst() {
        var count = 0
        while count < min(prefix.count, components.count), prefix[count] == components[count] {
            count += 1
        }
        prefix = Array(prefix.prefix(count))
    }
    guard prefix.count > 1 else { return "/" }
    return "/" + prefix.dropFirst().joined(separator: "/")
}

/// realpath(3) keeps `/private/tmp` (URL.resolvingSymlinksInPath strips it), matching Rust canonicalize.
private func canonicalPath(_ path: String) -> String {
    guard let resolved = realpath(path, nil) else { return path }
    defer { free(resolved) }
    return String(cString: resolved)
}

private func normalizedScanRootPath(_ path: String) -> String {
    let fallback = FileManager.default.homeDirectoryForCurrentUser.path
    let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return fallback }
    return (trimmed as NSString).expandingTildeInPath
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
