import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        HStack(spacing: 0) {
            SidebarView()
                .frame(width: 220)
            Divider().overlay(DCColor.border)
            ZStack {
                DCColor.window.ignoresSafeArea()
                currentSection
            }
        }
        .background(DCColor.window)
        .sheet(item: $model.presentedCleanupMode) { mode in
            CleanupPlanSheet(mode: mode)
                .environmentObject(model)
        }
        .alert(item: $model.alert) { alert in
            Alert(title: Text(alert.title), message: Text(alert.message), dismissButton: .default(Text("OK")))
        }
    }

    @ViewBuilder
    private var currentSection: some View {
        switch model.selectedSection {
        case .dashboard:
            DashboardView()
        case .scanResults:
            ScanResultsView()
        case .recommendations:
            RecommendationsView()
        case .trash:
            TrashView()
        case .history:
            HistoryView()
        case .settings:
            SettingsView()
        }
    }
}

extension CleanupMode: Identifiable {
    var id: String { rawValue }
}
