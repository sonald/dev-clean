import SwiftUI

struct TrashView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                DCPageHeader(
                    title: "Trash & Recovery",
                    subtitle: "Recoverable batches are stored locally until retention or manual purge."
                ) {
                    Button {
                        Task { await model.refreshTrash() }
                    } label: {
                        Label("Refresh", systemImage: "arrow.clockwise")
                    }
                    .buttonStyle(SecondaryButtonStyle())
                }

                HStack(alignment: .top, spacing: 20) {
                    VStack(alignment: .leading, spacing: 12) {
                        HStack {
                            Text("Recoverable Batches")
                                .font(.headline)
                            Spacer()
                            Text("Total: \(DevCleanerFormatters.bytes(model.trashTotalBytes))")
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(DCColor.tertiary)
                        }
                        ForEach(model.trashBatches) { batch in
                            TrashBatchCard(batch: batch)
                        }
                        if model.trashBatches.isEmpty {
                            EmptyState(title: "Trash is Empty", systemImage: "trash")
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .topLeading)

                    DCCard {
                        VStack(alignment: .leading, spacing: 14) {
                            Text("Empty Trash")
                                .font(.headline)
                            Text("Permanently delete all recoverable items. This action cannot be undone and will free \(DevCleanerFormatters.bytes(model.trashTotalBytes)).")
                                .font(.system(size: 13))
                                .foregroundStyle(DCColor.secondary)
                            Button {
                                Task { await model.emptyTrash() }
                            } label: {
                                Label("Empty Trash Now", systemImage: "trash.slash")
                                    .frame(maxWidth: .infinity)
                            }
                            .buttonStyle(PrimaryButtonStyle(color: DCColor.red))
                            .disabled(model.trashBatches.isEmpty)
                            Divider().overlay(DCColor.border)
                            Text("Recovery Policy")
                                .font(.headline)
                            Text("Retention: \(model.preferences.trashRetentionDays) days")
                            Text("Size limit: \(model.preferences.trashLimitGb) GB")
                                .foregroundStyle(DCColor.secondary)
                        }
                    }
                    .frame(width: 320)
                }
            }
            .padding(24)
        }
        .foregroundStyle(DCColor.text)
    }
}

private struct TrashBatchCard: View {
    @EnvironmentObject private var model: AppModel
    var batch: TrashBatch

    private var isExpanded: Bool {
        model.selectedTrashBatchID == batch.batchId && !model.trashEntries.isEmpty
    }

    var body: some View {
        DCCard {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    VStack(alignment: .leading) {
                        Text("Batch - \(DevCleanerFormatters.date(batch.createdAt))")
                            .font(.headline)
                        Text("\(batch.entriesCount) items • \(DevCleanerFormatters.bytes(batch.totalSize))")
                            .foregroundStyle(DCColor.secondary)
                    }
                    Spacer()
                    Button("Restore Batch") {
                        Task { await model.restore(batch: batch) }
                    }
                    .buttonStyle(SecondaryButtonStyle())
                }
                if isExpanded {
                    VStack(spacing: 0) {
                        ForEach(model.trashEntries.prefix(8)) { entry in
                            HStack {
                                Text(DevCleanerFormatters.shortPath(entry.originalPath))
                                    .font(.system(size: 12, design: .monospaced))
                                    .lineLimit(1)
                                Spacer()
                                Text(DevCleanerFormatters.bytes(entry.size))
                                    .font(.system(size: 12, design: .monospaced))
                                    .foregroundStyle(DCColor.tertiary)
                            }
                            .padding(.vertical, 8)
                        }
                    }
                    .padding(.top, 6)
                }
                HStack {
                    Button(isExpanded ? "Hide Items" : "View Items") {
                        if isExpanded {
                            model.trashEntries.removeAll()
                            model.selectedTrashBatchID = nil
                        } else {
                            Task { await model.showTrashEntries(batch: batch) }
                        }
                    }
                    .buttonStyle(GhostButtonStyle())
                    Spacer()
                    Button(role: .destructive) {
                        Task { await model.purge(batch: batch) }
                    } label: {
                        Label("Delete Permanently", systemImage: "trash")
                    }
                    .buttonStyle(PrimaryButtonStyle(color: DCColor.red))
                }
            }
        }
    }
}
