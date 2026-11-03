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
                if model.isCleaning {
                    CleanupProgressOverlay()
                        .environmentObject(model)
                }
                if model.showCleanupSummary, let summary = model.lastSummary {
                    CleanupSummaryOverlay(summary: summary)
                        .environmentObject(model)
                }
            }
        }
        .background(DCColor.window)
        .sheet(item: $model.pendingCleanup) { pending in
            CleanupPlanSheet(pending: pending)
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
