import SwiftUI

private let AMBER = Color.open
private let GREEN = Color.accent
private let REPLACED = Color(hex: 0x6b6b75)

struct Dec {
    let id, title, thread, state, quote, supersededBy, day: String
    let sourceId, chunkId: Int?
    let subjects: [String]
    let outside: Bool
    let because: [J]

    init(_ j: J) {
        id = j["id"].str; title = j["title"].str; thread = j["thread"].str; state = j["state"].str
        quote = j["quote"].str; supersededBy = j["superseded_by"].str; day = j["day"].str
        sourceId = j["source_id"].isNull ? nil : j["source_id"].int
        chunkId = j["chunk_id"].isNull ? nil : j["chunk_id"].int
        subjects = j["subjects"].arr.map(\.str)
        outside = j["outside"].bool
        because = j["because"].arr
    }

    var live: Bool { state == "settled" && supersededBy.isEmpty }
}

struct DLink {
    let a, b, kind, reason: String
    init(_ j: J) { a = j["a"].str; b = j["b"].str; kind = j["kind"].str; reason = j["reason"].str }
}

private func restateHeads(_ decs: [Dec], _ links: [DLink]) -> [String: Dec] {
    var parent = Dictionary(uniqueKeysWithValues: decs.map { ($0.id, $0.id) })
    func find(_ x: String) -> String {
        var r = x
        while let p = parent[r], p != r { r = p }
        return r
    }
    for l in links where l.kind == "restates" && parent[l.a] != nil && parent[l.b] != nil { parent[find(l.a)] = find(l.b) }
    var groups: [String: [Dec]] = [:]
    for d in decs { groups[find(d.id), default: []].append(d) }
    var head: [String: Dec] = [:]
    for members in groups.values {
        let sorted = members.sorted { $0.day < $1.day }
        let top = sorted.first(where: { $0.live }) ?? sorted.last!
        for m in members { head[m.id] = top }
    }
    return head
}

private func chainOf(_ start: String, _ links: [DLink]) -> Set<String> {
    var seen = Set<String>(), stack = [start]
    while let cur = stack.popLast() {
        if !seen.insert(cur).inserted { continue }
        for l in links {
            if l.a == cur { stack.append(l.b) }
            if l.b == cur { stack.append(l.a) }
        }
    }
    return seen
}

private func dayDate(_ d: String) -> Double {
    let f = DateFormatter(); f.dateFormat = "yyyy-MM-dd"
    return f.date(from: String(d.prefix(10)))?.timeIntervalSince1970 ?? 0
}

private func star(_ i: Int) -> (CGFloat, CGFloat, CGFloat) {
    let x = CGFloat((i * 977) % 1000) / 1000
    let y = CGFloat((i * 613) % 1000) / 1000
    let r = CGFloat(i % 3) * 0.35 + 0.4
    return (x, y, r)
}

private let STARS: [(CGFloat, CGFloat, CGFloat)] = (0..<70).map(star)

private func drawStars(_ ctx: GraphicsContext, _ sz: CGSize) {
    for (x, y, r) in STARS {
        ctx.fill(Path(ellipseIn: CGRect(x: x * sz.width - r, y: y * sz.height - r, width: r * 2, height: r * 2)), with: .color(.white.opacity(0.12)))
    }
}

struct GraphView: View {
    @EnvironmentObject var store: Store
    @State private var mode = "decisions"
    var project: String?

    var body: some View {
        if mode == "mentions" {
            MentionsGraph(tabs: AnyView(GraphTabs(mode: $mode)))
        } else {
            DecisionsView(tabs: AnyView(GraphTabs(mode: $mode)), initialProject: project).id(project ?? "")
                .onAppear { if store.openTopics { mode = "mentions" } }
        }
    }
}

struct GraphTabs: View {
    @Binding var mode: String
    var body: some View {
        HStack(spacing: 6) {
            ForEach([("decisions", "Decisions"), ("mentions", "Mentions")], id: \.0) { key, label in
                Button { mode = key } label: {
                    Text(label).font(.system(size: 12.5, weight: .medium)).padding(.horizontal, 12).frame(height: 28)
                        .background(mode == key ? GREEN.opacity(0.15) : .white.opacity(0.04), in: Capsule())
                        .overlay(Capsule().strokeBorder(Color.line2, lineWidth: 1))
                        .foregroundStyle(mode == key ? GREEN : Color.soft)
                }
                .buttonStyle(.plain)
            }
        }
    }
}

