import AppKit
import SwiftUI

/// Snapshot mode: BACKTRAIL_SNAPSHOT=/path.png renders the window offscreen to a PNG and exits.
/// BACKTRAIL_PAGE=home or thread:<name> picks the page; BACKTRAIL_SIZE=1280x860 sets the size.
enum Snapshot {
    static let env = ProcessInfo.processInfo.environment
    static var path: String? { env["BACKTRAIL_SNAPSHOT"] }

    static func page() -> Page {
        let p = env["BACKTRAIL_PAGE"] ?? "home"
        if p == "graph" { return .graph }
        if p.hasPrefix("thread:") { return .thread(String(p.dropFirst(7))) }
        if p.hasPrefix("search:") { return .search(String(p.dropFirst(7))) }
        if p.hasPrefix("source:") {
            let n = p.dropFirst(7).split(separator: ":").compactMap { Int($0) }
            return .source(n.first ?? 0, chunk: n.count > 1 ? n[1] : nil)
        }
        return .home
    }

    static func size() -> NSSize {
        let parts = (env["BACKTRAIL_SIZE"] ?? "1280x860").split(separator: "x").compactMap { Double($0) }
        return parts.count == 2 ? NSSize(width: parts[0], height: parts[1]) : NSSize(width: 1280, height: 860)
    }

    @MainActor static func run(to path: String) {
        NSApplication.shared.setActivationPolicy(.prohibited)
        let store = Store()
        store.page = page()
        let size = size()
        let host = NSHostingView(rootView: RootView().environmentObject(store).preferredColorScheme(.dark))
        host.frame = NSRect(origin: .zero, size: size)
        let win = NSWindow(contentRect: NSRect(origin: NSPoint(x: -20000, y: -20000), size: size),
                           styleMask: [.borderless], backing: .buffered, defer: false)
        win.contentView = host
        win.orderFrontRegardless()
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.0) {
            host.layoutSubtreeIfNeeded()
            guard let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) else { exit(1) }
            host.cacheDisplay(in: host.bounds, to: rep)
            try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
            exit(0)
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ n: Notification) {
        if let out = Snapshot.env["BACKTRAIL_ICON"] {
            NSApp.setActivationPolicy(.prohibited)
            NSApp.windows.forEach { $0.close() }
            Task { @MainActor in
                let r = ImageRenderer(content: AppIconArt())
                r.scale = 1
                if let img = r.nsImage, let tiff = img.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff) {
                    try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: out))
                }
                exit(0)
            }
            return
        }
        if let path = Snapshot.path {
            NSApp.windows.forEach { $0.close() }
            Task { @MainActor in Snapshot.run(to: path) }
        }
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ s: NSApplication) -> Bool { Snapshot.path == nil && Snapshot.env["BACKTRAIL_ICON"] == nil }
}

@main
struct BacktrailApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var delegate
    @StateObject private var store = Store()

    var body: some Scene {
        WindowGroup("Backtrail") {
            RootView()
                .environmentObject(store)
                .preferredColorScheme(.dark)
                .frame(minWidth: 980, minHeight: 640)
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 1280, height: 860)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("Add to a Thread…") { store.capture = .some(nil) }.keyboardShortcut("n")
            }
            CommandGroup(after: .toolbar) {
                Button("Reload") { store.changed() }.keyboardShortcut("r")
                Button("Search") { store.go(.search("")) }.keyboardShortcut("f")
                Button("Back") { store.goBack() }.keyboardShortcut("[")
                Button("Home") { store.go(.home) }.keyboardShortcut("1")
            }
        }
    }
}
