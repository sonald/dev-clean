import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var tab = "general"

    var body: some View {
        HStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Dev Cleaner")
                    .font(.headline)
                    .padding(.bottom, 8)
                SettingsTab(title: "General", id: "general", tab: $tab)
                SettingsTab(title: "Scan Roots", id: "roots", tab: $tab)
                SettingsTab(title: "Protection Rules", id: "protection", tab: $tab)
                SettingsTab(title: "Trash Retention", id: "trash", tab: $tab)
                SettingsTab(title: "Alerts", id: "alerts", tab: $tab)
                Spacer()
                Button {
                    model.startSmartScan()
                } label: {
                    Label("Start Deep Scan", systemImage: "scope")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(SecondaryButtonStyle())
                Button {
                    Task { await model.saveConfig() }
                } label: {
                    Label("Save Changes", systemImage: "checkmark")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(PrimaryButtonStyle())
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
    }

    @ViewBuilder
    private var settingsContent: some View {
        switch tab {
        case "roots":
            SettingsPanel(title: "Scan Roots", subtitle: "Configure primary directories Dev Cleaner analyzes.") {
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
                SettingsSectionHeader("Scan Behavior") { EmptyView() }
                ToggleRow(
                    title: "Follow Symlinks",
                    subtitle: "Resolve symbolic links to their target directories during the scan.",
                    isOn: $model.preferences.followSymlinks
                )
                ToggleRow(
                    title: "Ignore Hidden Directories",
                    subtitle: "Skip directories starting with a dot to improve scan speed.",
                    isOn: $model.preferences.ignoreHiddenDirs
                )
            }
        case "protection":
            SettingsPanel(title: "Protection Rules", subtitle: "Protected paths take precedence over all scan settings.") {
                SettingsSectionHeader("Rules") {
                    Button {
                        model.addProtectedFolder()
                    } label: {
                        Label("Add Rule", systemImage: "plus")
                    }
                    .buttonStyle(PrimaryButtonStyle())
                }
                RuleTable(paths: model.protectedPaths) { path in
                    model.removeProtectedPath(path)
                }
                SettingsSectionHeader("Automatic Protection") { EmptyView() }
                ToggleStepperRow(
                    title: "Protect Recent Files",
                    subtitle: "Exclude files accessed within the last configured number of days.",
                    isOn: $model.preferences.protectRecentFiles,
                    value: $model.preferences.protectRecentDays,
                    suffix: "days",
                    range: 1...60
                )
                ToggleStepperRow(
                    title: "Protect Large Files",
                    subtitle: "Auto-protect files larger than threshold.",
                    isOn: $model.preferences.protectLargeFiles,
                    value: Binding(
                        get: { Int(model.preferences.protectLargeFileGb) },
                        set: { model.preferences.protectLargeFileGb = UInt64($0) }
                    ),
                    suffix: "GB",
                    range: 1...100
                )
                SettingsSectionHeader("Risk Overrides") { EmptyView() }
                ToggleRow(
                    title: "High Risk Items",
                    subtitle: "Never delete system-critical binaries.",
                    isOn: $model.preferences.protectHighRiskItems
                )
                ToggleRow(
                    title: "Integrity Verification",
                    subtitle: "Scan for file signatures before action.",
                    isOn: $model.preferences.verifyFileSignatures
                )
            }
        case "trash":
            SettingsPanel(title: "Trash Retention", subtitle: "Manage deleted files and recoverable batches.") {
                HStack(alignment: .top, spacing: 22) {
                    VStack(alignment: .leading, spacing: 18) {
                        SettingsSectionHeader("Storage Path") { EmptyView() }
                        Text("~/Library/Application Support/dev-cleaner/trash")
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundStyle(DCColor.secondary)
                            .padding(12)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(DCColor.elevated)
                            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                        ToggleStepperRow(
                            title: "Automatic cleanup policy",
                            subtitle: "Remove files that have exceeded the retention period.",
                            isOn: $model.preferences.autoGcTrash,
                            value: $model.preferences.trashRetentionDays,
                            suffix: "days",
                            range: 1...180
                        )
                        StepperRow(
                            title: "Maximum trash size limit",
                            subtitle: "Oldest files are removed when trash exceeds this size.",
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
                                Task { await model.emptyTrash() }
                            }
                            .buttonStyle(PrimaryButtonStyle(color: DCColor.red))
                            .disabled(model.trashBatches.isEmpty)
                            Text("Immediate deletion is permanent.")
                                .font(.caption2)
                                .foregroundStyle(DCColor.tertiary)
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .frame(width: 260)
                }
                Text("Files moved to the Dev Cleaner Trash are stored locally and can be restored to their original locations until the retention period expires or manual emptying occurs.")
                    .font(.caption)
                    .foregroundStyle(DCColor.indigo)
                    .padding(12)
                    .background(DCColor.indigo.opacity(0.10))
                    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            }
        case "alerts":
            SettingsPanel(title: "Alerts Settings", subtitle: "Manage system alerts and thresholds.") {
                ToggleRow(title: "Enable alerts", subtitle: "Allow Dev Cleaner to notify you about cleanup activity.", isOn: $model.preferences.alertsEnabled)
                ToggleRow(title: "Warn Before Large Cleanup", subtitle: "Show a confirmation before actions exceed the configured threshold.", isOn: $model.preferences.alertBeforeLargeCleanup)
                ToggleRow(title: "Notify After Cleanup", subtitle: "Send a completion alert after cleanup or restore actions finish.", isOn: $model.preferences.alertAfterCleanup)
                ToggleRow(title: "Protected Skip Alerts", subtitle: "Notify when protected paths are skipped by policy.", isOn: $model.preferences.alertForProtectedSkips)
                StepperRow(
                    title: "Large cleanup threshold",
                    subtitle: "Any cleanup above this size requires explicit attention.",
                    value: Binding(
                        get: { Int(model.preferences.notificationThresholdGb) },
                        set: { model.preferences.notificationThresholdGb = UInt64($0) }
                    ),
                    suffix: "GB",
                    range: 1...100
                )
            }
        default:
            SettingsPanel(title: "General Settings", subtitle: "Manage how Dev Cleaner starts and operates.") {
                ToggleRow(title: "Launch at Login", subtitle: "Start Dev Cleaner when you sign in.", isOn: $model.preferences.launchAtLogin)
                ToggleRow(title: "Menubar Icon", subtitle: "Keep quick access visible in the macOS menu bar.", isOn: $model.preferences.showMenubarIcon)
                Picker("Appearance", selection: $model.preferences.appearance) {
                    Text("Light").tag("light")
                    Text("Dark").tag("dark")
                    Text("System").tag("system")
                }
                .pickerStyle(.segmented)
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
            ForEach(rows, id: \.pattern) { row in
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
        let configured = paths.map {
            RuleRow(type: "Folder", pattern: DevCleanerFormatters.shortPath($0), scope: "System-wide", status: "Active", rawValue: $0, removable: true)
        }
        if !configured.isEmpty {
            return configured
        }
        return [
            RuleRow(type: "Folder", pattern: "~/.ssh", scope: "System-wide", status: "Active", rawValue: "~/.ssh", removable: false),
            RuleRow(type: "Folder", pattern: "~/.aws", scope: "System-wide", status: "Active", rawValue: "~/.aws", removable: false),
            RuleRow(type: "Category", pattern: "Active project roots", scope: "System-wide", status: "Active", rawValue: "Active project roots", removable: false)
        ]
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