struct DecisionsView: View {
    @EnvironmentObject var store: Store
    let tabs: AnyView
    @State private var project: String?
    @State private var focus: String?
    @State private var closed = false
    @State private var overview: J?
    @State private var view: J?

    init(tabs: AnyView, initialProject: String?) {
        self.tabs = tabs
        _project = State(initialValue: initialProject)
    }

    private var decisions: [Dec] { view?["decisions"].arr.map(Dec.init) ?? [] }
    private var links: [DLink] { view?["links"].arr.map(DLink.init) ?? [] }

    private var newestTurn: String? {
        let by = Dictionary(decisions.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return links.filter { $0.kind == "changed" && by[$0.a].map { !$0.outside } == true }
            .max { by[$0.a]!.day < by[$1.a]!.day }?.a
    }

    private var shownFocus: String? { closed ? nil : (focus ?? newestTurn) }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(alignment: .center, spacing: 16) {
                Text(project ?? "Graph").font(.system(size: 32, weight: .semibold)).tracking(-0.5)
                tabs
                Spacer()
                if project != nil {
                    Button { project = nil; focus = nil; closed = false } label: { Label("All projects", systemImage: "arrow.left") }
                        .buttonStyle(BtnStyle(kind: .ghost))
                }
            }
            if let project {
                if view != nil {
                    HStack(alignment: .top, spacing: 16) {
                        StrandsCanvas(project: project, decisions: decisions, links: links, focus: shownFocus) { id in
                            if id == shownFocus { closed = true } else { focus = id; closed = false }
                        }
                        .background(DarkStage())
                        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(Color.line, lineWidth: 1))
                        .overlay(alignment: .topTrailing) { Legend(items: [("dot", GREEN, "Now"), ("ring", REPLACED, "Replaced"), ("line", AMBER, "Changed")]) }
                        if let f = shownFocus {
                            ChainPanel(id: f, project: project) { closed = true }.frame(width: 360).id(f)
                        }
                    }
                } else {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            } else if let overview {
                OverviewCanvas(data: overview) { p in project = p; focus = nil; closed = false }
                    .background(DarkStage())
                    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(Color.line, lineWidth: 1))
                    .overlay(alignment: .topTrailing) {
                        Legend(items: [("dot", STATE_COLOR["moving"]!, STATE_NAME["moving"]!), ("dot", STATE_COLOR["built"]!, STATE_NAME["built"]!),
                                       ("line", AMBER, "Changed"), ("dash", Color.moving, "Built on")])
                    }
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .padding(.horizontal, 32).padding(.top, 28).padding(.bottom, 20)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(ZStack { Color.bg; PageGlow() })
        .task(id: "\(project ?? "")-\(store.tick)") {
            if let project {
                view = try? await API.get("/api/kg/decisions?project=\(API.q(project))")
            } else {
                overview = try? await API.get("/api/kg/decisions")
            }
        }
    }
}

private struct DarkStage: View {
    var body: some View {
        ZStack {
            Color(hex: 0x0c0c0f)
            RadialGradient(colors: [GREEN.opacity(0.07), .clear], center: UnitPoint(x: 0.7, y: 0.1), startRadius: 0, endRadius: 600)
        }
    }
}

private struct Legend: View {
    let items: [(String, Color, String)]
    var body: some View {
        HStack(spacing: 14) {
            ForEach(items, id: \.2) { shape, c, label in
                HStack(spacing: 6) {
                    switch shape {
                    case "ring": Circle().strokeBorder(c, lineWidth: 1.6).frame(width: 10, height: 10)
                    case "line": Capsule().fill(c).frame(width: 16, height: 2)
                    case "dash": DashLine().stroke(c, style: StrokeStyle(lineWidth: 2, dash: [3, 3])).frame(width: 16, height: 2)
                    default: Circle().fill(c).frame(width: 10, height: 10).shadow(color: c, radius: 4)
                    }
                    Text(label)
                }
            }
        }
        .font(.system(size: 12)).foregroundStyle(Color.mut)
        .padding(.horizontal, 18).padding(.top, 14)
        .allowsHitTesting(false)
    }
}

