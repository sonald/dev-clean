import SwiftUI

struct CleanupPlanSheet: View {
    @EnvironmentObject private var model: AppModel
    var pending: PendingCleanup

    private var mode: CleanupMode { pending.mode }

    private var headline: String {
        let size = DevCleanerFormatters.bytes(pending.totalBytes)
        switch mode {
        case .dryRun: return "Preview cleanup of \(size)"
        case .trash: return "Move \(size) to Trash"
        case .permanentDelete: return "Permanently delete \(size)"
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                Text(headline)
                    .font(.system(size: 28, weight: .bold))
                Text(subtitle)
                    .foregroundStyle(mode == .permanentDelete ? DCColor.red : DCColor.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 48)
            .padding(.top, 32)
            .padding(.bottom, 20)

            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    PlanBucket(title: "Low Risk", bytes: bytes(for: "low"), color: DCColor.green)
                    PlanBucket(title: "Medium Risk", bytes: bytes(for: "medium"), color: DCColor.yellow)
                    PlanBucket(title: "High Risk", bytes: bytes(for: "high"), color: DCColor.red)
                }
                Text("\(pending.items.count) ITEMS")
                    .font(.caption.weight(.bold))
                    .foregroundStyle(DCColor.secondary)
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(pending.items) { item in
                            HStack(spacing: 10) {
                                RiskTag(item.riskLevel ?? "medium")
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(item.displayName)
                                        .font(.system(size: 13, weight: .semibold))
                                    Text(DevCleanerFormatters.shortPath(item.cleanableDir))
                                        .font(.system(size: 11, design: .monospaced))
                                        .foregroundStyle(DCColor.secondary)
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                }
                                Spacer()
                                Text(DevCleanerFormatters.bytes(item.size))
                                    .font(.system(size: 12, design: .monospaced))
                                    .foregroundStyle(DCColor.secondary)
                            }
                            .padding(.vertical, 7)
                            Divider().overlay(DCColor.border)
                        }
                    }
                }
                .frame(maxHeight: 220)
            }
            .padding(.horizontal, 48)
            .padding(.bottom, 20)

            HStack {
                Text(footnote)
                    .font(.caption)
                    .foregroundStyle(DCColor.secondary)
                Spacer()
                Button("Cancel") { model.pendingCleanup = nil }
                    .buttonStyle(SecondaryButtonStyle())
                    .keyboardShortcut(.cancelAction)
                if mode != .dryRun {
                    Button("Dry Run") {
                        Task { await model.runCleanup(items: pending.items, mode: .dryRun) }
                    }
                    .buttonStyle(SecondaryButtonStyle())
                }
                Button(mode.title) {
                    Task { await model.runCleanup(items: pending.items, mode: mode) }
                }
                .buttonStyle(PrimaryButtonStyle(color: mode == .permanentDelete ? DCColor.red : DCColor.purple))
            }
            .padding(.horizontal, 48)
            .padding(.vertical, 24)
        }
        .frame(width: 672)
        .background(DCColor.window)
        .foregroundStyle(DCColor.text)
    }

    private var subtitle: String {
        switch mode {
        case .dryRun: "Nothing will be changed. Each item is re-checked exactly as a real cleanup would."
        case .trash: "Items move to the Dev Cleaner Trash and can be restored. Disk space is released when the Trash is emptied."
        case .permanentDelete: "These items bypass the Trash. Permanent deletion cannot be undone."
        }
    }

    private var footnote: String {
        "Each item is re-verified before it is touched; in-use, recent, and protected items are skipped."
    }

    private func bytes(for risk: String) -> UInt64 {
        pending.items.filter { $0.riskLevel == risk }.reduce(0) { $0 + $1.size }
    }
}

