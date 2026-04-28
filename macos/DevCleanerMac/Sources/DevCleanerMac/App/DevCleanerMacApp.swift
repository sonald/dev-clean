import SwiftUI

@main
struct DevCleanerMacApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup("Dev Cleaner") {
            ContentView()
                .environmentObject(model)
                .frame(minWidth: 1024, minHeight: 720)
                .preferredColorScheme(model.preferences.appearance == "system" ? nil : .dark)
                .task {
                    await model.bootstrap()
                }
        }
        .windowStyle(.hiddenTitleBar)
        .commands {
            CommandGroup(replacing: .newItem) { }
            CommandMenu("Dev Cleaner") {
                Button("Smart Scan") {
                    Task { await model.smartScan() }
                }
                .keyboardShortcut("r", modifiers: [.command])

                Button("Clean with Trash") {
                    model.presentCleanupPlan(mode: .trash)
                }
                .keyboardShortcut(.delete, modifiers: [.command])
                .disabled(model.selectedProjects.isEmpty)
            }
        }

        Settings {
            SettingsView()
                .environmentObject(model)
                .frame(width: 860, height: 620)
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }
}