private struct DashLine: Shape {
    func path(in r: CGRect) -> Path { var p = Path(); p.move(to: CGPoint(x: 0, y: r.midY)); p.addLine(to: CGPoint(x: r.width, y: r.midY)); return p }
}

private struct OverviewCanvas: View {
    let data: J
    let open: (String) -> Void
    @State private var hover: String?
    @State private var cache = LayoutCache()

    private var projects: [J] { data["projects"].arr }
    private var edges: [J] { data["edges"].arr.filter { $0["kind"].str != "shared" } }
    private func radius(_ p: J) -> CGFloat { 10 + sqrt(CGFloat(p["n"].int)) * 2.6 }
    private func color(_ p: J) -> Color { STATE_COLOR[p["state"].str] ?? .mut }

    var body: some View {
        GeometryReader { geo in
            let sz = geo.size
            let pos = cache.positions(for: sz) { layout($0) }
            ZStack(alignment: .topLeading) {
                Canvas { ctx, sz in
                    drawStars(ctx, sz)
                    for e in edges {
                        guard let a = pos[e["a"].str], let b = pos[e["b"].str],
                              let pa = projects.first(where: { $0["name"].str == e["a"].str }),
                              let pb = projects.first(where: { $0["name"].str == e["b"].str }) else { continue }
                        let dx = b.x - a.x, dy = b.y - a.y, d = max(hypot(dx, dy), 1)
                        let s = CGPoint(x: a.x + dx / d * (radius(pa) + 4), y: a.y + dy / d * (radius(pa) + 4))
                        let t = CGPoint(x: b.x - dx / d * (radius(pb) + 6), y: b.y - dy / d * (radius(pb) + 6))
                        let m = CGPoint(x: (s.x + t.x) / 2 - dy * 0.15, y: (s.y + t.y) / 2 + dx * 0.15)
                        var p = Path(); p.move(to: s); p.addQuadCurve(to: t, control: m)
                        let lit = hover == nil || e["a"].str == hover || e["b"].str == hover
                        if e["kind"].str == "changed" {
                            ctx.stroke(p, with: .color(AMBER.opacity(lit ? 0.9 : 0.06)), lineWidth: lit && hover != nil ? 2.4 : 1.6)
                            let ang = atan2(t.y - m.y, t.x - m.x)
                            var head = Path()
                            head.move(to: t)
                            head.addLine(to: CGPoint(x: t.x - 9 * cos(ang - 0.45), y: t.y - 9 * sin(ang - 0.45)))
                            head.addLine(to: CGPoint(x: t.x - 9 * cos(ang + 0.45), y: t.y - 9 * sin(ang + 0.45)))
                            head.closeSubpath()
                            ctx.fill(head, with: .color(AMBER.opacity(lit ? 0.9 : 0.06)))
                        } else {
                            ctx.stroke(p, with: .color(Color.moving.opacity(lit ? 0.5 : 0.05)), style: StrokeStyle(lineWidth: 1.2, dash: [4, 5]))
                        }
                    }
                    for p in projects {
                        guard let q = pos[p["name"].str] else { continue }
                        let r = radius(p), c = color(p)
                        var g = ctx
                        g.opacity = near(p["name"].str) ? 1 : 0.18
                        g.fill(Path(ellipseIn: CGRect(x: q.x - r - 14, y: q.y - r - 14, width: (r + 14) * 2, height: (r + 14) * 2)), with: .color(c.opacity(0.12)))
                        let body = Path(ellipseIn: CGRect(x: q.x - r, y: q.y - r, width: r * 2, height: r * 2))
                        g.fill(body, with: .radialGradient(Gradient(colors: [.white.opacity(0.55), c, c.opacity(0.6)]),
                                                           center: CGPoint(x: q.x - r * 0.3, y: q.y - r * 0.4), startRadius: 0, endRadius: r * 1.2))
                        if p["turns"].int > 0 {
                            let frac = min(Double(p["turns"].int), 12) / 30
                            var arc = Path()
                            arc.addArc(center: q, radius: r + 3, startAngle: .degrees(-90), endAngle: .degrees(-90 + 360 * frac), clockwise: false)
                            g.stroke(arc, with: .color(AMBER.opacity(0.75)), lineWidth: 1.5)
                        }
                        g.draw(Text(p["name"].str).font(.system(size: 13, weight: .semibold)).foregroundColor(Color(hex: 0xe4e4e7)),
                               at: CGPoint(x: q.x, y: q.y + r + 9), anchor: .top)
                    }
                }
                if let h = hover, let p = projects.first(where: { $0["name"].str == h }), let q = pos[h] {
                    let r = radius(p)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(dayFmt(p["headline"]["day"].str)).font(.system(size: 11.5)).foregroundStyle(Color.mut)
                        Text(p["headline"]["title"].str).font(.system(size: 13.5)).lineLimit(3)
                    }
                    .padding(.horizontal, 14).padding(.vertical, 10)
                    .frame(width: 270, alignment: .leading)
                    .background(Color(hex: 0x0f0f12).opacity(0.96), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
                    .offset(x: min(max(q.x - 135, 12), sz.width - 282), y: q.y + r + 30 > sz.height - 110 ? q.y - r - 100 : q.y + r + 30)
                    .allowsHitTesting(false)
                }
            }
            .contentShape(Rectangle())
            .onTapGesture(coordinateSpace: .local) { loc in if let n = nodeAt(loc, pos) { open(n) } }
            .onContinuousHover(coordinateSpace: .local) { phase in
                switch phase {
                case .active(let loc): let n = nodeAt(loc, pos); if n != hover { hover = n }
                case .ended: hover = nil
                }
            }
        }
    }

