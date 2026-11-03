import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var tab = "scanning"
    @State private var confirmEmpty = false

    var body: some View {
        HStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Dev Cleaner")
                    .font(.headline)
                    .padding(.bottom, 8)
                SettingsTab(title: "Scanning", id: "scanning", tab: $tab)
                SettingsTab(title: "Scan Roots", id: "roots", tab: $tab)
                SettingsTab(title: "Protection Rules", id: "protection", tab: $tab)
                SettingsTab(title: "Trash Retention", id: "trash", tab: $tab)
                Spacer()
                Text("Changes are saved automatically.")
                    .font(.caption)
                    .foregroundStyle(DCColor.tertiary)
                Button {
                    model.startSmartScan()
                } label: {
                    Label("Scan Now", systemImage: "scope")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(SecondaryButtonStyle())
            }
            .padding(18)
            .frame(width: 220)
            .background(.ultraThinMaterial)

            Divider().overlay(DCColor.border)
            ScrollView {
                settingsContent
                    .padding(24)
            }
        }
        .background(DCColor.window)
        .foregroundStyle(DCColor.text)
        .onChange(of: model.preferences) { _ in
            model.schedulePreferencesSave()
        }
    }

    @ViewBuilder
    private var settingsContent: some View {
        switch tab {
        case "roots":
            SettingsPanel(title: "Scan Roots", subtitle: "Folders Dev Cleaner searches for build artifacts.") {
                SettingsSectionHeader("Active Directories") {
                    Button {
                        model.addScanRoot()
                    } label: {
                        Label("Add Path", systemImage: "plus")
                    }
                    .buttonStyle(SecondaryButtonStyle())
                }
                VStack(spacing: 0) {
                    ForEach(model.activeScanRoots, id: \.self) { path in
                        HStack {
                            Image(systemName: "folder")
                                .foregroundStyle(DCColor.secondary)
                            Text(DevCleanerFormatters.shortPath(path))
                                .font(.system(size: 12, design: .monospaced))
                            Spacer()
                            Button {
                                model.removeScanRoot(path)
                            } label: {
                                Image(systemName: "minus.circle")
                            }
                            .buttonStyle(.plain)
                            .foregroundStyle(DCColor.secondary)
                            .disabled(model.activeScanRoots.count == 1)
                        }
                        .padding(.vertical, 12)
                        Divider().overlay(DCColor.border)
                    }
                }
            }
        case "protection":
            SettingsPanel(title: "Protection Rules", subtitle: "Protected paths are never listed or cleaned.") {
                SettingsSectionHeader("Protected Folders") {
                    Button {
                        model.addProtectedFolder()
                    } label: {
                        Label("Add Folder", systemImage: "plus")
                    }
                    .buttonStyle(PrimaryButtonStyle())
                }
                RuleTable(paths: model.protectedPaths) { path in
                    model.removeProtectedPath(path)
                }
                Text("A project is also protected when its root contains a `.dev-cleaner-keep` file. Glob rules (`keep_globs`, `keep_project_roots`) can be added in the config file.")
                    .font(.caption)
                    .foregroundStyle(DCColor.tertiary)
            }
        case "trash":
            SettingsPanel(title: "Trash Retention", subtitle: "Trashed items still use disk space until they are purged.") {
                HStack(alignment: .top, spacing: 22) {
                    VStack(alignment: .leading, spacing: 18) {
                        SettingsSectionHeader("Storage Path") { EmptyView() }
                        Text(model.trashRoot.map(DevCleanerFormatters.shortPath) ?? "Loading...")
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundStyle(DCColor.secondary)
                            .textSelection(.enabled)
                            .padding(12)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(DCColor.elevated)
                            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                        Text("Items on other volumes stay on that volume, in a hidden `.dev-cleaner-trash` folder, so trashing never copies data onto this disk.")
                            .font(.caption)
                            .foregroundStyle(DCColor.tertiary)
                        ToggleStepperRow(
                            title: "Purge old batches automatically",
                            subtitle: "Runs at launch and after each cleanup. Batches older than this are deleted permanently.",
                            isOn: $model.preferences.autoGcTrash,
                            value: $model.preferences.trashRetentionDays,
                            suffix: "days",
                            range: 1...180
                        )
                        StepperRow(
                            title: "Warn when Trash exceeds",
                            subtitle: "Trash and Dashboard show a warning above this size. Recent batches are never purged automatically.",
                            value: Binding(
                                get: { Int(model.preferences.trashLimitGb) },
                                set: { model.preferences.trashLimitGb = UInt64($0) }
                            ),
                            suffix: "GB",
                            range: 1...200
                        )
                    }
                    DCCard {
                        VStack(spacing: 16) {
                            Text("CURRENT SIZE")
                                .font(.caption.weight(.bold))
                                .foregroundStyle(DCColor.tertiary)
                            Text(DevCleanerFormatters.bytes(model.trashTotalBytes))
                                .font(.system(size: 32, weight: .bold))
                            Button("Empty Trash Now") {
                                confirmEmpty = true
                            }
                            .buttonStyle(PrimaryButtonStyle(color: DCColor.red))
                            .disabled(model.trashBatches.isEmpty)
                            .emptyTrashConfirmation(isPresented: $confirmEmpty, model: model)
                            Text("Emptying is permanent.")
                                .font(.caption2)
                                .foregroundStyle(DCColor.tertiary)
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .frame(width: 260)
                }
            }
        default:
            SettingsPanel(title: "Scanning", subtitle: "What Dev Cleaner looks for and what it shows.") {
                StepperRow(
                    title: "Scan depth",
                    subtitle: "How many folder levels below each scan root are searched.",
                    value: $model.preferences.scanDepth,
                    suffix: "levels",
                    range: 1...12
                )
                StepperRow(
                    title: "Recent-change protection",
                    subtitle: "Items modified within this window are hidden and never cleaned.",
                    value: $model.preferences.recentDays,
                    suffix: "days",
                    range: 1...60
                )
                ToggleRow(
                    title: "Show recently modified items",
                    subtitle: "List items inside the protection window so they can be cleaned deliberately.",
                    isOn: $model.preferences.includeRecent
                )
                ToggleRow(
                    title: "Show high-risk items",
                    subtitle: "Dependencies such as node_modules and .venv, and folders matched only by .gitignore. Hidden by default.",
                    isOn: $model.preferences.showHighRisk
                )
                Text("Scan settings apply to the next scan.")
                    .font(.caption)
                    .foregroundStyle(DCColor.tertiary)
            }
        }
    }
}

private struct SettingsTab: View {
    var title: String
    var id: String
    @Binding var tab: String
    var body: some View {
        Button(title) { tab = id }
            .buttonStyle(.plain)
            .foregroundStyle(tab == id ? DCColor.blue : DCColor.secondary)
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(tab == id ? DCColor.blue.opacity(0.16) : .clear)
            .clipShape(RoundedRectangle(cornerRadius: 8))
    }
}

private struct SettingsPanel<Content: View>: View {
    var title: String
    var subtitle: String
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.title2.bold())
                Text(subtitle).foregroundStyle(DCColor.secondary)
            }
            DCCard {
                VStack(alignment: .leading, spacing: 14) {
                    content
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct SettingsSectionHeader<Trailing: View>: View {
    var title: String
    @ViewBuilder var trailing: Trailing

    init(_ title: String, @ViewBuilder trailing: () -> Trailing) {
        self.title = title
        self.trailing = trailing()
    }

    var body: some View {
        HStack {
            Text(title)
                .font(.headline)
            Spacer()
            trailing
        }
    }
}

private struct ToggleRow: View {
    var title: String
    var subtitle: String
    @Binding var isOn: Bool

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.system(size: 13, weight: .semibold))
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(DCColor.tertiary)
            }
            Spacer()
            Toggle("", isOn: $isOn)
                .labelsHidden()
        }
        .padding(.vertical, 8)
    }
}

