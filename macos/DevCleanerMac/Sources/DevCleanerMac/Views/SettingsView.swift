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
                Text("Named scan profiles are preserved in the Rust config. Use CLI profile commands for precise root editing in this first native build.")
                    .foregroundStyle(DCColor.secondary)
            }
        case "protection":
            SettingsPanel(title: "Protection Rules", subtitle: "Protected paths take precedence over all scan settings.") {
                Text("Core protection maps to keep_paths, keep_globs, and keep_project_roots.")
                    .foregroundStyle(DCColor.secondary)
            }
        case "trash":
            SettingsPanel(title: "Trash Retention", subtitle: "Manage deleted files and recoverable batches.") {
                Stepper("Retention: \(model.preferences.trashRetentionDays) days", value: $model.preferences.trashRetentionDays, in: 1...180)
                Stepper("Maximum trash size: \(model.preferences.trashLimitGb) GB", value: $model.preferences.trashLimitGb, in: 1...200)
                Button {
                    Task { await model.refreshTrash() }
                } label: {
                    Label("Refresh Trash", systemImage: "arrow.clockwise")
                }
                .buttonStyle(SecondaryButtonStyle())
            }
        case "alerts":
            SettingsPanel(title: "Alerts Settings", subtitle: "Manage system alerts and thresholds.") {
                Toggle("Enable alerts", isOn: $model.preferences.alertsEnabled)
                Stepper("Large cleanup threshold: \(model.preferences.notificationThresholdGb) GB", value: $model.preferences.notificationThresholdGb, in: 1...100)
            }
        default:
            SettingsPanel(title: "General Settings", subtitle: "Manage how Dev Cleaner starts and operates.") {
                Toggle("Launch at login", isOn: $model.preferences.launchAtLogin)
                Toggle("Menubar Icon", isOn: $model.preferences.showMenubarIcon)
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
