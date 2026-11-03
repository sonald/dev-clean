import SwiftUI

struct HistoryView: View {
    @EnvironmentObject private var model: AppModel
    @State private var query = ""

    var body: some View {
        VStack(spacing: 0) {
            DCTopBar {
                Text("History & Audit")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(DCColor.secondary)
                Spacer()
                TextField("Search by batch ID or type...", text: $query)
                    .textFieldStyle(.plain)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 7)
                    .background(DCColor.panel2)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .frame(width: 260)
                Button {
                    Task { await model.exportAuditCSV() }
                } label: {
                    Label("Export CSV", systemImage: "square.and.arrow.up")
                }
                .buttonStyle(SecondaryButtonStyle())
                Button {
                    Task { await model.refreshAudit() }
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
                .buttonStyle(SecondaryButtonStyle())
            }

            HStack {
                Text("Reclaimed: \(DevCleanerFormatters.bytes(filteredRuns.reduce(0) { $0 + $1.reclaimedBytes }))")
                    .font(.system(size: 13, weight: .semibold))
                    .help("Permanent deletes and Trash purges. Dry runs and moves to Trash do not free space.")
                Picker("Audit Filter", selection: $model.auditFilter) {
                    ForEach(AuditFilter.allCases) { filter in
                        Text(filter.title).tag(filter)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 360)
                Spacer()
                Text("Showing \(filteredRuns.count) entries")
                    .font(.caption)
                    .foregroundStyle(DCColor.tertiary)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 16)

            Table(filteredRuns) {
                TableColumn("BATCH ID / TYPE") { run in
                    VStack(alignment: .leading) {
                        Text(run.runId).font(.system(.body, design: .monospaced))
                        Text(run.command.capitalized).foregroundStyle(DCColor.secondary)
                    }
                }
                TableColumn("DATE & TIME") { run in
                    Text(run.startedDate.map { DevCleanerFormatters.date($0) } ?? run.startedAt ?? "--")
                        .foregroundStyle(DCColor.secondary)
                }
                TableColumn("RESULT") { run in
                    Text(run.resultLabel)
                        .foregroundStyle(run.reclaimedBytes > 0 ? DCColor.text : DCColor.secondary)
                }
                TableColumn("STATUS") { run in
                    Text(run.statusLabel)
                        .foregroundStyle(run.failed > 0 ? DCColor.yellow : run.mode == "dry_run" ? DCColor.secondary : DCColor.green)
                }
            }
            .scrollContentBackground(.hidden)
        }
        .foregroundStyle(DCColor.text)
    }

    private var filteredRuns: [AuditRun] {
        let runs = model.filteredAuditRuns
        guard !query.isEmpty else { return runs }
        return runs.filter { $0.runId.localizedCaseInsensitiveContains(query) || $0.command.localizedCaseInsensitiveContains(query) }
    }
}

extension AuditRun {
    var resultLabel: String {
        let size = DevCleanerFormatters.bytes(freedBytes)
        switch mode {
        case "dry_run": return "Would free \(size)"
        case "trash": return "Moved \(size) to Trash"
        case "permanent_delete": return "Freed \(size)"
        case "purge", "gc": return "Purged \(size) from Trash"
        case "restore": return "Restored \(cleaned) items"
        default: return command == "undo" ? "Restored \(cleaned) items" : size
        }
    }

    var statusLabel: String {
        if mode == "dry_run" { return "Dry run" }
        if failed > 0 { return "Partial" }
        if finishedAt == nil { return "Incomplete" }
        return skipped > 0 ? "Done, \(skipped) skipped" : "Success"
    }
}