private struct StepperRow: View {
    var title: String
    var subtitle: String
    @Binding var value: Int
    var suffix: String
    var range: ClosedRange<Int>

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.system(size: 13, weight: .semibold))
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(DCColor.tertiary)
            }
            Spacer()
            Stepper("\(value) \(suffix)", value: $value, in: range)
                .frame(width: 160, alignment: .trailing)
        }
        .padding(.vertical, 8)
    }
}

private struct ToggleStepperRow: View {
    var title: String
    var subtitle: String
    @Binding var isOn: Bool
    @Binding var value: Int
    var suffix: String
    var range: ClosedRange<Int>

    var body: some View {
        HStack {
            ToggleRow(title: title, subtitle: subtitle, isOn: $isOn)
            Stepper("\(value) \(suffix)", value: $value, in: range)
                .frame(width: 150, alignment: .trailing)
                .disabled(!isOn)
        }
    }
}

private struct RuleTable: View {
    var paths: [String]
    var remove: (String) -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Rule Type").frame(width: 140, alignment: .leading)
                Text("Name / Pattern").frame(maxWidth: .infinity, alignment: .leading)
                Text("Scope").frame(width: 160, alignment: .leading)
                Text("Status").frame(width: 100, alignment: .leading)
            }
            .font(.caption.weight(.bold))
            .foregroundStyle(DCColor.secondary)
            .padding(.vertical, 10)
            if rows.isEmpty {
                Text("No protected folders yet.")
                    .foregroundStyle(DCColor.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.vertical, 14)
            }
            ForEach(rows, id: \.rawValue) { row in
                HStack {
                    Text(row.type).frame(width: 140, alignment: .leading)
                    Text(row.pattern).font(.system(size: 12, design: .monospaced)).frame(maxWidth: .infinity, alignment: .leading)
                    Text(row.scope).foregroundStyle(DCColor.secondary).frame(width: 160, alignment: .leading)
                    DCPill(title: row.status, color: row.status == "Active" ? DCColor.green : DCColor.secondary, filled: row.status == "Active")
                        .frame(width: 100, alignment: .leading)
                    if row.removable {
                        Button {
                            remove(row.rawValue)
                        } label: {
                            Image(systemName: "minus.circle")
                        }
                        .buttonStyle(.plain)
                        .foregroundStyle(DCColor.secondary)
                    } else {
                        Image(systemName: "lock")
                            .foregroundStyle(DCColor.tertiary)
                    }
                }
                .font(.system(size: 13))
                .padding(.vertical, 14)
                Divider().overlay(DCColor.border)
            }
        }
    }

    private var rows: [RuleRow] {
        paths.map {
            RuleRow(type: "Folder", pattern: DevCleanerFormatters.shortPath($0), scope: "Folder and subfolders", status: "Active", rawValue: $0, removable: true)
        }
    }
}

private struct RuleRow {
    var type: String
    var pattern: String
    var scope: String
    var status: String
    var rawValue: String
    var removable: Bool
}

struct EmptyState: View {
    var title: String
    var systemImage: String
    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: systemImage).font(.largeTitle)
            Text(title).font(.headline)
        }
        .foregroundStyle(DCColor.secondary)
        .frame(maxWidth: .infinity, minHeight: 180)
    }
}
