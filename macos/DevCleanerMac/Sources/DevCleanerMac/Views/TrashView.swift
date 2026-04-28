import SwiftUI

struct TrashView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                HStack {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Trash & Recovery")
                            .font(.title2.bold())
                        Text("Recoverable batches are stored locally until retention or manual purge.")
                            .foregroundStyle(DCColor.secondary)
                    }
                    Spacer()
                    Button {
                        Task { await model.refreshTrash() }
                    } label: {
                        Label("Refresh", systemImage: "arrow.clockwise")
                    }
                    .buttonStyle(SecondaryButtonStyle())
                }

                HStack(alignment: .top, spacing: 20) {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("Recoverable Batches")
                            .font(.headline)
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
                            Text("Recovery Policy")
                                .font(.headline)
                            Text("Items moved to Dev Cleaner Trash are stored locally and can be restored until the retention period expires or manual emptying occurs.")
                                .foregroundStyle(DCColor.secondary)
                            Divider().overlay(DCColor.border)
                            Text("Retention: \(model.preferences.trashRetentionDays) days")
                            Text("Size limit: \(model.preferences.trashLimitGb) GB")
                            Button {
                                if let oldest = model.trashBatches.last {
                                    Task { await model.purge(batch: oldest) }
                                }
                            } label: {
                                Label("Empty Oldest Batch", systemImage: "trash.slash")
                                    .frame(maxWidth: .infinity)
                            }
                            .buttonStyle(PrimaryButtonStyle(color: DCColor.red))
                            .disabled(model.trashBatches.isEmpty)
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
