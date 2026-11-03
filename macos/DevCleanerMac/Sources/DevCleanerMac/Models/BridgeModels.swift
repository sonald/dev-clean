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
    /// Kept so plans written by the app carry the same rule the helper reported.
    var matchedRule: MatchedRule? = nil

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
    var isHighRisk: Bool { riskLevel == "high" }
    var ruleLabel: String? {
        guard let rule = matchedRule else { return nil }
        switch rule.source {
        case "gitignore": return ".gitignore pattern “\(rule.pattern)”"
        case "custom": return "custom rule “\(rule.name ?? rule.pattern)”"
        case "heuristic": return "heuristic “\(rule.pattern)”"
        default: return "built-in rule “\(rule.pattern)”"
        }
    }
    /// In-use items are always skipped by the helper unless forced, so the GUI never offers them.
    var isSelectable: Bool { !inUse && protected != true }

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
        case matchedRule = "matched_rule"
    }
}

struct MatchedRule: Codable, Hashable {
    var source: String
    var pattern: String
    var name: String?
}

/// One `item_action` line of an audit run.
struct AuditItemRecord: Decodable, Identifiable, Hashable {
    var path: String
    var action: String
    var result: String
    var bytes: UInt64?
    var reason: String?
    var ts: String?

    var id: String { path + action + (ts ?? "") }
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
    var mode: String?

    var id: String { runId }

    /// Bytes that actually left the disk: permanent deletes and trash purges.
    var reclaimedBytes: UInt64 {
        switch mode {
        case "permanent_delete", "purge", "gc": freedBytes
        case nil: command == "clean" || command == "apply" ? 0 : freedBytes
        default: 0
        }
    }

    var startedDate: Date? {
        startedAt.flatMap { AuditRun.parseDate($0) }
    }

    static func parseDate(_ value: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: value) { return date }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: value)
    }

    enum CodingKeys: String, CodingKey {
        case runId = "run_id"
        case command
        case startedAt = "started_at"
        case finishedAt = "finished_at"
        case cleaned
        case skipped
        case failed
        case freedBytes = "freed_bytes"
        case mode
    }
}

/// Mirrors `GuiPreferences` in `src/bridge.rs`; fields missing on either side are dropped on save.
struct GuiPreferences: Codable, Equatable {
    var scanRootPath: String = FileManager.default.homeDirectoryForCurrentUser.path
    var scanRoots: [String] = [FileManager.default.homeDirectoryForCurrentUser.path]
    var scanDepth: Int = 4
    var recentDays: Int = 7
    var includeRecent: Bool = false
    var showHighRisk: Bool = false
    var trashRetentionDays: Int = 30
    var trashLimitGb: UInt64 = 10
    var autoGcTrash: Bool = true

    enum CodingKeys: String, CodingKey {
        case scanRootPath = "scan_root_path"
        case scanRoots = "scan_roots"
        case scanDepth = "scan_depth"
        case recentDays = "recent_days"
        case includeRecent = "include_recent"
        case showHighRisk = "show_high_risk"
        case trashRetentionDays = "trash_retention_days"
        case trashLimitGb = "trash_limit_gb"
        case autoGcTrash = "auto_gc_trash"
    }

    init() {}

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let defaults = GuiPreferences()
        scanRootPath = try container.decodeIfPresent(String.self, forKey: .scanRootPath) ?? defaults.scanRootPath
        scanRoots = try container.decodeIfPresent([String].self, forKey: .scanRoots) ?? [scanRootPath]
        if scanRoots.isEmpty {
            scanRoots = [scanRootPath]
        }
        scanDepth = try container.decodeIfPresent(Int.self, forKey: .scanDepth) ?? defaults.scanDepth
        recentDays = try container.decodeIfPresent(Int.self, forKey: .recentDays) ?? defaults.recentDays
        includeRecent = try container.decodeIfPresent(Bool.self, forKey: .includeRecent) ?? defaults.includeRecent
        showHighRisk = try container.decodeIfPresent(Bool.self, forKey: .showHighRisk) ?? defaults.showHighRisk
        trashRetentionDays = try container.decodeIfPresent(Int.self, forKey: .trashRetentionDays) ?? defaults.trashRetentionDays
        trashLimitGb = try container.decodeIfPresent(UInt64.self, forKey: .trashLimitGb) ?? defaults.trashLimitGb
        autoGcTrash = try container.decodeIfPresent(Bool.self, forKey: .autoGcTrash) ?? defaults.autoGcTrash
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
        case .trash: "Move to Trash"
        case .permanentDelete: "Delete Permanently"
        }
    }

    /// What the byte total means for this mode. Only permanent deletion frees space immediately.
    var bytesLabel: String {
        switch self {
        case .dryRun: "WOULD FREE"
        case .trash: "MOVED TO TRASH"
        case .permanentDelete: "SPACE FREED"
        }
    }
}

/// A cleanup the user has asked for but not yet confirmed.
struct PendingCleanup: Identifiable {
    var id = UUID()
    var mode: CleanupMode
    var items: [BridgeProject]

    var totalBytes: UInt64 { items.reduce(0) { $0 + $1.size } }
}

enum AppSection: String, CaseIterable, Identifiable {
    case dashboard
    case scanResults
    case recommendations
    case trash
    case history
    case settings

    var id: String { rawValue }
    static var sidebarSections: [AppSection] {
        [.dashboard, .scanResults, .recommendations, .trash, .history, .settings]
    }

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
