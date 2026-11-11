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
                // The palette in DesignTokens is dark-only.
                .preferredColorScheme(.dark)
                .task {
                    await model.bootstrap()
                }
        }
        .windowStyle(.hiddenTitleBar)
        .commands {
            CommandGroup(replacing: .newItem) { }
            CommandMenu("Dev Cleaner") {
                Button(model.isScanning ? "Stop Scan" : "Smart Scan") {
                    if model.isScanning {
                        model.stopScan()
                    } else {
                        model.startSmartScan()
                    }
                }
                .keyboardShortcut("r", modifiers: [.command])
                .disabled(model.isCleaning)

                Button("Move Selected to Trash") {
                    model.presentCleanup(mode: .trash, items: model.selectedProjects)
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
