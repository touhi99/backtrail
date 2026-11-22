import Foundation

struct J {
    let v: Any?
    subscript(_ k: String) -> J { J(v: (v as? [String: Any])?[k]) }
    var s: String? { v as? String }
    var str: String { s ?? "" }
    var int: Int { (v as? NSNumber)?.intValue ?? Int(str) ?? 0 }
    var bool: Bool { (v as? NSNumber)?.boolValue ?? false }
    var arr: [J] { (v as? [Any])?.map { J(v: $0) } ?? [] }
    var isNull: Bool { v == nil || v is NSNull }
}

extension J: Identifiable {
    var id: String { self["id"].s ?? String(self["id"].int) }
}

enum API {
    static let base = "http://127.0.0.1:8799"
    private static let allowed: CharacterSet = {
        var c = CharacterSet.urlQueryAllowed
        c.remove(charactersIn: "&+=?#")
        return c
    }()
    static func q(_ s: String) -> String { s.addingPercentEncoding(withAllowedCharacters: allowed) ?? s }

    static func get(_ path: String) async throws -> J {
        let (data, _) = try await URLSession.shared.data(from: URL(string: base + path)!)
        let j = J(v: try JSONSerialization.jsonObject(with: data))
        lock.withLock { memo[path] = j }
        return j
    }

    private static let lock = NSLock()
    nonisolated(unsafe) private static var memo: [String: J] = [:]
    static func cached(_ path: String) -> J? {
        lock.withLock { memo[path] }
    }

    @discardableResult
    static func post(_ path: String, _ body: [String: Any]) async throws -> J {
        var rq = URLRequest(url: URL(string: base + path)!)
        rq.httpMethod = "POST"
        rq.setValue("application/json", forHTTPHeaderField: "Content-Type")
        rq.httpBody = try JSONSerialization.data(withJSONObject: body)
        let (data, _) = try await URLSession.shared.data(for: rq)
        return J(v: try JSONSerialization.jsonObject(with: data))
    }

    static func web(_ path: String) -> URL { URL(string: base + path)! }
}

enum Page: Hashable {
    case home
    case graph
    case corrections
    case decisions(String)
    case thread(String)
    case map(String)
    case source(Int, chunk: Int?)

    var label: String {
        switch self {
        case .home: "Home"
        case .graph: "Graph"
        case .corrections: "Corrections"
        case .decisions(let name), .thread(let name), .map(let name): name
        case .source: "Source"
        }
    }
}

@MainActor
final class Store: ObservableObject {
    @Published var today: J?
    @Published var offline = false
    @Published var page: Page = .home
    @Published var back: [Page] = []
    @Published var sheet: Sheet?
    var graphMode: String?

    enum Sheet: Identifiable, Equatable {
        case search(String, thread: String)
        case items
        case log
        var id: String {
            switch self {
            case .search(let q, let t): "search|\(q)|\(t)"
            case .items: "items"
            case .log: "log"
            }
        }
    }

    func go(_ p: Page) {
        sheet = nil
        guard p != page else { return }
        back.append(page)
        page = p
    }

    func goBack() {
        if let p = back.popLast() { page = p }
    }
    @Published var capture: String?? = nil
    @Published var tick = 0
    var openTopics = Snapshot.env["BACKTRAIL_TAB"] == "topics"
    @Published var toReview = 0

    func reload() async {
        do {
            let first = today == nil
            today = try await API.get("/api/today")
            if let s = try? await API.get("/api/kg/suggest") { toReview = s["entities"].arr.count + s["aliases"].arr.count }
            offline = false
            if first { prefetch() }
        } catch {
            offline = true
        }
    }

    private func prefetch() {
        let names = moving.map { $0["name"].str }
        Task.detached(priority: .utility) {
            await withTaskGroup(of: Void.self) { g in
                for n in names { g.addTask { _ = try? await API.get("/api/thread?name=\(API.q(n))") } }
                g.addTask { _ = await GraphData.load() }
            }
        }
    }

    func changed() {
        tick += 1
        Task { await reload() }
    }

    var threads: [J] { today?["threads"].arr ?? [] }
    var scope: String { if case .thread(let n) = page { n } else { "" } }
    var moving: [J] { threads.filter { $0["state"].str == "moving" } }
}

func dayFmt(_ t: String?) -> String {
    guard let t, t.count >= 10 else { return t ?? "" }
    let f = DateFormatter()
    f.dateFormat = "yyyy-MM-dd"
    guard let d = f.date(from: String(t.prefix(10))) else { return t }
    let o = DateFormatter()
    o.locale = Locale(identifier: "en_GB")
    o.dateFormat = "d MMM"
    return o.string(from: d)
}

func parseTS(_ t: String?) -> Date? {
    guard let t else { return nil }
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let d = f.date(from: t) { return d }
    f.formatOptions = [.withInternetDateTime]
    return f.date(from: t)
}
