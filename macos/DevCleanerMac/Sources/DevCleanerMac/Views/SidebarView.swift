import SwiftUI

struct SidebarView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Dev Cleaner")
                    .font(.system(size: 18, weight: .bold))
                    .foregroundStyle(.white)
                Text("System Maintenance")
                    .font(.caption)
                    .foregroundStyle(DCColor.secondary)
            }
            .padding(.horizontal, 20)
            .padding(.top, 18)
            .padding(.bottom, 26)

            ForEach(AppSection.sidebarSections) { section in
                Button {
                    withAnimation(.easeInOut(duration: 0.16)) {
                        model.selectedSection = section
                    }
                } label: {
                    HStack(spacing: 12) {
                        Image(systemName: section.systemImage)
                            .frame(width: 16)
                        Text(section.title)
                            .lineLimit(1)
                        Spacer()
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 9)
                    .foregroundStyle(model.selectedSection == section ? DCColor.blue : DCColor.secondary)
                    .background(model.selectedSection == section ? DCColor.blue.opacity(0.14) : .clear)
                    .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                }
                .buttonStyle(.plain)
                .padding(.horizontal, 12)
            }

            Spacer()

            if model.isScanning || model.isCleaning {
                VStack(alignment: .leading, spacing: 8) {
                    Text(model.isScanning ? "Scanning" : "Cleaning")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(DCColor.secondary)
                    ProgressView(value: model.isScanning ? model.scanProgress : model.cleanupProgress)
                        .tint(DCColor.blue)
                    Text(model.isScanning ? model.scanStatusMessage : "\(model.cleanupProcessedCount)/\(max(model.cleanupTotalCount, 1)) processed")
                        .font(.caption2)
                        .foregroundStyle(DCColor.tertiary)
                        .lineLimit(1)
                }
                .padding(14)
                .background(DCColor.panel.opacity(0.65))
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                .padding(.horizontal, 14)
                .padding(.bottom, 14)
            }
        }
        .background(DCColor.sidebar.opacity(0.92))
    }
}
