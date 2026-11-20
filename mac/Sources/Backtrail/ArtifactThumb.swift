import SwiftUI
import WebKit

@MainActor
final class ThumbMaker: NSObject, WKNavigationDelegate {
    static let shared = ThumbMaker()
    private var cache: [Int: NSImage] = [:]
    private var busy = false
    private var waiters: [CheckedContinuation<Void, Never>] = []
    private var loaded: (() -> Void)?

    func image(for id: Int) async -> NSImage? {
        if let i = cache[id] { return i }
        while busy { await withCheckedContinuation { waiters.append($0) } }
        busy = true
        defer {
            busy = false
            if !waiters.isEmpty { waiters.removeFirst().resume() }
        }
        if let i = cache[id] { return i }
        guard let j = try? await API.get("/api/source_raw?id=\(id)"), !j["text"].str.isEmpty else { return nil }
        let img = await render(j["text"].str)
        if let img { cache[id] = img }
        return img
    }

    private func render(_ html: String) async -> NSImage? {
        let size = NSSize(width: 1280, height: 800)
        let web = WKWebView(frame: NSRect(origin: .zero, size: size))
        web.navigationDelegate = self
        let win = NSWindow(contentRect: NSRect(origin: NSPoint(x: -6000, y: -6000), size: size), styleMask: .borderless, backing: .buffered, defer: false)
        win.isReleasedWhenClosed = false
        win.contentView = web
        win.orderBack(nil)
        defer { win.close() }
        await withCheckedContinuation { (k: CheckedContinuation<Void, Never>) in
            var done = false
            let finish = { if !done { done = true; k.resume() } }
            loaded = finish
            web.loadHTMLString(html, baseURL: nil)
            DispatchQueue.main.asyncAfter(deadline: .now() + 8) { finish() }
        }
        try? await Task.sleep(nanoseconds: 700_000_000)
        let cfg = WKSnapshotConfiguration()
        cfg.rect = NSRect(origin: .zero, size: size)
        cfg.snapshotWidth = 520
        return try? await web.takeSnapshot(configuration: cfg)
    }

    nonisolated func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        Task { @MainActor in self.loaded?() }
    }
}

struct ArtifactThumb: View {
    let sourceID: Int
    @State private var img: NSImage?

    var body: some View {
        ZStack {
            Color(white: 0.08)
            if let img { Image(nsImage: img).resizable().scaledToFill() } else { ProgressView().controlSize(.small) }
        }
        .aspectRatio(16.0 / 10.0, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color.line))
        .task { img = await ThumbMaker.shared.image(for: sourceID) }
    }
}
