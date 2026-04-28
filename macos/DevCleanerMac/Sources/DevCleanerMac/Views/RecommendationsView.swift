import SwiftUI

struct RecommendationsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var target = "10GB"
    @State private var strategy = "balanced"

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                HStack {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Smart Recommendations")
                            .font(.title2.bold())
                        Text("Configure cleanup parameters to generate a targeted optimization plan.")
                            .foregroundStyle(DCColor.secondary)
                    }
                    Spacer()
                    Button {
                        Task { await model.generateRecommendation(strategy: strategy, target: target) }
                    } label: {
                        Label("Generate Cleanup Plan", systemImage: "sparkles")
                    }
                    .buttonStyle(PrimaryButtonStyle())
                }

                HStack(alignment: .top, spacing: 18) {
                    DCCard {
                        VStack(alignment: .leading, spacing: 14) {
                            Text("Target Space to Release").font(.headline)
                            TextField("10GB", text: $target)
                                .textFieldStyle(.roundedBorder)
                            Text("Cleanup Strategy").font(.headline)
                            Picker("Strategy", selection: $strategy) {
                                Text("Safe").tag("safe")
                                Text("Balanced").tag("balanced")
                                Text("Maximum").tag("maximum")
                            }
                            .pickerStyle(.segmented)
                            StrategyDescription(strategy: strategy)
                        }
                    }
                    .frame(width: 360)

                    DCCard {
                        VStack(alignment: .leading, spacing: 16) {
                            Text("Plan Preview")
                                .font(.headline)
                            if let preview = model.recommendation {
                                Text("BASED ON '\(preview.strategy.uppercased())' STRATEGY")
                                    .font(.caption.bold())
                                    .foregroundStyle(DCColor.secondary)
                                Text(DevCleanerFormatters.bytes(preview.selectedBytes))
                                    .font(.system(size: 36, weight: .bold))
                                    .foregroundStyle(DCColor.green)
                                Text("\(preview.selectedCount) selected items")
                                    .foregroundStyle(DCColor.secondary)
                                Divider().overlay(DCColor.border)
                                ForEach(preview.projects.prefix(6)) { project in
                                    HStack {
                                        Text(project.displayName)
                                        Spacer()
                                        Text(DevCleanerFormatters.bytes(project.size))
                                            .foregroundStyle(DCColor.secondary)
                                    }
                                }
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
                            }
                        }
                    }
                }
            }
            .padding(24)
        }
        .foregroundStyle(DCColor.text)
    }
}

private struct StrategyDescription: View {
    var strategy: String
    var body: some View {
        let copy = switch strategy {
        case "safe": "Verified safe-to-delete caches and low-risk build products."
        case "maximum": "Aggressive cleanup including larger local caches and archives."
        default: "Includes unneeded build artifacts and older dependency directories."
        }
        Text(copy)
            .font(.callout)
            .foregroundStyle(DCColor.secondary)
    }
}
