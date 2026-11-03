import SwiftUI

struct RecommendationsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var target = "10GB"
    @State private var strategy = "balanced"

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                DCPageHeader(
                    title: "Smart Recommendations",
                    subtitle: "Configure your cleanup parameters to generate a targeted optimization plan."
                ) {
                    Button {
                        Task { await model.generateRecommendation(strategy: strategy, target: target) }
                    } label: {
                        Label("Generate Cleanup Plan", systemImage: "sparkles")
                    }
                    .buttonStyle(PrimaryButtonStyle())
                }

                HStack(alignment: .top, spacing: 20) {
                    DCCard {
                        VStack(alignment: .leading, spacing: 28) {
                            VStack(alignment: .leading, spacing: 12) {
                                Text("Target Space to Release").font(.headline)
                                Text("Set the minimum amount of disk space you wish to recover.")
                                    .font(.system(size: 13))
                                    .foregroundStyle(DCColor.secondary)
                                HStack(spacing: 8) {
                                    TextField("10GB", text: $target)
                                        .textFieldStyle(.plain)
                                        .font(.system(size: 26, weight: .bold, design: .monospaced))
                                        .padding(.horizontal, 14)
                                        .padding(.vertical, 10)
                                        .background(DCColor.elevated)
                                        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                                        .frame(width: 150)
                                    if let volume = model.volume {
                                        Text("Free now: \(DevCleanerFormatters.bytes(volume.availableBytes))")
                                            .font(.caption)
                                            .foregroundStyle(DCColor.tertiary)
                                    }
                                }
                            }

                            VStack(alignment: .leading, spacing: 12) {
                                Text("Cleanup Strategy").font(.headline)
                                Text(model.preferences.showHighRisk
                                     ? "How candidates are ranked. High-risk items are included because they are shown in Scan Results."
                                     : "How candidates are ranked. Only low- and medium-risk items are considered.")
                                    .font(.system(size: 13))
                                    .foregroundStyle(DCColor.secondary)
                                HStack(spacing: 10) {
                                    StrategyCard(id: "safe", title: "Safe First", subtitle: "Prefers old, low-risk items. May pick more items to reach the target.", selection: $strategy)
                                    StrategyCard(id: "balanced", title: "Balanced", subtitle: "Weighs size, age, and risk evenly.", selection: $strategy)
                                    StrategyCard(id: "maximum", title: "Max Space", subtitle: "Largest items first; risk only lightly penalized.", selection: $strategy)
                                }
                            }
                        }
                    }
                    .frame(minWidth: 520)

                    DCCard {
                        VStack(alignment: .leading, spacing: 18) {
                            HStack(alignment: .top) {
                                VStack(alignment: .leading, spacing: 6) {
                                    Text("Plan Preview")
                                        .font(.headline)
                                    Text("BASED ON '\(strategy.uppercased())' STRATEGY")
                                        .font(.caption.bold())
                                        .foregroundStyle(DCColor.secondary)
                                }
                                Spacer()
                                VStack(alignment: .trailing, spacing: 3) {
                                    Text(DevCleanerFormatters.bytes(model.recommendation?.selectedBytes ?? 0))
                                        .font(.system(size: 13, design: .monospaced))
                                        .foregroundStyle(DCColor.indigo)
                                    Text("EST. RECOVERY")
                                        .font(.caption2)
                                        .foregroundStyle(DCColor.tertiary)
                                }
                            }
                            if let preview = model.recommendation {
                                ForEach(preview.projects.prefix(6)) { project in
                                    HStack {
                                        Text(project.displayName)
                                            .foregroundStyle(project.skipReason == nil ? DCColor.text : DCColor.tertiary)
                                        Spacer()
                                        Text(DevCleanerFormatters.bytes(project.size))
                                            .font(.system(size: 12, design: .monospaced))
                                            .foregroundStyle(DCColor.secondary)
                                    }
                                }
                                if preview.projects.count > 6 {
                                    Text("and \(preview.projects.count - 6) more")
                                        .font(.caption)
                                        .foregroundStyle(DCColor.tertiary)
                                }
                                Divider().overlay(DCColor.border)
                                if let volume = model.volume {
                                    DrivePreviewBar(volume: volume, selectedBytes: preview.selectedBytes)
                                }
                                Text("\(preview.selectedCount) items selected. The highlighted segment is the space they occupy.")
                                    .font(.caption)
                                    .foregroundStyle(DCColor.secondary)
                                HStack {
                                    Spacer()
                                    Button {
                                        model.presentCleanup(mode: .trash, items: preview.projects)
                                    } label: {
                                        Label("Review & Move to Trash", systemImage: "trash")
                                    }
                                    .buttonStyle(PrimaryButtonStyle(color: DCColor.purple))
                                    .disabled(preview.projects.isEmpty)
                                }
                            } else {
                                Text("No plan generated yet.")
                                    .foregroundStyle(DCColor.secondary)
                            }
                        }
                    }
                    .frame(width: 420)
                }
            }
            .padding(24)
        }
        .foregroundStyle(DCColor.text)
    }
}

private struct StrategyCard: View {
    var id: String
    var title: String
    var subtitle: String
    @Binding var selection: String

    var body: some View {
        let selected = selection == id
        Button {
            selection = id
        } label: {
            VStack(alignment: .leading, spacing: 8) {
                Text(title)
                    .font(.system(size: 13, weight: .bold))
                    .foregroundStyle(selected ? DCColor.text : DCColor.secondary)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(selected ? DCColor.indigo.opacity(0.8) : DCColor.tertiary)
                    .lineLimit(3)
            }
            .frame(maxWidth: .infinity, minHeight: 86, alignment: .topLeading)
            .padding(12)
            .background(selected ? DCColor.blue.opacity(0.16) : DCColor.elevated)
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(selected ? DCColor.blue.opacity(0.55) : DCColor.border)
            )
        }
        .buttonStyle(.plain)
    }
}

private struct DrivePreviewBar: View {
    var volume: VolumeInfo
    var selectedBytes: UInt64

    var body: some View {
        let total = Double(max(volume.totalBytes, 1))
        let used = Double(volume.usedBytes) / total
        let selected = min(Double(selectedBytes) / total, used)
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("\(volume.name.uppercased()) (\(DevCleanerFormatters.bytes(volume.totalBytes)))")
                Spacer()
                Text("\(Int((used * 100).rounded()))% USED")
            }
            .font(.caption2.bold())
            .foregroundStyle(DCColor.tertiary)
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(DCColor.elevated)
                    Capsule().fill(DCColor.blue.opacity(0.55)).frame(width: proxy.size.width * used)
                    Capsule().fill(DCColor.indigo).frame(width: proxy.size.width * selected).offset(x: proxy.size.width * (used - selected))
                }
            }
            .frame(height: 12)
        }
    }
}
