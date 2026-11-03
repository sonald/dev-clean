import SwiftUI

struct DashboardView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                DCPageHeader(
                    title: "System Overview",
                    subtitle: "Your development environment has \(DevCleanerFormatters.bytes(model.totalCleanableBytes)) of cleanable space."
                ) {
                    Button {
                        if model.isScanning {
                            model.stopScan()
                        } else {
                            model.startSmartScan()
                        }
                    } label: {
                        Label(model.isScanning ? "Stop" : "Smart Scan", systemImage: model.isScanning ? "stop.circle" : "scope")
                    }
                    .buttonStyle(PrimaryButtonStyle(color: model.isScanning ? DCColor.red : DCColor.blue))
                }

                DCCard {
                    HStack(spacing: 12) {
                        Image(systemName: "folder")
                            .foregroundStyle(DCColor.blue)
                            .frame(width: 22)
                        VStack(alignment: .leading, spacing: 4) {
                            Text(model.scanRootDisplayName)
                                .font(.headline)
                                .lineLimit(1)
                            Text(model.scanStatusMessage)
                                .font(.caption)
                                .foregroundStyle(DCColor.secondary)
                                .lineLimit(1)
                        }
                        Spacer()
                        Button {
                            model.chooseScanRoot()
                        } label: {
                            Label("Choose Folder", systemImage: "folder.badge.plus")
                        }
                        .buttonStyle(SecondaryButtonStyle())
                        .disabled(model.isScanning)
                        Button {
                            model.resetScanRootToHome()
                        } label: {
                            Label("Reset", systemImage: "house")
                        }
                        .buttonStyle(SecondaryButtonStyle())
                        .disabled(model.isScanning)
                    }
                }

                HStack(spacing: 16) {
                    DCCard {
                        VStack(alignment: .leading, spacing: 18) {
                            HStack {
                                Text(model.volume?.name ?? "Volume unavailable")
                                    .font(.system(size: 18, weight: .semibold))
                                Spacer()
                                if let volume = model.volume {
                                    Text("\(DevCleanerFormatters.bytes(volume.availableBytes)) free of \(DevCleanerFormatters.bytes(volume.totalBytes))")
                                        .font(.system(size: 12, design: .monospaced))
                                        .foregroundStyle(DCColor.tertiary)
                                }
                            }
                            if let volume = model.volume {
                                StorageBar(volume: volume, cleanableBytes: model.totalCleanableBytes)
                            }
                            HStack {
                                LegendDot(title: "Used", color: DCColor.blue)
                                LegendDot(title: "Cleanable", color: DCColor.green)
                                LegendDot(title: "Free", color: DCColor.tertiary)
                                Spacer()
                            }
                        }
                    }
                    DCCard {
                        VStack(spacing: 6) {
                            Image(systemName: "shippingbox")
                                .font(.system(size: 28))
                                .foregroundStyle(DCColor.green)
                            Text(DevCleanerFormatters.bytes(model.totalCleanableBytes))
                                .font(.system(size: 28, weight: .bold))
                            Text("POTENTIAL SAVINGS")
                                .font(.caption)
                                .foregroundStyle(DCColor.secondary)
                            if !model.hiddenHighRiskProjects.isEmpty {
                                Text("+ \(DevCleanerFormatters.bytes(model.hiddenHighRiskProjects.reduce(0) { $0 + $1.size })) high-risk hidden")
                                    .font(.caption2)
                                    .foregroundStyle(DCColor.tertiary)
                            }
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .frame(width: 280)
                }

                DCCard {
                    HStack(spacing: 18) {
                        Image(systemName: "sparkles")
                            .foregroundStyle(DCColor.indigo)
                            .frame(width: 38, height: 38)
                            .background(DCColor.indigo.opacity(0.16))
                            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Smart Recommendations")
                                .font(.headline)
                            Text("Generate a targeted cleanup plan from the current scan root and safety policy.")
                                .foregroundStyle(DCColor.secondary)
                        }
                        Spacer()
                        Button {
                            model.selectedSection = .recommendations
                        } label: {
                            Label("Configure Plan", systemImage: "arrow.right")
                        }
                        .buttonStyle(SecondaryButtonStyle())
                    }
                }

                if let summary = model.lastSummary {
                    CleanupSummaryView(summary: summary)
                }

                HStack(alignment: .top, spacing: 16) {
                    DCCard {
                        VStack(alignment: .leading, spacing: 12) {
                            HStack {
                                Text("Protected Folders")
                                    .font(.headline)
                                Spacer()
                                Button("Manage") { model.selectedSection = .settings }
                                    .buttonStyle(GhostButtonStyle())
                            }
                            if model.protectedPaths.isEmpty {
                                Text("No protected folders. Recently modified, in-use, and `.dev-cleaner-keep` projects are always skipped.")
                                    .font(.caption)
                                    .foregroundStyle(DCColor.secondary)
                            }
                            ForEach(model.protectedPaths.prefix(5), id: \.self) { path in
                                ProtectedRow(path: DevCleanerFormatters.shortPath(path), subtitle: "Never listed or cleaned")
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    DCCard {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Text("Dev Cleaner Trash")
                                    .font(.headline)
                                Spacer()
                                Button("Open") { model.selectedSection = .trash }
                                    .buttonStyle(GhostButtonStyle())
                            }
                            Text(DevCleanerFormatters.bytes(model.trashTotalBytes))
                                .font(.system(size: 24, weight: .bold))
                                .foregroundStyle(model.trashOverLimit ? DCColor.yellow : DCColor.text)
                            Text(model.trashOverLimit
                                 ? "Over the \(model.preferences.trashLimitGb) GB warning limit. Trashed items still use disk space."
                                 : "\(model.trashBatches.count) recoverable batches. Space is released when batches are purged.")
                                .font(.caption)
                                .foregroundStyle(DCColor.secondary)
                        }
                    }
                    .frame(width: 280)
                }
            }
            .padding(24)
        }
        .foregroundStyle(DCColor.text)
    }
}

