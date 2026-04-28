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

    private func makeProject(size: UInt64 = 42) -> BridgeProject {
        BridgeProject(
            root: "/tmp/app",
            projectType: "Rust",
            projectName: nil,
            category: "build",
            riskLevel: "medium",
            confidence: "high",
            cleanableDir: "/tmp/app/target",
            size: size,
            sizeCalculated: true,
            lastModified: nil,
            inUse: false,
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

    func run(arguments: [String], onEvent: @escaping @MainActor (BridgeEvent) -> Void) async throws {
        recordedArguments.append(arguments)
    }
}