    private func near(_ n: String) -> Bool {
        guard let h = hover else { return true }
        return n == h || edges.contains { ($0["a"].str == h && $0["b"].str == n) || ($0["b"].str == h && $0["a"].str == n) }
    }

    private func nodeAt(_ loc: CGPoint, _ pos: [String: CGPoint]) -> String? {
        projects.first { p in pos[p["name"].str].map { hypot($0.x - loc.x, $0.y - loc.y) <= radius(p) + 8 } ?? false }?["name"].str
    }

    private func layout(_ sz: CGSize) -> [String: CGPoint] {
        let W = sz.width, H = sz.height
        let linked = Set(edges.flatMap { [$0["a"].str, $0["b"].str] })
        let core = projects.filter { linked.contains($0["name"].str) }, loose = projects.filter { !linked.contains($0["name"].str) }
        var p: [String: CGPoint] = [:]
        let ga = Double.pi * (3 - sqrt(5.0))
        for (i, x) in core.enumerated() {
            let r = 40 + sqrt(Double(i + 1)) * 46
            p[x["name"].str] = CGPoint(x: W / 2 + cos(Double(i) * ga) * r * 1.5, y: H / 2 + sin(Double(i) * ga) * r)
        }
        for (i, x) in loose.enumerated() {
            let a = Double(i) / Double(max(loose.count, 1)) * .pi * 2 + 0.3
            p[x["name"].str] = CGPoint(x: W / 2 + cos(a) * W * 0.42, y: H / 2 + sin(a) * H * 0.4)
        }
        let rad = Dictionary(uniqueKeysWithValues: projects.map { ($0["name"].str, radius($0)) })
        let names = projects.map { $0["name"].str }
        for _ in 0..<500 {
            for i in names.indices { for j in (i + 1)..<names.count {
                let a = p[names[i]]!, b = p[names[j]]!
                let dx = a.x - b.x, dy = a.y - b.y, d = max(hypot(dx, dy), 1)
                let want = rad[names[i]]! + rad[names[j]]! + 90
                if d < want {
                    let k = (want - d) / d * 0.5
                    p[names[i]] = CGPoint(x: a.x + dx * k, y: a.y + dy * k)
                    p[names[j]] = CGPoint(x: b.x - dx * k, y: b.y - dy * k)
                }
            } }
            for e in edges {
                guard let a = p[e["a"].str], let b = p[e["b"].str] else { continue }
                let dx = b.x - a.x, dy = b.y - a.y, d = max(hypot(dx, dy), 1), k = (d - 190) / d * 0.02
                p[e["a"].str] = CGPoint(x: a.x + dx * k, y: a.y + dy * k)
                p[e["b"].str] = CGPoint(x: b.x - dx * k, y: b.y - dy * k)
            }
            for n in names {
                var q = p[n]!
                let r = rad[n]!
                q.x += (W / 2 - q.x) * 0.004; q.y += (H / 2 - q.y) * 0.006
                q.x = min(W - r - 70, max(r + 70, q.x)); q.y = min(H - r - 40, max(r + 30, q.y))
                p[n] = q
            }
        }
        return p
    }
}

