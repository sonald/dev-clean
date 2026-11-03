import Foundation

enum DevCleanerFormatters {
    static func bytes(_ value: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(value), countStyle: .file)
    }

    static func shortPath(_ path: String) -> String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        if path.hasPrefix(home) {
            return "~" + path.dropFirst(home.count)
        }
        return path
    }

    /// Embedded in English sentences ("Modified 3 days ago").
    // ponytail: fixed to English like every UI string; use the app localization once there is one.
    static func relative(_ value: Date) -> String {
        let formatter = RelativeDateTimeFormatter()
        formatter.locale = Locale(identifier: "en")
        return formatter.localizedString(for: value, relativeTo: Date())
    }

    static func date(_ value: Date?) -> String {
        guard let value else { return "--" }
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return formatter.string(from: value)
    }
}
