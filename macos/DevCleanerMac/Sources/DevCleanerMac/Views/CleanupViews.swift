import SwiftUI

struct CleanupPlanSheet: View {
    @EnvironmentObject private var model: AppModel
    var mode: CleanupMode

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                Text("Ready to release \(DevCleanerFormatters.bytes(model.selectedBytes))")
                    .font(.system(size: 28, weight: .bold))
                Text(mode == .permanentDelete ? "Permanent deletion cannot be undone." : "Review the cleanup plan before executing.")
                    .foregroundStyle(mode == .permanentDelete ? DCColor.red : DCColor.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 48)
            .padding(.top, 32)
            .padding(.bottom, 24)

            VStack(alignment: .leading, spacing: 16) {
                Text("Space Breakdown")
                    .font(.headline)
                HStack {
                    PlanBucket(title: "Low Risk", bytes: bytes(for: "low"), color: DCColor.green)
                    PlanBucket(title: "Medium Risk", bytes: bytes(for: "medium"), color: DCColor.yellow)
                    PlanBucket(title: "High Risk", bytes: bytes(for: "high"), color: DCColor.red)
                }
                DCCard {
                    Label(mode == .permanentDelete ? "Permanent Delete Safety Check" : "Safety Check", systemImage: "shield")
                        .font(.headline)
                    Text(mode == .permanentDelete ? "These items will bypass Dev Cleaner Trash. Use this only when you do not need restore support." : "Selected items will be moved to internal Trash and can be restored while retained locally.")
                        .foregroundStyle(DCColor.secondary)
                        .padding(.top, 4)
                }
            }
            .padding(.horizontal, 48)
            .padding(.bottom, 24)

            HStack {
                Text(mode == .trash ? "Items can be restored from the Trash pane." : "This action is not undoable.")
                    .font(.caption)
                    .foregroundStyle(DCColor.secondary)
                Spacer()
                Button("Cancel") { model.presentedCleanupMode = nil }
                    .buttonStyle(SecondaryButtonStyle())
                Button("Dry Run") {
                    Task { await model.runSelectedCleanup(mode: .dryRun) }
                }
                .buttonStyle(SecondaryButtonStyle())
                Button(mode.title) {
                    Task { await model.runSelectedCleanup(mode: mode) }
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

    private func bytes(for risk: String) -> UInt64 {
        model.selectedProjects.filter { $0.riskLevel == risk }.reduce(0) { $0 + $1.size }
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
                        Text(model.cleanupMode == .trash ? "Moving items to Trash..." : "Applying cleanup plan...")
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
            Color(red: 0.94, green: 0.95, blue: 0.98)
                .ignoresSafeArea()
            VStack(spacing: 24) {
                VStack(spacing: 10) {
                    Image(systemName: summary.cancelled ? "stop.circle.fill" : "checkmark.seal.fill")
                        .font(.system(size: 48))
                        .foregroundStyle(summary.cancelled ? DCColor.yellow : DCColor.green)
                    Text(summary.cancelled ? "Cleanup Stopped" : "Cleanup Complete")
                        .font(.system(size: 28, weight: .bold))
                        .foregroundStyle(Color(red: 0.10, green: 0.11, blue: 0.12))
                    Text(summary.cancelled ? "Partial results were preserved in the audit log." : "Your development environment is operating at peak efficiency.")
                        .font(.system(size: 13))
                        .foregroundStyle(Color(red: 0.44, green: 0.46, blue: 0.53))
                }

                VStack(spacing: 2) {
                    Text("TOTAL SPACE RELEASED")
                        .font(.caption.weight(.bold))
                        .foregroundStyle(Color(red: 0.44, green: 0.46, blue: 0.53))
                    Text(DevCleanerFormatters.bytes(summary.bytesFreed))
                        .font(.system(size: 48, weight: .bold))
                        .foregroundStyle(Color(red: 0.10, green: 0.11, blue: 0.12))
                }

                HStack(spacing: 24) {
                    SummaryTile(title: "Items Cleaned", value: "\(summary.cleanedCount)")
                    SummaryTile(title: "Items Skipped", value: "\(summary.skippedCount)")
                    SummaryTile(title: "Failed", value: "\(summary.failedCount)")
                }

                VStack(alignment: .leading, spacing: 12) {
                    Text("SPACE RECLAIMED BY CATEGORY")
                        .font(.caption.weight(.bold))
                        .foregroundStyle(Color(red: 0.44, green: 0.46, blue: 0.53))
                    CategoryBar(title: "Build", fraction: 0.66, bytes: summary.bytesFreed * 66 / 100, color: DCColor.blue)
                    CategoryBar(title: "Cache", fraction: 0.25, bytes: summary.bytesFreed * 25 / 100, color: DCColor.purple)
                    CategoryBar(title: "Deps", fraction: 0.09, bytes: summary.bytesFreed * 9 / 100, color: DCColor.secondary)
                }
                .frame(width: 400)

                HStack(spacing: 14) {
                    Button("Go back to Dashboard") {
                        model.showCleanupSummary = false
                        model.selectedSection = .dashboard
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    Button("View Trash") {
                        model.showCleanupSummary = false
                        model.selectedSection = .trash
                    }
                    .buttonStyle(GhostButtonStyle())
                    Button("View Detailed Audit Log") {
                        model.showCleanupSummary = false
                        model.selectedSection = .history
                    }
                    .buttonStyle(GhostButtonStyle())
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

private struct SummaryTile: View {
    var title: String
    var value: String

    var body: some View {
        VStack(spacing: 8) {
            Text(title)
                .font(.system(size: 13))
                .foregroundStyle(Color(red: 0.44, green: 0.46, blue: 0.53))
            Text(value)
                .font(.system(size: 24, weight: .bold))
                .foregroundStyle(Color(red: 0.10, green: 0.11, blue: 0.12))
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
                .foregroundStyle(Color(red: 0.55, green: 0.57, blue: 0.64))
                .frame(width: 56, alignment: .leading)
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.black.opacity(0.08))
                    Capsule()
                        .fill(color)
                        .frame(width: proxy.size.width * fraction)
                }
            }
            .frame(height: 6)
            Text(DevCleanerFormatters.bytes(bytes))
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Color(red: 0.10, green: 0.11, blue: 0.12))
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
                Label(summary.cancelled ? "Cleanup Stopped" : "Cleanup Complete", systemImage: summary.cancelled ? "stop.circle" : "checkmark.seal")
                    .font(.title3.bold())
                    .foregroundStyle(summary.cancelled ? DCColor.yellow : DCColor.green)
                HStack {
                    SummaryMetric(title: "TOTAL SPACE RELEASED", value: DevCleanerFormatters.bytes(summary.bytesFreed))
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
