import SwiftUI

struct DashboardView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                HStack {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("System Overview")
                            .font(.system(size: 22, weight: .bold))
                        Text("Your development environment has \(DevCleanerFormatters.bytes(model.totalCleanableBytes)) of cleanable space.")
                            .foregroundStyle(DCColor.secondary)
                    }
                    Spacer()
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
                .foregroundStyle(DCColor.text)

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
                        VStack(alignment: .leading, spacing: 14) {
                            HStack {
                                Text("Macintosh HD")
                                    .font(.headline)
                                Spacer()
                                Text("Cleanable")
                                    .foregroundStyle(DCColor.secondary)
                            }
                            ProgressView(value: min(Double(model.totalCleanableBytes) / Double(500 * 1024 * 1024 * 1024), 1))
                                .tint(DCColor.green)
                            HStack {
                                Label("Scanned items \(model.projects.count)", systemImage: "folder")
                                Spacer()
                                Label("Selected \(DevCleanerFormatters.bytes(model.selectedBytes))", systemImage: "checkmark.circle")
                            }
                            .font(.caption)
                            .foregroundStyle(DCColor.secondary)
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
