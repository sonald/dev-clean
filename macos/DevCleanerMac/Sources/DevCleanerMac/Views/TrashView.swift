import SwiftUI

struct TrashView: View {
    @EnvironmentObject private var model: AppModel
    @State private var confirmEmpty = false

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

                if model.trashOverLimit {
                    Label("Trash holds \(DevCleanerFormatters.bytes(model.trashTotalBytes)), over your \(model.preferences.trashLimitGb) GB warning limit. Trashed items still use disk space until purged.", systemImage: "exclamationmark.triangle.fill")
                        .font(.system(size: 13))
                        .foregroundStyle(DCColor.yellow)
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(DCColor.yellow.opacity(0.10))
                        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
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
                                confirmEmpty = true
                            } label: {
                                Label("Empty Trash Now", systemImage: "trash.slash")
                                    .frame(maxWidth: .infinity)
                            }
                            .buttonStyle(PrimaryButtonStyle(color: DCColor.red))
                            .disabled(model.trashBatches.isEmpty)
                            .emptyTrashConfirmation(isPresented: $confirmEmpty, model: model)
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
    @State private var confirmPurge = false

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
                        confirmPurge = true
                    } label: {
                        Label("Delete Permanently", systemImage: "trash")
                    }
                    .buttonStyle(PrimaryButtonStyle(color: DCColor.red))
                    .confirmationDialog(
                        "Permanently delete this batch?",
                        isPresented: $confirmPurge,
                        titleVisibility: .visible
                    ) {
                        Button("Delete \(batch.entriesCount) Items Permanently", role: .destructive) {
                            Task { await model.purge(batch: batch) }
                        }
                    } message: {
                        Text("\(DevCleanerFormatters.bytes(batch.totalSize)) will be deleted and cannot be restored.")
                    }
                }
            }
        }
    }
}

extension View {
    /// Shared by the Trash pane and Settings so both paths ask before deleting everything.
    func emptyTrashConfirmation(isPresented: Binding<Bool>, model: AppModel) -> some View {
        confirmationDialog("Empty the Dev Cleaner Trash?", isPresented: isPresented, titleVisibility: .visible) {
            Button("Delete All \(model.trashBatches.count) Batches Permanently", role: .destructive) {
                Task { await model.emptyTrash() }
            }
        } message: {
            Text("\(DevCleanerFormatters.bytes(model.trashTotalBytes)) will be deleted and cannot be restored.")
        }
    }
}
