import Foundation

struct BridgeProject: Codable, Identifiable, Hashable {
    var root: String
    var projectType: String
    var projectName: String?
    var category: String?
    var riskLevel: String?
    var confidence: String?
    var cleanableDir: String
    var size: UInt64
    var sizeCalculated: Bool?
    var lastModified: Date?
    var inUse: Bool
    var protected: Bool?
    var protectedBy: String?
    var recent: Bool?
    var selectionReason: String?
    var skipReason: String?

    var id: String { cleanableDir }
    var displayName: String {
        URL(fileURLWithPath: cleanableDir).lastPathComponent
    }
    var displayType: String {
        projectName ?? projectType.replacingOccurrences(of: "NodeJs", with: "Node.js")
    }
    var categoryLabel: String {
        switch category {
        case "deps": "Dependency Cache"
        case "build": "Build Cache"
        case "cache": "Cache"
        default: "Cleanable"
        }
    }
    var riskLabel: String {
        (riskLevel ?? "medium").capitalized + " Risk"
    }

    enum CodingKeys: String, CodingKey {
        case root
        case projectType = "project_type"
        case projectName = "project_name"
        case category
        case riskLevel = "risk_level"
        case confidence
        case cleanableDir = "cleanable_dir"
        case size
        case sizeCalculated = "size_calculated"
        case lastModified = "last_modified"
        case inUse = "in_use"
        case protected
        case protectedBy = "protected_by"
        case recent
        case selectionReason = "selection_reason"
        case skipReason = "skip_reason"
    }
}

struct TrashBatch: Codable, Identifiable, Hashable {
    var batchId: String
    var createdAt: Date
    var entriesCount: Int
    var totalSize: UInt64

    var id: String { batchId }

    enum CodingKeys: String, CodingKey {
        case batchId = "batch_id"
        case createdAt = "created_at"
        case entriesCount = "entries_count"
        case totalSize = "total_size"
    }
}

struct TrashEntry: Codable, Identifiable, Hashable {
    var batchId: String
    var createdAt: Date
    var originalPath: String
    var trashedPath: String
    var size: UInt64

    var id: String { originalPath + batchId }

    enum CodingKeys: String, CodingKey {
        case batchId = "batch_id"
        case createdAt = "created_at"
        case originalPath = "original_path"
        case trashedPath = "trashed_path"
        case size
    }
}

struct AuditRun: Codable, Identifiable, Hashable {
    var runId: String
    var command: String
    var startedAt: String?
    var finishedAt: String?
    var cleaned: Int
    var skipped: Int
    var failed: Int
    var freedBytes: UInt64

    var id: String { runId }

    enum CodingKeys: String, CodingKey {
        case runId = "run_id"
        case command
        case startedAt = "started_at"
        case finishedAt = "finished_at"
        case cleaned
        case skipped
        case failed
        case freedBytes = "freed_bytes"
    }
}

struct GuiPreferences: Codable, Equatable {
    var appearance: String = "dark"
    var scanRootPath: String = FileManager.default.homeDirectoryForCurrentUser.path
    var launchAtLogin: Bool = false
    var showMenubarIcon: Bool = true
    var alertsEnabled: Bool = false
    var notificationThresholdGb: UInt64 = 1
    var trashRetentionDays: Int = 30
    var trashLimitGb: UInt64 = 10

    enum CodingKeys: String, CodingKey {
        case appearance
        case scanRootPath = "scan_root_path"
        case launchAtLogin = "launch_at_login"
        case showMenubarIcon = "show_menubar_icon"
        case alertsEnabled = "alerts_enabled"
        case notificationThresholdGb = "notification_threshold_gb"
        case trashRetentionDays = "trash_retention_days"
        case trashLimitGb = "trash_limit_gb"
    }

    init() {}

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        appearance = try container.decodeIfPresent(String.self, forKey: .appearance) ?? "dark"
        scanRootPath = try container.decodeIfPresent(String.self, forKey: .scanRootPath)
            ?? FileManager.default.homeDirectoryForCurrentUser.path
        launchAtLogin = try container.decodeIfPresent(Bool.self, forKey: .launchAtLogin) ?? false
        showMenubarIcon = try container.decodeIfPresent(Bool.self, forKey: .showMenubarIcon) ?? true
        alertsEnabled = try container.decodeIfPresent(Bool.self, forKey: .alertsEnabled) ?? false
        notificationThresholdGb = try container.decodeIfPresent(UInt64.self, forKey: .notificationThresholdGb) ?? 1
        trashRetentionDays = try container.decodeIfPresent(Int.self, forKey: .trashRetentionDays) ?? 30
        trashLimitGb = try container.decodeIfPresent(UInt64.self, forKey: .trashLimitGb) ?? 10
    }
}

struct ConfigSnapshot: Codable {
    var configPath: String
    var config: JSONValue
    var guiPreferences: GuiPreferences

    enum CodingKeys: String, CodingKey {
        case configPath = "config_path"
        case config
        case guiPreferences = "gui_preferences"
    }
}

enum CleanupMode: String {
    case dryRun = "dry_run"
    case trash
    case permanentDelete = "permanent_delete"

    var title: String {
        switch self {
        case .dryRun: "Dry Run"
        case .trash: "Clean with Trash"
        case .permanentDelete: "Delete Permanently"
        }
    }
}

enum AppSection: String, CaseIterable, Identifiable {
    case dashboard
    case scanResults
    case recommendations
    case trash
    case history
    case settings

    var id: String { rawValue }
    var title: String {
        switch self {
        case .dashboard: "Dashboard"
        case .scanResults: "Scan Results"
        case .recommendations: "Recommendations"
        case .trash: "Trash"
        case .history: "History"
        case .settings: "Settings"
        }
    }
    var systemImage: String {
        switch self {
        case .dashboard: "square.grid.2x2"
        case .scanResults: "chart.bar.doc.horizontal"
        case .recommendations: "sparkles"
        case .trash: "trash"
        case .history: "clock.arrow.circlepath"
        case .settings: "gearshape"
        }
    }
}

enum JSONValue: Codable, Equatable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case object([String: JSONValue])
    case array([JSONValue])
    case null

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Double.self) {
            self = .number(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([String: JSONValue].self) {
            self = .object(value)
        } else {
            self = .array(try container.decode([JSONValue].self))
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .string(let value): try container.encode(value)
        case .number(let value): try container.encode(value)
        case .bool(let value): try container.encode(value)
        case .object(let value): try container.encode(value)
        case .array(let value): try container.encode(value)
        case .null: try container.encodeNil()
        }
    }
}