private final class LayoutCache {
    private var size: CGSize = .zero
    private var pos: [String: CGPoint] = [:]

    func positions(for sz: CGSize, compute: (CGSize) -> [String: CGPoint]) -> [String: CGPoint] {
        if sz.width > 0, sz != size || pos.isEmpty {
            size = sz
            pos = compute(sz)
        }
        return pos
    }
}

private struct StrandNode { let d: Dec; let strand: String; let now: Bool }

private struct StrandModel {
    var head: [String: Dec] = [:]
    var strands: [String] = []
    var nodes: [StrandNode] = []
    var arcs: [(Dec, Dec)] = []

    init(project: String, decisions: [Dec], links: [DLink], focus: String?) {
        head = restateHeads(decisions, links)
        var seenHead = Set<String>(), heads: [Dec] = []
        for d in decisions { let h = head[d.id]!; if seenHead.insert(h.id).inserted { heads.append(h) } }
        let all = links.filter { $0.kind == "changed" }.compactMap { l -> (Dec, Dec)? in
            guard let f = head[l.b], let t = head[l.a], f.id != t.id else { return nil }
            return (f, t)
        }
        let chained = Set(all.flatMap { [$0.0.id, $0.1.id] })
        var rank: [String: Int] = [:]
        for d in decisions { for s in d.subjects where s != project { rank[s, default: 0] += 1 } }
        func subj(_ d: Dec) -> String {
            d.subjects.filter { $0 != project }.max { (rank[$0] ?? 0) < (rank[$1] ?? 0) } ?? project
        }
        var score: [String: (c: Int, n: Int)] = [:]
        for h in heads { let k = subj(h); var v = score[k] ?? (0, 0); v.n += 1; if chained.contains(h.id) { v.c += 1 }; score[k] = v }
        let lit = focus.map { chainOf($0, links) } ?? []
        let head = self.head
        let must = Array(Set(heads.filter { h in lit.contains(h.id) || decisions.contains { head[$0.id]?.id == h.id && lit.contains($0.id) } }.map(subj)))
        let ranked = score.sorted { $0.value.c != $1.value.c ? $0.value.c > $1.value.c : $0.value.n > $1.value.n }.map(\.key).filter { !must.contains($0) }
        strands = Array((must + ranked).prefix(max(6, must.count)))
        for st in strands {
            let mine = heads.filter { subj($0) == st && !$0.outside }
            let current = mine.filter(\.live).max { $0.day < $1.day }
            for h in heads where subj(h) == st && chained.contains(h.id) { nodes.append(StrandNode(d: h, strand: st, now: h.id == current?.id)) }
            if let current, !chained.contains(current.id) { nodes.append(StrandNode(d: current, strand: st, now: true)) }
        }
        let shown = Set(nodes.map(\.d.id))
        arcs = all.filter { shown.contains($0.0.id) && shown.contains($0.1.id) }
    }
}

private struct StrandsCanvas: View {
    let project: String
    let decisions: [Dec]
    let links: [DLink]
    let focus: String?
    let pick: (String) -> Void
    @State private var tip: StrandNode?

