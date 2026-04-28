import SwiftUI

struct CleanupPlanSheet: View {
    @EnvironmentObject private var model: AppModel
    var mode: CleanupMode

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                Label("Ready to release \(DevCleanerFormatters.bytes(model.selectedBytes))", systemImage: "paintbrush")
                    .font(.title2.bold())
                Text(mode == .permanentDelete ? "Permanent deletion cannot be undone." : "Review the cleanup plan before executing.")
                    .foregroundStyle(mode == .permanentDelete ? DCColor.red : DCColor.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(24)
            .background(DCColor.panel)

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
            .padding(24)

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
            .padding(24)
            .background(DCColor.panel)
        }
        .frame(width: 620)
        .background(DCColor.window)
        .foregroundStyle(DCColor.text)
    }

    private func bytes(for risk: String) -> UInt64 {
        model.selectedProjects.filter { $0.riskLevel == risk }.reduce(0) { $0 + $1.size }
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
