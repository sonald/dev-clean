import SwiftUI

struct SidebarView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Dev Cleaner")
                    .font(.system(size: 22, weight: .bold))
                    .foregroundStyle(.white)
                Text("SYSTEM MAINTENANCE")
                    .font(.caption)
                    .foregroundStyle(DCColor.secondary)
            }
            .padding(.horizontal, 18)
            .padding(.top, 20)
            .padding(.bottom, 18)

            ForEach(AppSection.allCases) { section in
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
                    .padding(.vertical, 8)
                    .foregroundStyle(model.selectedSection == section ? DCColor.blue : DCColor.secondary)
                    .background(model.selectedSection == section ? DCColor.blue.opacity(0.18) : .clear)
                    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                }
                .buttonStyle(.plain)
                .padding(.horizontal, 8)
                .padding(.vertical, 1)
            }

            Spacer()

            if model.isScanning || model.isCleaning {
                ProgressView(value: model.isScanning ? model.scanProgress : model.cleanupProgress)
                    .tint(DCColor.blue)
                    .padding()
            }
        }
        .background(.ultraThinMaterial)
    }
}
