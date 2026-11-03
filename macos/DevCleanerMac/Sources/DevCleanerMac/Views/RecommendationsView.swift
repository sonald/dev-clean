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
                                    Text("Available: 42.8 GB")
                                        .font(.caption)
                                        .foregroundStyle(DCColor.tertiary)
                                }
                            }

                            VStack(alignment: .leading, spacing: 12) {
                                Text("Cleanup Strategy").font(.headline)
                                Text("Select the aggressiveness of the cleanup algorithm.")
                                    .font(.system(size: 13))
                                    .foregroundStyle(DCColor.secondary)
                                HStack(spacing: 10) {
                                    StrategyCard(id: "safe", title: "Safe", subtitle: "Verified safe-to-delete system logs and basic caches.", selection: $strategy)
                                    StrategyCard(id: "balanced", title: "Balanced", subtitle: "Includes unneeded build artifacts and older derived data.", selection: $strategy)
                                    StrategyCard(id: "maximum", title: "Maximum", subtitle: "Aggressive cleanup including all local caches and archives.", selection: $strategy)
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
                                Divider().overlay(DCColor.border)
                                DrivePreviewBar(fraction: preview.selectedBytes == 0 ? 0 : min(Double(preview.selectedBytes) / Double(512 * 1024 * 1024 * 1024), 0.35))
                                Text("\(preview.selectedCount) items selected. Highlighted segment represents space to be freed.")
                                    .font(.caption)
                                    .foregroundStyle(DCColor.secondary)
                                HStack {
                                    Spacer()
                                    Button {
                                        model.recommendation = preview
                                        Task { await model.runRecommendationCleanup(mode: .trash) }
                                    } label: {
                                        Label("Clean with Trash", systemImage: "trash")
                                    }
                                    .buttonStyle(PrimaryButtonStyle(color: DCColor.purple))
                                }
                            } else {
                                Text("No plan generated yet.")
                                    .foregroundStyle(DCColor.secondary)
                                DrivePreviewBar(fraction: 0.08)
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
    var fraction: Double

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("DRIVE CAPACITY (512 GB)")
                Spacer()
                Text("65% USED")
            }
            .font(.caption2.bold())
            .foregroundStyle(DCColor.tertiary)
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(DCColor.elevated)
                    Capsule().fill(DCColor.blue.opacity(0.55)).frame(width: proxy.size.width * 0.65)
                    Capsule().fill(DCColor.indigo).frame(width: proxy.size.width * fraction).offset(x: proxy.size.width * 0.65)
                }
            }
            .frame(height: 12)
        }
    }
}