private struct StorageBar: View {
    var volume: VolumeInfo
    var cleanableBytes: UInt64

    var body: some View {
        GeometryReader { proxy in
            let width = proxy.size.width
            let total = Double(max(volume.totalBytes, 1))
            let used = Double(volume.usedBytes) / total
            // Cleanable bytes are part of "used"; draw them as the tail of the used segment.
            let cleanable = min(Double(cleanableBytes) / total, used)
            ZStack(alignment: .leading) {
                Capsule().fill(DCColor.tertiary.opacity(0.30))
                Capsule()
                    .fill(DCColor.blue)
                    .frame(width: width * used)
                Capsule()
                    .fill(DCColor.green)
                    .frame(width: width * cleanable)
                    .offset(x: width * (used - cleanable))
            }
        }
        .frame(height: 16)
    }
}

private struct LegendDot: View {
    var title: String
    var color: Color

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(color)
                .frame(width: 8, height: 8)
            Text(title)
                .font(.system(size: 13))
                .foregroundStyle(DCColor.secondary)
        }
    }
}

private struct ProtectedRow: View {
    var path: String
    var subtitle: String

    var body: some View {
        HStack {
            Image(systemName: "folder.badge.gearshape")
                .foregroundStyle(DCColor.secondary)
            VStack(alignment: .leading) {
                Text(path).font(.subheadline.weight(.semibold))
                Text(subtitle).font(.caption).foregroundStyle(DCColor.secondary)
            }
            Spacer()
            Label("Safe", systemImage: "circle.fill")
                .font(.caption.weight(.semibold))
                .foregroundStyle(DCColor.green)
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(DCColor.green.opacity(0.12))
                .clipShape(Capsule())
        }
        .padding(.vertical, 6)
    }
}