struct CleanupProgressOverlay: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        ZStack {
            DCColor.window.opacity(0.84)
                .ignoresSafeArea()
            DCCard(padding: 24) {
                VStack(alignment: .leading, spacing: 22) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Cleaning in Progress")
                            .font(.system(size: 28, weight: .bold))
                        Text(progressSubtitle)
                            .font(.system(size: 13))
                            .foregroundStyle(DCColor.secondary)
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Text("OVERALL PROGRESS")
                                .font(.caption.weight(.bold))
                                .foregroundStyle(DCColor.secondary)
                            Spacer()
                            Text("\(Int(model.cleanupProgress * 100))%")
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(DCColor.text)
                        }
                        ProgressView(value: model.cleanupProgress)
                            .tint(DCColor.blue)
                    }

                    HStack(spacing: 12) {
                        ProgressMetric(title: "PROCESSING", value: "\(max(model.cleanupTotalCount - model.cleanupProcessedCount, 0))", color: DCColor.text)
                        ProgressMetric(title: "SUCCESSFUL", value: "\(model.cleanupSucceededCount)", color: DCColor.green)
                        ProgressMetric(title: "SKIPPED", value: "\(model.cleanupSkippedCount)", color: DCColor.tertiary)
                    }

                    VStack(alignment: .leading, spacing: 7) {
                        ForEach(model.cleanupLog.suffix(7), id: \.self) { line in
                            Text(line)
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(line.contains("[success]") ? DCColor.green : DCColor.secondary)
                                .lineLimit(1)
                        }
                        if let path = model.cleanupCurrentPath {
                            Text("> Processing \(DevCleanerFormatters.shortPath(path))...")
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(DCColor.green)
                                .lineLimit(1)
                        }
                    }
                    .frame(maxWidth: .infinity, minHeight: 148, alignment: .topLeading)
                    .padding(14)
                    .background(Color.black.opacity(0.20))
                    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))

                    HStack {
                        Spacer()
                        Button("Stop") {
                            model.stopAfterCurrentItem()
                        }
                        .buttonStyle(SecondaryButtonStyle())
                    }
                }
            }
            .frame(width: 540)
            .shadow(color: .black.opacity(0.35), radius: 32, y: 22)
        }
    }
}

extension CleanupProgressOverlay {
    private var progressSubtitle: String {
        switch model.cleanupMode {
        case .dryRun: "Dry run: checking items, nothing will be changed..."
        case .trash: "Moving items to Trash..."
        case .permanentDelete: "Deleting items permanently..."
        }
    }
}

private struct ProgressMetric: View {
    var title: String
    var value: String
    var color: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.caption.weight(.bold))
                .foregroundStyle(DCColor.secondary)
            Text(value)
                .font(.title3.bold())
                .foregroundStyle(color)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(DCColor.elevated)
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
}

struct CleanupSummaryOverlay: View {
    @EnvironmentObject private var model: AppModel
    var summary: CleanupSummary

    var body: some View {
        ZStack {
            DCColor.window.opacity(0.96)
                .ignoresSafeArea()
            VStack(spacing: 24) {
                VStack(spacing: 10) {
                    Image(systemName: summary.cancelled ? "stop.circle.fill" : summary.mode == .dryRun ? "eye.circle.fill" : "checkmark.seal.fill")
                        .font(.system(size: 48))
                        .foregroundStyle(summary.cancelled ? DCColor.yellow : DCColor.green)
                    Text(summary.headline)
                        .font(.system(size: 28, weight: .bold))
                    Text(summary.detail)
                        .font(.system(size: 13))
                        .foregroundStyle(DCColor.secondary)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: 460)
                }

                VStack(spacing: 2) {
                    Text(summary.mode.bytesLabel)
                        .font(.caption.weight(.bold))
                        .foregroundStyle(DCColor.secondary)
                    Text(DevCleanerFormatters.bytes(summary.bytesFreed))
                        .font(.system(size: 48, weight: .bold))
                }

                HStack(spacing: 24) {
                    SummaryTile(title: summary.mode == .dryRun ? "Would Clean" : "Items Cleaned", value: "\(summary.cleanedCount)")
                    SummaryTile(title: "Items Skipped", value: "\(summary.skippedCount)")
                    SummaryTile(title: "Failed", value: "\(summary.failedCount)")
                }

                if !summary.bytesByCategory.isEmpty {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("BY CATEGORY")
                            .font(.caption.weight(.bold))
                            .foregroundStyle(DCColor.secondary)
                        ForEach(summary.categoryRows, id: \.category) { row in
                            CategoryBar(title: row.category.capitalized, fraction: row.fraction, bytes: row.bytes, color: color(for: row.category))
                        }
                    }
                    .frame(width: 400)
                }

                HStack(spacing: 14) {
                    Button("Done") {
                        model.showCleanupSummary = false
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    if summary.mode == .trash {
                        Button("View Trash") {
                            model.showCleanupSummary = false
                            model.selectedSection = .trash
                        }
                        .buttonStyle(GhostButtonStyle())
                    }
                    Button("View Audit Log") {
                        model.showCleanupSummary = false
                        model.selectedSection = .history
                    }
                    .buttonStyle(GhostButtonStyle())
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .foregroundStyle(DCColor.text)
        }
    }

    private func color(for category: String) -> Color {
        switch category {
        case "build": DCColor.blue
        case "cache": DCColor.purple
        case "deps": DCColor.yellow
        default: DCColor.secondary
        }
    }
}

extension CleanupSummary {
    var headline: String {
        if cancelled { return "Cleanup Stopped" }
        switch mode {
        case .dryRun: return "Dry Run Complete"
        case .trash: return "Moved to Trash"
        case .permanentDelete: return "Cleanup Complete"
        }
    }

