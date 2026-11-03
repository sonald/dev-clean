import XCTest
@testable import DevCleanerMac

final class BridgeClientTests: XCTestCase {
    func testDecodesReadyScalarEvent() throws {
        let event = try BridgeEventDecoder.decode(Data(#"{"type":"ready","version":"1.2.3"}"#.utf8))
        guard case .ready(let version) = event else {
            return XCTFail("expected ready")
        }
        XCTAssertEqual(version, "1.2.3")
    }

    func testDecodesErrorScalarEvent() throws {
        let event = try BridgeEventDecoder.decode(Data(#"{"type":"error","message":"scan failed"}"#.utf8))
        guard case .error(let message) = event else {
            return XCTFail("expected error")
        }
        XCTAssertEqual(message, "scan failed")
    }

    func testDecodesConfigSavedScalarEvent() throws {
        let event = try BridgeEventDecoder.decode(Data(#"{"type":"config_saved","path":"/tmp/dev-cleaner.toml"}"#.utf8))
        guard case .configSaved(let path) = event else {
            return XCTFail("expected config saved")
        }
        XCTAssertEqual(path, "/tmp/dev-cleaner.toml")
    }

    func testDecodesCleanupCancelledScalarEvent() throws {
        let event = try BridgeEventDecoder.decode(Data(#"{"type":"cleanup_cancelled","remaining":3}"#.utf8))
        guard case .cleanupCancelled(let remaining) = event else {
            return XCTFail("expected cleanup cancelled")
        }
        XCTAssertEqual(remaining, 3)
    }

    func testKnownEventDecodeFailureThrows() {
        XCTAssertThrowsError(try BridgeEventDecoder.decode(Data(#"{"type":"ready"}"#.utf8)))
    }

    func testGuiPreferencesDefaultScanRootPathUsesHome() {
        XCTAssertEqual(GuiPreferences().scanRootPath, FileManager.default.homeDirectoryForCurrentUser.path)
        XCTAssertEqual(GuiPreferences().scanRoots, [FileManager.default.homeDirectoryForCurrentUser.path])
    }

    func testGuiPreferencesDecodesNewSettingsWithDefaults() throws {
        let json = #"{"scan_root_path":"/tmp/primary","scan_roots":["/tmp/primary","/tmp/secondary"],"follow_symlinks":true,"trash_retention_days":45}"#
        let preferences = try JSONDecoder.bridge.decode(GuiPreferences.self, from: Data(json.utf8))

        XCTAssertEqual(preferences.scanRootPath, "/tmp/primary")
        XCTAssertEqual(preferences.scanRoots, ["/tmp/primary", "/tmp/secondary"])
        XCTAssertEqual(preferences.trashRetentionDays, 45)
        XCTAssertEqual(preferences.scanDepth, 4)
        XCTAssertEqual(preferences.recentDays, 7)
        XCTAssertFalse(preferences.showHighRisk)
        XCTAssertFalse(preferences.includeRecent)
        XCTAssertTrue(preferences.autoGcTrash)
    }

    func testCommonAncestorPath() {
        XCTAssertEqual(commonAncestorPath(["/Users/a/work", "/Users/a/Documents/x"]), "/Users/a")
        XCTAssertEqual(commonAncestorPath(["/Users/a/work"]), "/Users/a/work")
        XCTAssertEqual(commonAncestorPath(["/Volumes/SSD/code", "/Users/a"]), "/")
        XCTAssertEqual(commonAncestorPath(["/tmp/ab", "/tmp/a"]), "/tmp")
    }

    func testPlanScanRootCoversEveryRoot() async {
        let model = await AppModel(bridge: RecordingBridgeRunner())
        await MainActor.run {
            model.preferences.scanRoots = ["/tmp/dev-clean-a/one", "/tmp/dev-clean-b"]
            XCTAssertEqual(model.planScanRoot, "/tmp")
        }
    }

    func testScanArgumentsFollowPreferences() async {
        let bridge = RecordingBridgeRunner()
        let model = await AppModel(bridge: bridge)
        await MainActor.run {
            model.preferences.scanRoots = ["/tmp/dev-clean-root"]
            model.preferences.scanDepth = 6
            model.preferences.recentDays = 3
            model.preferences.includeRecent = true
            model.startSmartScan()
        }
        let didRecordScan = await waitUntil { bridge.recordedArguments.count == 1 }
        XCTAssertTrue(didRecordScan)
        XCTAssertEqual(bridge.recordedArguments.last, [
            "scan", "/tmp/dev-clean-root", "--depth", "6", "--max-risk", "high",
            "--recent-days", "3", "--include-recent"
        ])
    }

    func testHighRiskAndInUseItemsAreNotOffered() async {
        let model = await AppModel(bridge: RecordingBridgeRunner())
        let low = makeProject(dir: "/tmp/app/target", size: 10)
        let high = makeProject(dir: "/tmp/app/node_modules", size: 100, risk: "high")
        let inUse = makeProject(dir: "/tmp/app/.venv", size: 1000, inUse: true)
        await MainActor.run {
            model.projects = [low, high, inUse]
            model.selectedProjectIDs = [low.id, high.id]
            model.toggleSelection(inUse)

            XCTAssertEqual(model.visibleProjects.map(\.id), [low.id, inUse.id])
            XCTAssertEqual(model.selectedProjects.map(\.id), [low.id])
            XCTAssertEqual(model.totalCleanableBytes, 10)
            XCTAssertEqual(model.hiddenHighRiskProjects.map(\.id), [high.id])

            model.preferences.showHighRisk = true
            XCTAssertEqual(model.selectedProjects.map(\.id), [low.id, high.id])
            XCTAssertEqual(model.totalCleanableBytes, 110)
        }
    }

    func testPresentCleanupReviewsExactItems() async {
        let model = await AppModel(bridge: RecordingBridgeRunner())
        let focused = makeProject(dir: "/tmp/app/target")
        let other = makeProject(dir: "/tmp/other/target")
        await MainActor.run {
            model.projects = [focused, other]
            model.selectedProjectIDs = [other.id]
            model.presentCleanup(mode: .trash, items: [focused])
            XCTAssertEqual(model.pendingCleanup?.items.map(\.id), [focused.id])
            XCTAssertEqual(model.pendingCleanup?.mode, .trash)
        }
    }

    func testCompletedItemsLeaveResultsAndSummaryKeepsMode() async {
        let project = makeProject(dir: "/tmp/dev-clean-missing/target", size: 42)
        let payload = JSONValue.object([
            "cleaned_count": .number(1),
            "bytes_freed": .number(42),
            "bytes_by_category": .object(["build": .number(42)]),
            "skipped_count": .number(1)
        ])
        let bridge = RecordingBridgeRunner(events: [
            .cleanupStarted(totalCount: 2, totalBytes: 50, mode: "dry_run"),
            .cleanupCompleted(path: project.cleanableDir, size: 42),
            .cleanupFinished(payload)
        ])
        let model = await AppModel(bridge: bridge)
        await MainActor.run {
            model.projects = [project]
            model.selectedProjectIDs = [project.id]
        }
        await model.runSelectedCleanup(mode: .dryRun)

        await MainActor.run {
            XCTAssertTrue(model.projects.isEmpty)
            XCTAssertTrue(model.selectedProjectIDs.isEmpty)
            XCTAssertEqual(model.lastSummary?.mode, .dryRun)
            XCTAssertEqual(model.lastSummary?.bytesByCategory["build"], 42)
            XCTAssertEqual(model.lastSummary?.skippedCount, 1)
            XCTAssertEqual(model.lastSummary?.headline, "Dry Run Complete")
        }
    }

    func testAuditFiltersUseDatesCommandsAndModes() async {
        let model = await AppModel(bridge: RecordingBridgeRunner())
        let recent = ISO8601DateFormatter().string(from: Date())
        let old = ISO8601DateFormatter().string(from: Date().addingTimeInterval(-40 * 24 * 60 * 60))
        let runs = [
            AuditRun(runId: "a", command: "apply", startedAt: recent, finishedAt: recent, cleaned: 1, skipped: 0, failed: 0, freedBytes: 100, mode: "dry_run"),
            AuditRun(runId: "b", command: "apply", startedAt: old, finishedAt: old, cleaned: 1, skipped: 0, failed: 0, freedBytes: 200, mode: "trash"),
            AuditRun(runId: "c", command: "undo", startedAt: recent, finishedAt: recent, cleaned: 2, skipped: 0, failed: 0, freedBytes: 0, mode: "restore"),
            AuditRun(runId: "d", command: "trash", startedAt: recent, finishedAt: recent, cleaned: 3, skipped: 0, failed: 0, freedBytes: 300, mode: "purge")
        ]
        await MainActor.run {
            model.auditRuns = runs
            model.auditFilter = .last30Days
            XCTAssertEqual(model.filteredAuditRuns.map(\.runId), ["a", "c", "d"])
            model.auditFilter = .restore
            XCTAssertEqual(model.filteredAuditRuns.map(\.runId), ["c"])
            model.auditFilter = .cleanup
            XCTAssertEqual(model.filteredAuditRuns.map(\.runId), ["a", "b"])
        }
        XCTAssertEqual(runs.map(\.reclaimedBytes), [0, 0, 0, 300])
        XCTAssertEqual(runs[0].resultLabel, "Would free " + DevCleanerFormatters.bytes(100))
    }

    func testBridgeClientRunPropagatesDecodeFailure() async throws {
        let helperDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("dev-cleaner-helper-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: helperDirectory, withIntermediateDirectories: true)
        let helperURL = helperDirectory.appendingPathComponent("helper.sh")
        let script = """
        #!/bin/sh
        printf '%s\\n' '{"type":"ready"}'
        """
        try script.write(to: helperURL, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: helperURL.path)
        defer { try? FileManager.default.removeItem(at: helperDirectory) }

        let client = try BridgeClient(helperURL: helperURL)
        do {
            try await client.run(arguments: []) { _ in
                XCTFail("malformed helper event should not be delivered")
            }
            XCTFail("expected decode failure")
        } catch BridgeClientError.decodeFailed(let message) {
            XCTAssertTrue(message.contains("Failed to decode helper event"))
        } catch {
            XCTFail("expected decodeFailed, got \(error)")
        }
    }

    func testBridgeClientRunTerminatesOnCancellation() async throws {
        let helperDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("dev-cleaner-helper-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: helperDirectory, withIntermediateDirectories: true)
        let helperURL = helperDirectory.appendingPathComponent("helper.sh")
        let script = """
        #!/bin/sh
        printf '%s\\n' '{"type":"ready","version":"1.2.3"}'
        while true; do sleep 1; done
        """
        try script.write(to: helperURL, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: helperURL.path)
        defer { try? FileManager.default.removeItem(at: helperDirectory) }

        let ready = expectation(description: "helper emitted ready")
        let client = try BridgeClient(helperURL: helperURL)
        let task = Task {
            try await client.run(arguments: []) { event in
                if case .ready = event {
                    ready.fulfill()
                }
            }
        }

        await fulfillment(of: [ready], timeout: 2)
        task.cancel()

        do {
            try await task.value
            XCTFail("expected cancellation")
        } catch is CancellationError {
            // Expected.
        } catch {
            XCTFail("expected CancellationError, got \(error)")
        }
    }

    func testDecodesScanItemEvent() throws {
        let json = """
        {
          "type": "scan_item",
          "project": {
            "root": "/tmp/app",
            "project_type": "Rust",
            "category": "build",
            "risk_level": "medium",
            "confidence": "high",
            "cleanable_dir": "/tmp/app/target",
            "size": 42,
            "size_calculated": true,
            "last_modified": "2026-04-28T00:00:00Z",
            "in_use": false,
            "protected": false,
            "recent": false
          }
        }
        """
        let event = try BridgeEventDecoder.decode(Data(json.utf8))
        guard case .scanItem(let project) = event else {
            return XCTFail("expected scan item")
        }
        XCTAssertEqual(project.displayName, "target")
        XCTAssertEqual(project.size, 42)
        XCTAssertEqual(project.categoryLabel, "Build Cache")
    }

    func testAppModelSelectionTotals() async {
        let model = await AppModel(bridge: RecordingBridgeRunner())
        let project = makeProject(size: 1024)
        await MainActor.run {
            model.projects = [project]
            model.selectedProjectIDs = [project.id]
            XCTAssertEqual(model.selectedBytes, 1024)
            XCTAssertEqual(model.selectedProjects.count, 1)
        }
    }

    func testSmartScanUsesSelectedRoot() async {
        let bridge = RecordingBridgeRunner()
        let model = await AppModel(bridge: bridge)

        await MainActor.run {
            model.setScanRoot(path: "/tmp/dev-clean-root", persist: false)
            model.startSmartScan()
        }

        let didRecordScan = await waitUntil { bridge.recordedArguments.count == 1 }
        XCTAssertTrue(didRecordScan)
        let arguments = bridge.recordedArguments.last ?? []
        XCTAssertEqual(Array(arguments.prefix(2)), ["scan", "/tmp/dev-clean-root"])
    }

    func testSmartScanRunsAllConfiguredRoots() async {
        let bridge = RecordingBridgeRunner()
        let model = await AppModel(bridge: bridge)

        await MainActor.run {
            model.preferences.scanRoots = ["/tmp/dev-clean-a", "/tmp/dev-clean-b"]
            model.startSmartScan()
        }

        let didRecordScans = await waitUntil { bridge.recordedArguments.count == 2 }
        XCTAssertTrue(didRecordScans)
        XCTAssertEqual(bridge.recordedArguments.map { Array($0.prefix(2)) }, [
            ["scan", "/tmp/dev-clean-a"],
            ["scan", "/tmp/dev-clean-b"]
        ])
    }

    func testProtectionRulesMutateConfigSnapshot() async {
        let model = await AppModel(bridge: RecordingBridgeRunner())

        await MainActor.run {
            model.configSnapshot = ConfigSnapshot(configPath: "/tmp/config.toml", config: .object(["keep_paths": .array([.string("/tmp/a")])]), guiPreferences: GuiPreferences())
            XCTAssertEqual(model.protectedPaths, ["/tmp/a"])
            model.removeProtectedPath("/tmp/a")
            XCTAssertEqual(model.protectedPaths, [])
        }
    }

    func testRecommendationUsesSelectedRoot() async {
        let bridge = RecordingBridgeRunner()
        let model = await AppModel(bridge: bridge)

        await MainActor.run {
            model.setScanRoot(path: "/tmp/dev-clean-recommend", persist: false)
        }
        await model.generateRecommendation()

        let arguments = bridge.recordedArguments.last ?? []
        XCTAssertEqual(Array(arguments.prefix(2)), ["recommend", "/tmp/dev-clean-recommend"])
    }

    func testStopScanCancelsTaskAndKeepsPartialResults() async {
        let project = makeProject()
        let bridge = RecordingBridgeRunner(events: [.scanItem(project)], waitUntilCancelled: true)
        let model = await AppModel(bridge: bridge)

        await MainActor.run {
            model.startSmartScan()
        }

        let didStartScan = await waitUntil {
            await MainActor.run {
                model.isScanning && model.projects.count == 1
            }
        }
        XCTAssertTrue(didStartScan)

        await MainActor.run {
            model.stopScan()
        }

        let didStopScan = await waitUntil {
            await MainActor.run { !model.isScanning }
        }
        XCTAssertTrue(didStopScan)

        await MainActor.run {
            XCTAssertEqual(model.projects.map(\.id), [project.id])
            XCTAssertNil(model.alert)
            XCTAssertTrue(model.scanStatusMessage.contains("stopped"))
        }
    }

    func testSelectedTrashCleanupDoesNotForceAndUsesTrash() async {
        let bridge = RecordingBridgeRunner()
        let model = await AppModel(bridge: bridge)
        let project = makeProject()

        await MainActor.run {
            model.projects = [project]
            model.selectedProjectIDs = [project.id]
        }
        await model.runSelectedCleanup(mode: .trash)

        let arguments = bridge.recordedArguments.last ?? []
        XCTAssertEqual(arguments.first, "apply")
        XCTAssertFalse(arguments.contains("--force"))
        XCTAssertTrue(arguments.contains("--trash"))
        XCTAssertTrue(arguments.contains("--cancel-file"))
    }

    func testSelectedPermanentCleanupUsesExplicitDangerousFlag() async {
        let bridge = RecordingBridgeRunner()
        let model = await AppModel(bridge: bridge)
        let project = makeProject()

        await MainActor.run {
            model.projects = [project]
            model.selectedProjectIDs = [project.id]
        }
        await model.runSelectedCleanup(mode: .permanentDelete)

        let arguments = bridge.recordedArguments.last ?? []
        XCTAssertEqual(arguments.first, "apply")
        XCTAssertTrue(arguments.contains("--permanent-delete"))
        XCTAssertFalse(arguments.contains("--trash"))
        XCTAssertFalse(arguments.contains("--force"))
    }

    func testRecommendationCoversEveryScanRoot() async {
        let bridge = RecordingBridgeRunner()
        let model = await AppModel(bridge: bridge)
        await MainActor.run {
            model.preferences.scanRoots = ["/tmp/dev-clean-a", "/tmp/dev-clean-b"]
            model.preferences.scanDepth = 6
        }
        await model.generateRecommendation()

        let arguments = bridge.recordedArguments.last ?? []
        XCTAssertEqual(Array(arguments.prefix(3)), ["recommend", "/tmp/dev-clean-a", "/tmp/dev-clean-b"])
        XCTAssertEqual(arguments.firstIndex(of: "--depth").map { arguments[$0 + 1] }, "6")
        let finished = await MainActor.run { !model.isRecommending }
        XCTAssertTrue(finished)
    }

    func testScanKeepsDiscoveryOrderUntilItEnds() async {
        let small = makeProject(dir: "/tmp/a/target", size: 1)
        let big = makeProject(dir: "/tmp/b/target", size: 99)
        let bridge = RecordingBridgeRunner(events: [.scanItem(small), .scanItem(big)], waitUntilCancelled: true)
        let model = await AppModel(bridge: bridge)
        await MainActor.run { model.startSmartScan() }

        let streamed = await waitUntil { await MainActor.run { model.projects.count == 2 } }
        XCTAssertTrue(streamed)
        await MainActor.run {
            XCTAssertEqual(model.projects.map(\.size), [1, 99], "rows must not jump while the scan runs")
            model.stopScan()
        }
        let sorted = await waitUntil { await MainActor.run { !model.isScanning && model.projects.first?.size == 99 } }
        XCTAssertTrue(sorted)
    }

    func testApplyUsesTheRecencyRulesOfTheScan() async {
        let bridge = RecordingBridgeRunner()
        let model = await AppModel(bridge: bridge)
        await MainActor.run {
            model.preferences.recentDays = 3
            model.preferences.includeRecent = true
        }
        await model.runCleanup(items: [makeProject()], mode: .trash)

        let arguments = bridge.recordedArguments.first { $0.first == "apply" } ?? []
        XCTAssertEqual(arguments.firstIndex(of: "--recent-days").map { arguments[$0 + 1] }, "3")
        XCTAssertTrue(arguments.contains("--include-recent"))
    }

    func testCleanupDoesNotSwitchSections() async {
        let project = makeProject()
        let bridge = RecordingBridgeRunner(events: [
            .cleanupStarted(totalCount: 1, totalBytes: 42, mode: "trash"),
            .cleanupFinished(.object(["cleaned_count": .number(1)]))
        ])
        let model = await AppModel(bridge: bridge)
        await MainActor.run { model.selectedSection = .recommendations }
        await model.runCleanup(items: [project], mode: .trash)

        await MainActor.run {
            XCTAssertEqual(model.selectedSection, .recommendations)
            XCTAssertTrue(model.showCleanupSummary)
        }
    }

    func testSelectVisibleFollowsFiltersAndSkipsBlockedItems() async {
        let model = await AppModel(bridge: RecordingBridgeRunner())
        let cache = { var project = makeProject(dir: "/tmp/c/cache"); project.category = "cache"; return project }()
        let build = makeProject(dir: "/tmp/a/target")
        let inUse = makeProject(dir: "/tmp/b/target", inUse: true)
        let hidden = makeProject(dir: "/tmp/d/target", risk: "high")

        await MainActor.run {
            model.projects = [cache, build, inUse, hidden]
            model.categoryFilter = "build"
            model.selectVisible()
            XCTAssertEqual(model.selectedProjectIDs, [build.id])
            model.clearSelection()
            XCTAssertTrue(model.selectedProjectIDs.isEmpty)
        }
    }

    func testRestoreAndPurgeReportTheirOutcome() async {
        let batch = TrashBatch(batchId: "b1", createdAt: Date(), entriesCount: 2, totalSize: 10)
        let restore = JSONValue.object([
            "operation": .string("restore"),
            "result": .object([
                "restored_count": .number(1), "skipped_count": .number(1), "failed_count": .number(0),
                "errors": .array([.string("Restore target already exists (use --force to override): /tmp/x")])
            ])
        ])
        let bridge = RecordingBridgeRunner(events: [.trashOperationFinished(restore)])
        let model = await AppModel(bridge: bridge)
        await model.restore(batch: batch)
        await MainActor.run {
            XCTAssertEqual(model.alert?.title, "Restored 1 of 2 Items")
            XCTAssertTrue(model.alert?.message.contains("already exists") == true)
            model.alert = nil
        }

        bridge.events = [.trashOperationFinished(.object([
            "operation": .string("purge"),
            "result": .object(["failed_batches": .number(1), "errors": .array([.string("Volume is not mounted: /Volumes/X")])])
        ]))]
        await model.purge(batch: batch)
        let purgeAlert = await MainActor.run { model.alert }
        XCTAssertEqual(purgeAlert?.title, "Purge Incomplete")
        XCTAssertTrue(purgeAlert?.message.contains("not mounted") == true)
    }

    func testAuditDetailsListOnlyItemActions() async {
        let records = JSONValue.object(["run": .string("r1"), "records": .array([
            .object(["type": .string("run_started"), "run_id": .string("r1"), "command": .string("apply")]),
            .object(["type": .string("item_action"), "run_id": .string("r1"), "command": .string("apply"),
                     "path": .string("/tmp/a/target"), "action": .string("trash"), "result": .string("completed"),
                     "bytes": .number(42), "reason": .null, "ts": .string("2026-11-03T04:16:48Z")]),
            .object(["type": .string("run_finished"), "run_id": .string("r1"), "cleaned": .number(1)])
        ])])
        let bridge = RecordingBridgeRunner(events: [.auditRecords(records)])
        let model = await AppModel(bridge: bridge)
        await model.loadAuditRecords(runID: "r1")

        XCTAssertEqual(bridge.recordedArguments.last, ["audit", "show", "--run", "r1"])
        await MainActor.run {
            XCTAssertEqual(model.auditItems.map(\.path), ["/tmp/a/target"])
            XCTAssertEqual(model.auditItems.first?.bytes, 42)
        }
    }

    func testMatchedRuleSurvivesPlanEncoding() throws {
        let json = #"{"root":"/tmp/llm","project_type":"Python","cleanable_dir":"/tmp/llm/gpt2","size":1,"in_use":false,"matched_rule":{"source":"gitignore","pattern":"gpt2"}}"#
        let project = try JSONDecoder.bridge.decode(BridgeProject.self, from: Data(json.utf8))
        XCTAssertEqual(project.ruleLabel, ".gitignore pattern “gpt2”")
        let encoded = String(decoding: try JSONEncoder.bridge.encode(project), as: UTF8.self)
        XCTAssertTrue(encoded.contains(#""matched_rule":{"#), encoded)
    }

    private func makeProject(dir: String = "/tmp/app/target", size: UInt64 = 42, risk: String = "medium", inUse: Bool = false) -> BridgeProject {
        BridgeProject(
            root: (dir as NSString).deletingLastPathComponent,
            projectType: "Rust",
            projectName: nil,
            category: "build",
            riskLevel: risk,
            confidence: "high",
            cleanableDir: dir,
            size: size,
            sizeCalculated: true,
            lastModified: nil,
            inUse: inUse,
            protected: false,
            protectedBy: nil,
            recent: false,
            selectionReason: nil,
            skipReason: nil
        )
    }
}

private final class RecordingBridgeRunner: BridgeRunning {
    private(set) var recordedArguments: [[String]] = []
    var events: [BridgeEvent]
    var waitUntilCancelled: Bool

    init(events: [BridgeEvent] = [], waitUntilCancelled: Bool = false) {
        self.events = events
        self.waitUntilCancelled = waitUntilCancelled
    }

    func run(arguments: [String], onEvent: @escaping @MainActor (BridgeEvent) -> Void) async throws {
        recordedArguments.append(arguments)
        // Replay once: follow-up calls (trash/audit refresh after cleanup) must not loop.
        let pending = events
        events = []
        for event in pending {
            await onEvent(event)
        }
        if waitUntilCancelled {
            while !Task.isCancelled {
                try await Task.sleep(nanoseconds: 10_000_000)
            }
            throw CancellationError()
        }
    }
}

private func waitUntil(
    timeout: TimeInterval = 1,
    condition: @escaping () async -> Bool
) async -> Bool {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
        if await condition() {
            return true
        }
        try? await Task.sleep(nanoseconds: 10_000_000)
    }
    return await condition()
}
