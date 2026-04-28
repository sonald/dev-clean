import SwiftUI

struct ScanResultsView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            Divider().overlay(DCColor.border)
            HStack(spacing: 0) {
                resultsList
                    .frame(minWidth: 420)
                Divider().overlay(DCColor.border)
                ProjectDetailView(project: model.focusedProject)
                    .frame(width: 360)
            }
        }
        .foregroundStyle(DCColor.text)
    }

    private var toolbar: some View {
        HStack(spacing: 12) {
            TextField("Search results...", text: $model.searchText)
                .textFieldStyle(.plain)
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
                .background(DCColor.panel2)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                .frame(width: 260)
            Picker("Category", selection: $model.categoryFilter) {
                Text("Category: All").tag("all")
                Text("Cache").tag("cache")
                Text("Build").tag("build")
                Text("Deps").tag("deps")
            }
            .labelsHidden()
            Picker("Risk", selection: $model.riskFilter) {
                Text("Risk: Any").tag("all")
                Text("Low").tag("low")
                Text("Medium").tag("medium")
                Text("High").tag("high")
            }
            .labelsHidden()
            Spacer()
            Button {
                model.presentCleanupPlan(mode: .trash)
            } label: {
                Label("Move to Trash", systemImage: "trash")
            }
            .buttonStyle(PrimaryButtonStyle(color: DCColor.red))
            .disabled(model.selectedProjects.isEmpty)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    private var resultsList: some View {
        VStack(spacing: 0) {
            if model.isScanning {
                ProgressView(value: model.scanProgress)
                    .tint(DCColor.blue)
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
            }
            List(model.visibleProjects, selection: $model.focusedProjectID) { project in
                ProjectRow(project: project, selected: model.selectedProjectIDs.contains(project.id)) {
                    if model.selectedProjectIDs.contains(project.id) {
                        model.selectedProjectIDs.remove(project.id)
                    } else {
                        model.selectedProjectIDs.insert(project.id)
                    }
                    model.focusedProjectID = project.id
                }
                .tag(project.id)
                .listRowBackground(Color.clear)
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .background(DCColor.window)
        }
    }
}

struct ProjectRow: View {
    var project: BridgeProject
    var selected: Bool
    var toggle: () -> Void

    var body: some View {
        HStack(spacing: 14) {
            Button(action: toggle) {
                Image(systemName: selected ? "checkmark.square.fill" : "square")
                    .foregroundStyle(selected ? DCColor.blue : DCColor.secondary)
            }
            .buttonStyle(.plain)
            Image(systemName: icon)
                .foregroundStyle(DCColor.secondary)
                .frame(width: 20)
            VStack(alignment: .leading, spacing: 4) {
                Text(project.displayName)
                    .font(.system(size: 13, weight: .bold))
                Text(DevCleanerFormatters.shortPath(project.cleanableDir))
                    .font(.system(size: 12))
                    .foregroundStyle(DCColor.secondary)
                    .lineLimit(1)
                HStack {
                    Tag(project.categoryLabel)
                    RiskTag(project.riskLevel ?? "medium")
                }
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 4) {
                Text(DevCleanerFormatters.bytes(project.size))
                    .font(.system(size: 13, weight: .bold))
                if project.inUse {
                    Text("In use")
                        .font(.caption)
                        .foregroundStyle(DCColor.yellow)
                }
            }
        }
        .padding(10)
        .background(selected ? DCColor.blue.opacity(0.18) : Color.clear)
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }

    private var icon: String {
        switch project.category {
        case "deps": "shippingbox"
        case "build": "hammer"
        case "cache": "externaldrive"
        default: "folder"
        }
    }
}

struct ProjectDetailView: View {
    var project: BridgeProject?
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 20) {
            if let project {
                VStack(spacing: 10) {
                    Image(systemName: "shippingbox")
                        .font(.system(size: 28))
                        .foregroundStyle(DCColor.blue)
                        .frame(width: 64, height: 64)
                        .background(DCColor.panel2)
                        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                    Text(project.displayName)
                        .font(.title3.bold())
                    Text("\(DevCleanerFormatters.bytes(project.size)) • \(project.displayType)")
                        .font(.caption)
                        .foregroundStyle(DCColor.secondary)
                }
                Divider().overlay(DCColor.border)
                DetailCard(title: "IDENTIFICATION ANALYSIS", icon: "magnifyingglass") {
                    Text("Detected as \(project.categoryLabel.lowercased()). Source is \(project.confidence ?? "unknown") confidence and risk is \(project.riskLabel.lowercased()).")
                    Text("Path: \(DevCleanerFormatters.shortPath(project.cleanableDir))")
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(DCColor.secondary)
                        .padding(8)
                        .background(Color.black.opacity(0.2))
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                }
                DetailCard(title: "RISK ANALYSIS", icon: "exclamationmark.triangle") {
                    Text(project.riskLevel == "high" ? "Review before deleting. This item may require manual restoration." : "Deletion impact is minimal. Build artifacts and caches can usually be regenerated by development tools.")
                }
                DCCard {
                    HStack {
                        Image(systemName: (project.protected ?? false) ? "lock.fill" : "checkmark.circle.fill")
                            .foregroundStyle((project.protected ?? false) ? DCColor.yellow : DCColor.green)
                            .font(.title2)
                        VStack(alignment: .leading) {
                            Text((project.protected ?? false) ? "Protected" : "Safe to Clean")
                                .font(.headline)
                            Text((project.protected ?? false) ? (project.protectedBy ?? "Protected by policy") : "Recommended for cleanup")
                                .font(.caption)
                                .foregroundStyle(DCColor.secondary)
                        }
                    }
                }
                Spacer()
                Button {
                    model.selectedProjectIDs.insert(project.id)
                    model.presentCleanupPlan(mode: .trash)
                } label: {
                    Label("Move to Trash (\(DevCleanerFormatters.bytes(project.size)))", systemImage: "trash")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(PrimaryButtonStyle(color: DCColor.red))
            } else {
                Spacer()
                EmptyState(title: "No Result Selected", systemImage: "folder.badge.questionmark")
                Spacer()
            }
        }
        .padding(24)
        .background(DCColor.window)
    }
}

struct DetailCard<Content: View>: View {
    var title: String
    var icon: String
    @ViewBuilder var content: Content

    var body: some View {
        DCCard {
            VStack(alignment: .leading, spacing: 10) {
                Label(title, systemImage: icon)
                    .font(.caption.bold())
                    .foregroundStyle(DCColor.secondary)
                content
                    .font(.system(size: 13))
                    .foregroundStyle(DCColor.text)
            }
        }
    }
}

struct Tag: View {
    var text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        Text(text)
            .font(.caption2)
            .foregroundStyle(DCColor.secondary)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(DCColor.panel2)
            .clipShape(Capsule())
    }
}

struct RiskTag: View {
    var risk: String
    init(_ risk: String) { self.risk = risk }
    var body: some View {
        Text(risk.capitalized + " Risk")
            .font(.caption2)
            .foregroundStyle(color)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(color.opacity(0.16))
            .clipShape(Capsule())
    }
    private var color: Color {
        switch risk {
        case "low": DCColor.green
        case "high": DCColor.red
        default: DCColor.yellow
        }
    }
}