    var body: some View {
        let model = StrandModel(project: project, decisions: decisions, links: links, focus: focus)
        let H = CGFloat(120 + model.strands.count * 84)
        GeometryReader { geo in
            let W = geo.size.width
            let L: CGFloat = 150, R: CGFloat = 230
            let days = model.nodes.map { dayDate($0.d.day) }
            let t0 = days.min() ?? 0, t1 = days.max() ?? 0
            let xOf = { (d: String) -> CGFloat in L + CGFloat(t1 == t0 ? 0.5 : (dayDate(d) - t0) / (t1 - t0)) * max(120, W - L - R) }
            let yOf = { (s: String) -> CGFloat in 56 + CGFloat(model.strands.firstIndex(of: s) ?? 0) * 84 }
            let px = spread(model, xOf)
            let lit = focus.map { chainOf($0, links) }
            let on = { (id: String) -> Bool in
                guard let lit else { return true }
                return lit.contains(id) || decisions.contains { model.head[$0.id]?.id == id && lit.contains($0.id) }
            }
            ZStack(alignment: .topLeading) {
                Canvas { ctx, sz in
                    drawStars(ctx, sz)
                    for st in model.strands {
                        let xs = model.nodes.filter { $0.strand == st }.map { px[$0.d.id]! }
                        guard let a = xs.min(), let b = xs.max() else { continue }
                        var p = Path(); p.move(to: CGPoint(x: a - 60, y: yOf(st))); p.addLine(to: CGPoint(x: b, y: yOf(st)))
                        ctx.stroke(p, with: .linearGradient(Gradient(colors: [Color(hex: 0x3a3a44, opacity: 0), Color(hex: 0x4a4a55)]),
                                                            startPoint: CGPoint(x: a - 60, y: 0), endPoint: CGPoint(x: b, y: 0)), lineWidth: 2)
                    }
                    var trunk = Path(); trunk.move(to: CGPoint(x: L - 20, y: H - 44)); trunk.addLine(to: CGPoint(x: W - R + 40, y: H - 44))
                    ctx.stroke(trunk, with: .linearGradient(Gradient(colors: [GREEN.opacity(0), GREEN]), startPoint: CGPoint(x: L - 20, y: 0), endPoint: CGPoint(x: W - R + 40, y: 0)), lineWidth: 2)
                    if t1 > t0 {
                        for f in [0, 0.25, 0.5, 0.75, 1.0] {
                            let t = t0 + f * (t1 - t0)
                            let d = Date(timeIntervalSince1970: t)
                            let o = DateFormatter(); o.locale = Locale(identifier: "en_GB"); o.dateFormat = "d MMM"
                            ctx.draw(Text(o.string(from: d)).font(.system(size: 11.5)).foregroundColor(.mut),
                                     at: CGPoint(x: L + CGFloat(f) * max(120, W - L - R), y: H - 20))
                        }
                    }
                    for (f, t) in model.arcs {
                        guard let fn = model.nodes.first(where: { $0.d.id == f.id }), let tn = model.nodes.first(where: { $0.d.id == t.id }) else { continue }
                        let a = CGPoint(x: px[f.id]!, y: yOf(fn.strand)), b = CGPoint(x: px[t.id]!, y: yOf(tn.strand))
                        let lift: CGFloat = a.y == b.y ? -max(26, abs(b.x - a.x) * 0.22) : 0
                        var p = Path(); p.move(to: a)
                        p.addCurve(to: b, control1: CGPoint(x: a.x + (b.x - a.x) * 0.4, y: a.y + lift), control2: CGPoint(x: b.x - (b.x - a.x) * 0.4, y: b.y + lift))
                        let bright = lit == nil || (on(f.id) && on(t.id))
                        ctx.stroke(p, with: .color(AMBER.opacity(bright ? (lit == nil ? 0.7 : 1) : 0.06)), lineWidth: bright && lit != nil ? 2.6 : 1.8)
                    }
                    for n in model.nodes {
                        let c = CGPoint(x: px[n.d.id]!, y: yOf(n.strand))
                        var g = ctx
                        g.opacity = on(n.d.id) ? 1 : 0.15
                        if n.d.live {
                            let r: CGFloat = n.now ? 8 : 6
                            g.fill(Path(ellipseIn: CGRect(x: c.x - r - 9, y: c.y - r - 9, width: (r + 9) * 2, height: (r + 9) * 2)), with: .color(GREEN.opacity(0.14)))
                            g.fill(Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2)), with: .color(GREEN))
                            if n.now { g.stroke(Path(ellipseIn: CGRect(x: c.x - 15, y: c.y - 15, width: 30, height: 30)), with: .color(GREEN.opacity(0.4)), lineWidth: 1) }
                        } else {
                            let ring = Path(ellipseIn: CGRect(x: c.x - 5.5, y: c.y - 5.5, width: 11, height: 11))
                            g.fill(ring, with: .color(Color(hex: 0x0e0e11)))
                            g.stroke(ring, with: .color(REPLACED), lineWidth: 1.6)
                        }
                        if focus != nil && lit?.contains(n.d.id) == true {
                            g.stroke(Path(ellipseIn: CGRect(x: c.x - 11, y: c.y - 11, width: 22, height: 22)), with: .color(.white.opacity(0.7)), lineWidth: 1)
                        }
                    }
                }
                ForEach(model.strands, id: \.self) { st in
                    Text(st).font(.system(size: 13, weight: .semibold)).foregroundStyle(Color.soft).lineLimit(1)
                        .frame(width: 118, alignment: .leading)
                        .offset(x: 20, y: yOf(st) - 9).opacity(lit == nil ? 1 : 0.4)
                }
                ForEach(model.nodes.filter(\.now), id: \.d.id) { n in
                    Text(n.d.title).font(.system(size: 13)).lineLimit(2).frame(width: 200, alignment: .leading)
                        .offset(x: px[n.d.id]! + 18, y: yOf(n.strand) - 17).opacity(on(n.d.id) ? 1 : 0.15)
                        .allowsHitTesting(false)
                }
                if let t = tip, !t.d.outside {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(dayFmt(t.d.day)).font(.system(size: 11.5)).foregroundStyle(Color.mut)
                        Text(t.d.title).font(.system(size: 12.5)).lineLimit(3)
                    }
                    .padding(.horizontal, 12).padding(.vertical, 8).frame(width: 260, alignment: .leading)
                    .background(Color(hex: 0x0f0f12).opacity(0.96), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
                    .offset(x: min(px[t.d.id]! + 14, W - 270), y: yOf(t.strand) + 14)
                    .allowsHitTesting(false)
                }
            }
            .contentShape(Rectangle())
            .onTapGesture(coordinateSpace: .local) { loc in if let n = nodeAt(loc, model, px, yOf) { pick(n.d.id) } }
            .onContinuousHover(coordinateSpace: .local) { phase in
                switch phase {
                case .active(let loc): let n = nodeAt(loc, model, px, yOf); if n?.d.id != tip?.d.id { tip = n }
                case .ended: tip = nil
                }
            }
        }
        .frame(height: H)
    }

    private func spread(_ m: StrandModel, _ xOf: (String) -> CGFloat) -> [String: CGFloat] {
        var out: [String: CGFloat] = [:]
        for st in m.strands {
            var last = -CGFloat.infinity
            for n in m.nodes.filter({ $0.strand == st }).sorted(by: { xOf($0.d.day) < xOf($1.d.day) }) {
                let v = max(xOf(n.d.day), last + 20); out[n.d.id] = v; last = v
            }
        }
        return out
    }

    private func nodeAt(_ loc: CGPoint, _ m: StrandModel, _ px: [String: CGFloat], _ yOf: (String) -> CGFloat) -> StrandNode? {
        m.nodes.min { hypot(px[$0.d.id]! - loc.x, yOf($0.strand) - loc.y) < hypot(px[$1.d.id]! - loc.x, yOf($1.strand) - loc.y) }
            .flatMap { hypot(px[$0.d.id]! - loc.x, yOf($0.strand) - loc.y) <= 16 ? $0 : nil }
    }
}