    var detail: String {
        var parts: [String] = []
        switch mode {
        case .dryRun: parts.append("Nothing was changed.")
        case .trash: parts.append("Items can be restored from Trash. Space is released when the Trash is emptied or expires.")
        case .permanentDelete: parts.append("Deleted items cannot be restored.")
        }
        if skippedCount > 0 {
            parts.append("\(skippedCount) skipped items are listed in the audit log with the reason.")
        }
        if cancelled {
            parts.append("Remaining items were not processed.")
        }
        return parts.joined(separator: " ")
    }

    var categoryRows: [(category: String, bytes: UInt64, fraction: Double)] {
        let total = max(bytesByCategory.values.reduce(0, +), 1)
        return bytesByCategory
            .sorted { $0.value > $1.value }
            .map { (category: $0.key, bytes: $0.value, fraction: Double($0.value) / Double(total)) }
    }
}

private struct SummaryTile: View {
    var title: String
    var value: String

    var body: some View {
        VStack(spacing: 8) {
            Text(title)
                .font(.system(size: 13))
                .foregroundStyle(DCColor.secondary)
            Text(value)
                .font(.system(size: 24, weight: .bold))
        }
        .frame(width: 150)
    }
}

private struct CategoryBar: View {
    var title: String
    var fraction: Double
    var bytes: UInt64
    var color: Color

    var body: some View {
        HStack {
            Text(title)
                .font(.system(size: 12))
                .foregroundStyle(DCColor.secondary)
                .frame(width: 64, alignment: .leading)
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(DCColor.elevated)
                    Capsule()
                        .fill(color)
                        .frame(width: proxy.size.width * fraction)
                }
            }
            .frame(height: 6)
            Text(DevCleanerFormatters.bytes(bytes))
                .font(.system(size: 12, weight: .semibold))
                .frame(width: 76, alignment: .trailing)
        }
    }
}

private struct PlanBucket: View {
    var title: String
    var bytes: UInt64
    var color: Color

    var body: some View {
        DCCard {
            VStack(alignment: .leading, spacing: 8) {
                Label(title, systemImage: "circle.fill")
                    .foregroundStyle(color)
                Text(DevCleanerFormatters.bytes(bytes))
                    .font(.title3.bold())
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

struct CleanupSummaryView: View {
    var summary: CleanupSummary

    var body: some View {
        DCCard {
            VStack(alignment: .leading, spacing: 12) {
                Label(summary.headline, systemImage: summary.cancelled ? "stop.circle" : "checkmark.seal")
                    .font(.title3.bold())
                    .foregroundStyle(summary.cancelled ? DCColor.yellow : DCColor.green)
                HStack {
                    SummaryMetric(title: summary.mode.bytesLabel, value: DevCleanerFormatters.bytes(summary.bytesFreed))
                    SummaryMetric(title: "Items Cleaned", value: "\(summary.cleanedCount)")
                    SummaryMetric(title: "Items Skipped", value: "\(summary.skippedCount)")
                    SummaryMetric(title: "Failed", value: "\(summary.failedCount)")
                }
            }
        }
    }
}

private struct SummaryMetric: View {
    var title: String
    var value: String
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.caption)
                .foregroundStyle(DCColor.secondary)
            Text(value)
                .font(.title3.bold())
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
