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
                .frame(minWidth: 120, maxWidth: 260)
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
            Toggle(isOn: $model.preferences.showHighRisk) {
                Text(model.preferences.showHighRisk ? "High risk shown" : "Show high risk (\(model.hiddenHighRiskProjects.count))")
                    .font(.caption)
            }
            .toggleStyle(.checkbox)
            .fixedSize()
            .help("Dependencies and .gitignore-only matches are high risk and hidden by default.")
            .onChange(of: model.preferences.showHighRisk) { _ in
                model.schedulePreferencesSave()
            }
            Label(model.scanRootDisplayName, systemImage: "folder")
                .font(.caption)
                .foregroundStyle(DCColor.secondary)
                .lineLimit(1)
                .frame(maxWidth: 180, alignment: .leading)
            Spacer()
            Button {
                if model.isScanning {
                    model.stopScan()
                } else {
                    model.startSmartScan()
                }
            } label: {
                Label(model.isScanning ? "Stop Scan" : "Scan", systemImage: model.isScanning ? "stop.circle" : "scope")
                    .fixedSize()
            }
            .buttonStyle(model.isScanning ? PrimaryButtonStyle(color: DCColor.red) : PrimaryButtonStyle())
            Button {
                model.presentCleanup(mode: .trash, items: model.selectedProjects)
            } label: {
                Label("Move \(model.selectedProjects.count) to Trash", systemImage: "trash")
                    .fixedSize()
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
            HStack {
                Text(model.scanStatusMessage)
                    .font(.caption)
                    .foregroundStyle(DCColor.secondary)
                    .lineLimit(1)
                Spacer()
                let selectable = model.visibleProjects.filter(\.isSelectable).count
                Button("Select All Shown (\(selectable))") { model.selectVisible() }
                    .disabled(selectable == 0)
                    .help("Selects what the current search and filters show. In-use and protected items are never selected.")
                Button("Clear Selection") { model.clearSelection() }
                    .disabled(model.selectedProjectIDs.isEmpty)
            }
            .buttonStyle(GhostButtonStyle())
            .font(.caption)
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            List(model.visibleProjects, selection: $model.focusedProjectID) { project in
                ProjectRow(project: project, selected: model.selectedProjectIDs.contains(project.id)) {
                    model.toggleSelection(project)
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
                Image(systemName: project.isSelectable ? (selected ? "checkmark.square.fill" : "square") : "lock.square")
                    .foregroundStyle(selected ? DCColor.blue : DCColor.secondary)
            }
            .buttonStyle(.plain)
            .disabled(!project.isSelectable)
            .help(project.isSelectable ? "Select for cleanup" : "In use or protected items are skipped by every cleanup.")
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
                    Text(metadata)
                        .font(.caption2)
                        .foregroundStyle(DCColor.tertiary)
                        .lineLimit(1)
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

    private var metadata: String {
        let modified = project.lastModified.map { "Modified \(DevCleanerFormatters.relative($0))" }
        return [modified, project.ruleLabel].compactMap { $0 }.joined(separator: " · ")
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
                    if let rule = project.ruleLabel {
                        Text("Matched by \(rule).")
                    }
                    if let modified = project.lastModified {
                        Text("Last modified \(DevCleanerFormatters.relative(modified)).")
                    }
                    Text("Path: \(DevCleanerFormatters.shortPath(project.cleanableDir))")
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(DCColor.secondary)
                        .padding(8)
                        .background(Color.black.opacity(0.2))
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                }
                let verdict = CleanupVerdict(project: project)
                DCCard {
                    HStack {
                        Image(systemName: verdict.icon)
                            .foregroundStyle(verdict.color)
                            .font(.title2)
                        VStack(alignment: .leading, spacing: 4) {
                            Text(verdict.title)
                                .font(.headline)
                            Text(verdict.detail)
                                .font(.caption)
                                .foregroundStyle(DCColor.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
                Spacer()
                Button {
                    model.presentCleanup(mode: .trash, items: [project])
                } label: {
                    Label("Move This Item to Trash (\(DevCleanerFormatters.bytes(project.size)))", systemImage: "trash")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(PrimaryButtonStyle(color: DCColor.red))
                .disabled(!project.isSelectable)
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
            .lineLimit(1)
            .fixedSize()
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

/// One honest conclusion per item, ordered by what blocks cleanup first.
struct CleanupVerdict {
    var title: String
    var detail: String
    var icon: String
    var color: Color

    init(project: BridgeProject) {
        if project.protected == true {
            self.init(title: "Protected", detail: project.protectedBy ?? "Protected by policy; it will not be cleaned.", icon: "lock.fill", color: DCColor.yellow)
        } else if project.inUse {
            self.init(title: "In Use", detail: "Its lock file changed in the last 7 days, so every cleanup skips it.", icon: "hourglass", color: DCColor.yellow)
        } else if project.recent == true {
            self.init(title: "Recently Modified", detail: "Changed inside the recent-change window. Shown because recent items are visible in Settings.", icon: "clock", color: DCColor.yellow)
        } else if project.confidence == "low" {
            self.init(title: "Review Before Cleaning", detail: "Matched only by a .gitignore pattern. It may hold data you created, not rebuildable output.", icon: "exclamationmark.triangle.fill", color: DCColor.red)
        } else {
            switch project.riskLevel {
            case "low":
                self.init(title: "Safe to Clean", detail: "A cache that tools recreate automatically.", icon: "checkmark.circle.fill", color: DCColor.green)
            case "high":
                self.init(title: "Review Before Cleaning", detail: "Dependencies must be reinstalled (for example npm install or uv sync), which may need network access.", icon: "exclamationmark.triangle.fill", color: DCColor.red)
            default:
                self.init(title: "Usually Safe", detail: "Build output that the next build regenerates; rebuilding takes time.", icon: "checkmark.circle", color: DCColor.green)
            }
        }
    }

    init(title: String, detail: String, icon: String, color: Color) {
        self.title = title
        self.detail = detail
        self.icon = icon
        self.color = color
    }
}