private struct ChainPanel: View {
    @EnvironmentObject var store: Store
    let id: String
    let project: String
    let close: () -> Void
    @State private var data: J?

    private struct Step: Identifiable { let h: Dec; let also: [Dec]; let because: [J]; var id: String { h.id } }

    private var levels: [[Step]] {
        guard let data else { return [] }
        let decs = data["decisions"].arr.map(Dec.init), links = data["links"].arr.map(DLink.init)
        let head = restateHeads(decs, links)
        var seen = Set<String>(), heads: [Dec] = []
        for d in decs { let h = head[d.id]!; if seen.insert(h.id).inserted { heads.append(h) } }
        var depth = Dictionary(uniqueKeysWithValues: heads.map { ($0.id, 0) })
        let changed = links.filter { $0.kind == "changed" }.compactMap { l -> (String, String)? in
            guard let f = head[l.b]?.id, let t = head[l.a]?.id, f != t else { return nil }
            return (f, t)
        }
        for _ in heads { for (f, t) in changed { depth[t] = max(depth[t] ?? 0, (depth[f] ?? 0) + 1) } }
        var out: [[Step]] = []
        for h in heads.sorted(by: { $0.day < $1.day }) {
            let d = depth[h.id] ?? 0
            while out.count <= d { out.append([]) }
            let members = decs.filter { head[$0.id]?.id == h.id }
            out[d].append(Step(h: h, also: members.filter { $0.id != h.id }, because: members.flatMap(\.because)))
        }
        return out.filter { !$0.isEmpty }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                if data == nil { ProgressView().frame(maxWidth: .infinity).padding(30) }
                ForEach(Array(levels.enumerated()), id: \.offset) { i, level in
                    VStack(alignment: .leading, spacing: 16) {
                        ForEach(level) { step in stepView(step) }
                    }
                    .padding(.leading, 28).padding(.bottom, i < levels.count - 1 ? 24 : 0)
                    .overlay(alignment: .topLeading) {
                        if i < levels.count - 1 {
                            LinearGradient(colors: [AMBER, AMBER.opacity(0.35)], startPoint: .top, endPoint: .bottom)
                                .frame(width: 2).padding(.top, 12).padding(.leading, 7)
                        }
                    }
                }
            }
            .padding(20)
        }
        .frame(maxHeight: .infinity)
        .panel(radius: 18)
        .overlay(alignment: .topTrailing) {
            Button(action: close) { Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)) }
                .buttonStyle(BtnStyle(kind: .ghost)).padding(12)
        }
        .task { data = try? await API.get("/api/kg/chain?id=\(API.q(id))") }
    }

    @ViewBuilder private func stepView(_ s: Step) -> some View {
        let alive = s.h.live
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Text(dayFmt(s.h.day)).foregroundStyle(alive ? GREEN : Color.mut)
                if alive { Text("Now").font(.system(size: 11, weight: .medium)).padding(.horizontal, 8).padding(.vertical, 1).background(GREEN.opacity(0.14), in: Capsule()).foregroundStyle(GREEN) }
                if s.h.thread != project { Text(s.h.thread).font(.system(size: 11)).padding(.horizontal, 8).padding(.vertical, 1).overlay(Capsule().strokeBorder(Color.line2)).foregroundStyle(Color.soft) }
            }
            .font(.system(size: 12))
            Button {
                if let sid = s.h.sourceId { store.go(.source(sid, chunk: s.h.chunkId)) }
            } label: {
                Text(s.h.quote.isEmpty ? s.h.title : "“\(s.h.quote)”").font(.system(size: 17)).foregroundStyle(alive ? Color.ink : Color.soft)
                    .multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
            }
            .buttonStyle(.plain)
            if !s.h.quote.isEmpty { Text(s.h.title).font(.system(size: 12.5)).foregroundStyle(Color.mut).fixedSize(horizontal: false, vertical: true) }
            if !s.because.isEmpty {
                FlowChips(items: s.because) { b in
                    if !b["source_id"].isNull { store.go(.source(b["source_id"].int, chunk: b["chunk_id"].isNull ? nil : b["chunk_id"].int)) }
                }
            }
            if !s.also.isEmpty {
                DisclosureGroup("Said \(s.also.count + 1) times") {
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(s.also, id: \.id) { m in Text("\(dayFmt(m.day)): “\(m.quote.isEmpty ? m.title : m.quote)”").fixedSize(horizontal: false, vertical: true) }
                    }
                    .padding(.top, 4)
                }
                .font(.system(size: 12)).foregroundStyle(Color.mut)
            }
        }
        .overlay(alignment: .topLeading) {
            Group {
                if alive { Circle().fill(GREEN).shadow(color: GREEN, radius: 6) }
                else { Circle().fill(Color(hex: 0x0e0e11)).overlay(Circle().strokeBorder(REPLACED, lineWidth: 1.6)) }
            }
            .frame(width: 16, height: 16).offset(x: -28, y: 0)
        }
    }
}

private struct FlowChips: View {
    let items: [J]
    let open: (J) -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(items, id: \.id) { b in
                Button { open(b) } label: {
                    HStack(spacing: 5) {
                        Image(systemName: "lightbulb").font(.system(size: 10))
                        Text(b["title"].str).lineLimit(1)
                    }
                    .font(.system(size: 12)).padding(.horizontal, 10).padding(.vertical, 4)
                    .background(Color.purple.opacity(0.13), in: Capsule()).foregroundStyle(Color.purple)
                }
                .buttonStyle(.plain).help(b["title"].str)
            }
        }
    }
}
