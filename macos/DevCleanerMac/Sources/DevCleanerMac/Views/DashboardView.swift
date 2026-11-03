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
                                Text("Macintosh HD")
                                    .font(.system(size: 18, weight: .semibold))
                                Spacer()
                                Text("500 GB Total")
                                    .font(.system(size: 12, design: .monospaced))
                                    .foregroundStyle(DCColor.tertiary)
                            }
                            StorageBar(cleanableBytes: model.totalCleanableBytes)
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

                DCCard {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("Protected Directories")
                            .font(.headline)
                        ProtectedRow(path: "~/.ssh", subtitle: "SSH Keys & Configs")
                        ProtectedRow(path: "~/.aws", subtitle: "AWS Credentials")
                        ProtectedRow(path: "~/Projects/Active", subtitle: "Current Workspaces")
                    }
                }
            }
            .padding(24)
        }
        .foregroundStyle(DCColor.text)
    }
}

private struct StorageBar: View {
    var cleanableBytes: UInt64

    var body: some View {
        GeometryReader { proxy in
            let width = proxy.size.width
            ZStack(alignment: .leading) {
                Capsule().fill(DCColor.tertiary.opacity(0.30))
                Capsule()
                    .fill(DCColor.blue)
                    .frame(width: width * 0.55)
                Capsule()
                    .fill(DCColor.green)
                    .frame(width: max(10, width * min(Double(cleanableBytes) / Double(500 * 1024 * 1024 * 1024), 0.25)))
                    .offset(x: width * 0.55)
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
