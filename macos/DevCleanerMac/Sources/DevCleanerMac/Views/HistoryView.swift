import SwiftUI

struct HistoryView: View {
    @EnvironmentObject private var model: AppModel
    @State private var query = ""

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("History & Audit")
                    .font(.title2.bold())
                Spacer()
                TextField("Search by batch ID or type...", text: $query)
                    .textFieldStyle(.plain)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 7)
                    .background(DCColor.panel2)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .frame(width: 260)
                Button {
                    Task { await model.refreshAudit() }
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
                .buttonStyle(SecondaryButtonStyle())
            }
            .padding(20)

            Table(filteredRuns) {
                TableColumn("BATCH ID / TYPE") { run in
                    VStack(alignment: .leading) {
                        Text(run.runId).font(.system(.body, design: .monospaced))
                        Text(run.command.capitalized).foregroundStyle(DCColor.secondary)
                    }
                }
                TableColumn("DATE & TIME") { run in
                    Text(run.startedAt ?? "--")
                        .foregroundStyle(DCColor.secondary)
                }
                TableColumn("SIZE RECLAIMED") { run in
                    Text(DevCleanerFormatters.bytes(run.freedBytes))
                }
                TableColumn("STATUS") { run in
                    Text(run.failed > 0 ? "Partial" : "Success")
                        .foregroundStyle(run.failed > 0 ? DCColor.yellow : DCColor.green)
                }
            }
            .scrollContentBackground(.hidden)
        }
        .foregroundStyle(DCColor.text)
    }

    private var filteredRuns: [AuditRun] {
        guard !query.isEmpty else { return model.auditRuns }
        return model.auditRuns.filter { $0.runId.localizedCaseInsensitiveContains(query) || $0.command.localizedCaseInsensitiveContains(query) }
    }
}
