import SwiftUI

let KIND_COLOR: [String: Color] = [
    "project": .moving, "service": .purple, "tool": .open, "model": Color(hex: 0xf87171), "site": .accent, "topic": Color(hex: 0xf472b6),
]
private let CLAIM_STATUS: [String: (String, Color)] = [
    "active": ("kept", .accent), "proposed": ("suggested", .mut), "superseded": ("replaced", .open), "retracted": ("rejected", .dropped),
]
private let GW: CGFloat = 1300, GH: CGFloat = 1000

private func shortName(_ s: String) -> String {
    s.replacingOccurrences(of: #" \(.*\)"#, with: "", options: .regularExpression)
}

private func nodeRadius(claims: Int) -> CGFloat { min(46, (9 + sqrt(Double(claims)) * 2.1) * 1.15) }

final class GraphData {
    nonisolated(unsafe) static var shared: GraphData?

    let nodes: [J]
    let ids: [String]
    let index: [String: Int]
    let names: [String]
    let kinds: [String]
    let colors: [Color]
    let radius: [CGFloat]
    let claims: [Int]
    let edges: [(a: Int, b: Int, w: CGFloat, topic: Bool)]
    let adj: [[(n: Int, w: Int)]]
    let pos: [CGPoint]
    let signature: String

    init(_ r: J) {
        let ns = r["nodes"].arr.filter { $0["claims"].int > 0 }
        nodes = ns
        ids = ns.map(\.id)
        index = Dictionary(uniqueKeysWithValues: ids.enumerated().map { ($1, $0) })
        names = ns.map { shortName($0["name"].str) }
        kinds = ns.map { $0["kind"].str }
        colors = kinds.map { KIND_COLOR[$0] ?? .mut }
        claims = ns.map { $0["claims"].int }
        radius = claims.map { nodeRadius(claims: $0) }
        var es: [(a: Int, b: Int, w: CGFloat, topic: Bool)] = []
        var adj = Array(repeating: [(n: Int, w: Int)](), count: ns.count)
        let kinds = self.kinds
        for e in r["edges"].arr {
            guard let a = index[e["a"].str], let b = index[e["b"].str] else { continue }
            let w = e["w"].int
            es.append((a, b, CGFloat(w), kinds[a] == "topic" || kinds[b] == "topic"))
            adj[a].append((b, w)); adj[b].append((a, w))
        }
        edges = es
        self.adj = adj.map { $0.sorted { $0.w > $1.w } }
        signature = "\(ns.count)-\(es.count)-\(claims.reduce(0, +))"
        pos = GraphData.layout(isTopic: kinds.map { $0 == "topic" }, edges: es)
    }

    private static func layout(isTopic: [Bool], edges: [(a: Int, b: Int, w: CGFloat, topic: Bool)]) -> [CGPoint] {
        let SX = GW / 880, SY = GH / 700
        let n = isTopic.count
        var px = [CGFloat](repeating: 0, count: n), py = px
        let main = (0..<n).filter { !isTopic[$0] }, topics = (0..<n).filter { isTopic[$0] }
        for (i, id) in main.enumerated() {
            let a = Double(i) / Double(main.count) * .pi * 2
            px[id] = GW / 2 + cos(a) * 200 * SX; py[id] = GH / 2 + sin(a) * 170 * SY
        }
        for (i, id) in topics.enumerated() {
            let a = Double(i) / Double(topics.count) * .pi * 2 - .pi / 2
            px[id] = GW / 2 + cos(a) * 390 * SX; py[id] = GH / 2 + 30 + sin(a) * 285 * SY
        }
        let springs = edges.filter { !$0.topic }.map { ($0.a, $0.b, min(1 + log($0.w), 3)) }
        let rep = 17000 * SX * SX, rest = 165 * SX
        var fx = [CGFloat](repeating: 0, count: n), fy = fx
        for _ in 0..<360 {
            for i in 0..<n { fx[i] = 0; fy[i] = 0 }
            for i in 0..<n {
                for j in (i + 1)..<n {
                    let dx = px[i] - px[j], dy = py[i] - py[j]
                    let d2 = max(dx * dx + dy * dy, 60), d = d2.squareRoot(), k = rep / d2 / d
                    fx[i] += dx * k; fy[i] += dy * k; fx[j] -= dx * k; fy[j] -= dy * k
                }
            }
            for (a, b, s) in springs {
                let dx = px[b] - px[a], dy = py[b] - py[a], d = max((dx * dx + dy * dy).squareRoot(), 1)
                let k = (d - rest) * 0.008 * s / d
                fx[a] += dx * k; fy[a] += dy * k; fx[b] -= dx * k; fy[b] -= dy * k
            }
            for i in main {
                let gx = fx[i] + (GW / 2 - px[i]) * 0.0035, gy = fy[i] + (GH / 2 - py[i]) * 0.007
                px[i] = min(GW - 70, max(70, px[i] + max(-10, min(10, gx))))
                py[i] = min(GH - 50, max(40, py[i] + max(-10, min(10, gy))))
            }
        }
        return (0..<n).map { CGPoint(x: px[$0], y: py[$0]) }
    }

    static func load() async -> GraphData? {
        guard let r = try? await API.get("/api/kg/graph") else { return shared }
        let fresh = await Task.detached { GraphData.quickSignature(r) }.value
        if let s = shared, s.signature == fresh { return s }
        let d = await Task.detached { GraphData(r) }.value
        shared = d
        return d
    }

    private static func quickSignature(_ r: J) -> String {
        let ns = r["nodes"].arr.filter { $0["claims"].int > 0 }
        let ids = Set(ns.map(\.id))
        let ec = r["edges"].arr.filter { ids.contains($0["a"].str) && ids.contains($0["b"].str) }.count
        return "\(ns.count)-\(ec)-\(ns.map { $0["claims"].int }.reduce(0, +))"
    }
}

struct MentionsGraph: View {
    let tabs: AnyView
    @EnvironmentObject var store: Store
    @State private var data: GraphData? = GraphData.shared
    @State private var sel = ""
    @State private var k: CGFloat = 1
    @State private var pan: CGSize = .zero
    @State private var panelOpen = true

    var body: some View {
        GeometryReader { geo in
            let canvas = CGSize(width: geo.size.width - (panelOpen ? 400 : 0), height: geo.size.height - 110)
            ZStack(alignment: .topLeading) {
                if let data {
                    GraphCanvas(data: data, sel: $sel, k: $k, pan: $pan, size: canvas)
                        .offset(y: 110)
                }

                header
                    .padding(.leading, 32).padding(.top, 28)

                zoomButtons
                    .padding(.leading, 20).padding(.bottom, 20)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)

                if panelOpen {
                    ClaimsPanel(sel: $sel, data: data, close: { panelOpen = false })
                        .frame(width: 368)
                        .padding(16)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                } else {
                    Button { panelOpen = true } label: { Image(systemName: "sidebar.right") }
                        .buttonStyle(BtnStyle()).help("Show panel")
                        .padding(20).frame(maxWidth: .infinity, alignment: .topTrailing)
                }
            }
        }
        .background(
            ZStack {
                Color.bg
                RadialGradient(colors: [Color.moving.opacity(0.08), .clear], center: UnitPoint(x: 0.62, y: 0.2), startRadius: 0, endRadius: 600)
                RadialGradient(colors: [Color.accent.opacity(0.06), .clear], center: UnitPoint(x: 0.2, y: 0.8), startRadius: 0, endRadius: 600)
            }
        )
        .task(id: store.tick) {
            if let d = await GraphData.load(), d !== data { data = d }
        }
        .background(Button("Deselect") { sel = "" }.keyboardShortcut(.escape, modifiers: []).opacity(0))
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Things mentioned in the work and what was said about them").font(.system(size: 13)).foregroundStyle(Color.mut).allowsHitTesting(false)
            HStack(spacing: 16) { Text("Graph").font(.system(size: 28, weight: .semibold)).tracking(-0.4).allowsHitTesting(false); tabs }
            HStack(spacing: 12) {
                ForEach(["project", "service", "tool", "model", "site", "topic"], id: \.self) { kd in
                    HStack(spacing: 5) { Circle().fill(KIND_COLOR[kd]!).frame(width: 7, height: 7); Text(kd) }
                }
                Text("Size is claims; a line means claims name both").foregroundStyle(Color.mut.opacity(0.8))
            }
            .font(.system(size: 11.5)).foregroundStyle(Color.mut)
            .allowsHitTesting(false)
        }
    }

    private var zoomButtons: some View {
        VStack(spacing: 6) {
            ForEach([("plus", "Zoom in"), ("minus", "Zoom out"), ("arrow.up.left.and.arrow.down.right", "Reset view")], id: \.0) { icon, label in
                Button {
                    withAnimation(.easeOut(duration: 0.2)) {
                        switch icon {
                        case "plus": k = min(3, k * 1.25)
                        case "minus": k = max(0.5, k / 1.25)
                        default: k = 1; pan = .zero
                        }
                    }
                } label: { Image(systemName: icon).font(.system(size: 12, weight: .semibold)).frame(width: 14) }
                .buttonStyle(BtnStyle()).help(label)
            }
        }
    }
}

private struct GraphCanvas: View {
    let data: GraphData
    @Binding var sel: String
    @Binding var k: CGFloat
    @Binding var pan: CGSize
    let size: CGSize
    @State private var hover = -1
    @State private var dragStart: CGSize?
    @State private var pinchStart: CGFloat?

    var body: some View {
        let scale = min(size.width / GW, size.height / GH) * k
        let selI = data.index[sel] ?? -1
        let focus = hover >= 0 ? hover : selI
        let nbr = focus >= 0 ? Set(data.adj[focus].map(\.n)) : []
        Canvas { ctx, sz in
            ctx.translateBy(x: sz.width / 2 + pan.width, y: sz.height / 2 + pan.height)
            ctx.scaleBy(x: scale, y: scale)
            ctx.translateBy(x: -GW / 2, y: -GH / 2)
            let dimLine = Color(hex: 0xa1a1aa).opacity(focus >= 0 ? 0.05 : 0.16)
            var lit = Path()
            for e in data.edges {
                let on = focus >= 0 && (e.a == focus || e.b == focus)
                if !on && e.topic { continue }
                var p = Path(); p.move(to: data.pos[e.a]); p.addLine(to: data.pos[e.b])
                if on { lit.addPath(p) } else { ctx.stroke(p, with: .color(dimLine), lineWidth: 0.8 + log(1 + e.w) * 0.8) }
            }
            ctx.stroke(lit, with: .color(Color.accent.opacity(0.85)), lineWidth: 2.2)
            for i in data.ids.indices {
                let p = data.pos[i], r = data.radius[i], c = data.colors[i]
                let on = i == selI, hot = i == hover
                var g = ctx
                g.opacity = focus < 0 || i == focus || nbr.contains(i) ? 1 : 0.22
                g.fill(Path(ellipseIn: CGRect(x: p.x - r - 12, y: p.y - r - 12, width: (r + 12) * 2, height: (r + 12) * 2)),
                       with: .color(c.opacity(on ? 0.28 : hot ? 0.2 : 0.1)))
                let body = Path(ellipseIn: CGRect(x: p.x - r, y: p.y - r, width: r * 2, height: r * 2))
                g.fill(body, with: .radialGradient(Gradient(colors: [.white.opacity(0.55), c, c.opacity(0.75)]),
                                                   center: CGPoint(x: p.x - r * 0.3, y: p.y - r * 0.4), startRadius: 0, endRadius: r * 1.2))
                g.stroke(body, with: .color(on ? .white : .white.opacity(hot ? 0.6 : 0.25)), lineWidth: on ? 2.5 : 1)
                g.draw(Text(data.names[i]).font(.system(size: on ? 21 : 18, weight: on || hot ? .semibold : .medium))
                    .foregroundColor(on || hot ? .white : Color(hex: 0xd4d4d8)),
                       at: CGPoint(x: p.x, y: p.y + r + 14), anchor: .top)
                g.draw(Text("\(data.claims[i]) claims").font(.system(size: 14)).foregroundColor(.mut),
                       at: CGPoint(x: p.x, y: p.y + r + 38), anchor: .top)
            }
        }
        .frame(width: size.width, height: size.height)
        .clipped()
        .contentShape(Rectangle())
        .gesture(DragGesture(minimumDistance: 2)
            .onChanged { v in
                if dragStart == nil { dragStart = pan }
                pan = CGSize(width: dragStart!.width + v.translation.width, height: dragStart!.height + v.translation.height)
            }
            .onEnded { _ in dragStart = nil })
        .simultaneousGesture(MagnifyGesture()
            .onChanged { v in
                if pinchStart == nil { pinchStart = k }
                k = min(3, max(0.5, pinchStart! * v.magnification))
            }
            .onEnded { _ in pinchStart = nil })
        .onTapGesture(coordinateSpace: .local) { loc in
            let i = nodeAt(loc, scale: scale)
            sel = i < 0 || data.ids[i] == sel ? "" : data.ids[i]
        }
        .onContinuousHover(coordinateSpace: .local) { phase in
            switch phase {
            case .active(let loc):
                let i = nodeAt(loc, scale: scale)
                if i != hover { hover = i }
            case .ended: hover = -1
            }
        }
    }

    private func nodeAt(_ loc: CGPoint, scale: CGFloat) -> Int {
        let gx = (loc.x - pan.width - size.width / 2) / scale + GW / 2
        let gy = (loc.y - pan.height - size.height / 2) / scale + GH / 2
        var best = -1, bestD = CGFloat.infinity
        for i in data.pos.indices {
            let d = hypot(data.pos[i].x - gx, data.pos[i].y - gy)
            if d <= data.radius[i] + 8 + 6 / scale, d < bestD { best = i; bestD = d }
        }
        return best
    }
}

private struct ClaimsPanel: View {
    @EnvironmentObject var store: Store
    @Environment(\.openURL) private var openURL
    @Binding var sel: String
    let data: GraphData?
    let close: () -> Void
    private var nodes: [J] { data?.nodes ?? [] }
    private var shownTab: String { !sel.isEmpty && tab == "topics" ? "claims" : tab }
    @State private var tab = "claims"
    @State private var q = ""
    @State private var status = ""
    @State private var speaker = ""
    @State private var det: J?
    @State private var lineage: [J] = []
    @State private var shown = 8
    @State private var sug: J?

    var body: some View {
        let cur = nodes.first { $0.id == sel }
        let toReview = (sug?["entities"].arr.count ?? 0) + (sug?["aliases"].arr.count ?? 0)
        let byId = Dictionary(uniqueKeysWithValues: nodes.map { ($0.id, $0) })
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 10) {
                if let cur {
                    HStack(alignment: .top) {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(cur["name"].str).font(.system(size: 19, weight: .semibold)).lineLimit(1)
                            HStack(spacing: 6) {
                                Circle().fill(KIND_COLOR[cur["kind"].str] ?? .mut).frame(width: 7, height: 7)
                                Text(cur["kind"].str)
                                Text("\(cur["claims"].int) claims")
                            }
                            .font(.system(size: 12)).foregroundStyle(Color.mut)
                        }
                        Spacer()
                        Button { sel = "" } label: { Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)) }
                            .buttonStyle(BtnStyle(kind: .ghost)).help("Deselect (Esc)")
                    }
                    let nbr = neighbours(cur.id)
                    if !nbr.isEmpty {
                        FlowLayout(spacing: 5) {
                            ForEach(nbr.prefix(10), id: \.0) { id, w in
                                Button { sel = id } label: {
                                    HStack(spacing: 4) {
                                        Circle().fill(KIND_COLOR[byId[id]?["kind"].str ?? ""] ?? .mut).frame(width: 6, height: 6)
                                        Text(shortName(byId[id]?["name"].str ?? id))
                                        Text("\(w)").foregroundStyle(Color.mut)
                                    }
                                    .font(.system(size: 11.5)).foregroundStyle(Color.soft)
                                    .padding(.horizontal, 8).frame(height: 24)
                                    .background(Color.white.opacity(0.04), in: Capsule())
                                    .overlay(Capsule().strokeBorder(Color.line2, lineWidth: 1))
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                HStack(spacing: 6) {
                    HStack(spacing: 4) {
                        tabButton("claims", "Claims", det.map { "\($0["total"].int)" })
                        tabButton("lineage", "Lineage", lineageFor(cur).isEmpty ? nil : "\(lineageFor(cur).count)")
                        if cur == nil { tabButton("topics", "Topics", toReview > 0 ? "\(toReview)" : nil, badge: .open) }
                    }
                    .padding(4).background(Color.white.opacity(0.05), in: RoundedRectangle(cornerRadius: 11, style: .continuous))
                    Button(action: close) { Image(systemName: "sidebar.right") }.buttonStyle(BtnStyle(kind: .ghost)).help("Hide panel")
                }
            }
            .padding(.horizontal, 16).padding(.top, 14).padding(.bottom, 12)
            Divider().overlay(Color.line)

            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    switch shownTab {
                    case "topics": TopicsPanel(sug: sug) { store.changed() }
                    case "lineage": lineageList(cur)
                    default: claims(cur: cur)
                    }
                }
                .padding(.horizontal, 16).padding(.vertical, 12)
            }
        }
        .background(Color(hex: 0x141417).opacity(0.96), in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
        .shadow(color: .black.opacity(0.5), radius: 30, y: 24)
        .task(id: "\(sel)|\(q)|\(status)|\(speaker)") {
            shown = 8
            try? await Task.sleep(nanoseconds: q.isEmpty ? 0 : 250_000_000)
            det = try? await API.get("/api/kg/claims?entity=\(API.q(sel))&q=\(API.q(q))&status=\(status)&speaker=\(speaker)")
        }
        .task { if let l = try? await API.get("/api/kg/lineage") { lineage = l.arr } }
        .task(id: store.tick) { sug = try? await API.get("/api/kg/suggest") }
    }

    private func neighbours(_ id: String) -> [(String, Int)] {
        guard let data, let i = data.index[id] else { return [] }
        return data.adj[i].map { (data.ids[$0.n], $0.w) }
    }

    private func lineageFor(_ cur: J?) -> [J] {
        guard let cur else { return lineage }
        return lineage.filter { $0["path"].str.components(separatedBy: " → ").contains(cur["name"].str) }
    }

    private func pick(name: String) {
        if let n = nodes.first(where: { $0["name"].str == name }) { sel = n.id; tab = "claims" }
    }

    private func tabButton(_ id: String, _ label: String, _ n: String?, badge: Color? = nil) -> some View {
        Button { tab = id } label: {
            HStack(spacing: 5) {
                Text(label)
                if let n {
                    Text(n).font(.system(size: 11)).monospacedDigit().foregroundStyle(badge ?? Color.mut)
                        .padding(.horizontal, badge == nil ? 0 : 6)
                        .background(badge.map { $0.opacity(0.15) } ?? .clear, in: Capsule())
                }
            }
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(shownTab == id ? .white : Color.soft)
            .frame(maxWidth: .infinity).frame(height: 30)
            .background(shownTab == id ? Color(hex: 0x2a2a30) : .clear, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func filterChip(_ v: String, _ cur: Binding<String>, _ label: String, _ dot: Color? = nil) -> some View {
        let on = cur.wrappedValue == v
        return Button { cur.wrappedValue = on ? "" : v } label: {
            HStack(spacing: 5) {
                if let dot { Circle().fill(dot).frame(width: 6, height: 6) }
                Text(label)
            }
            .font(.system(size: 12, weight: .medium))
            .foregroundStyle(on ? Color.accent : Color.soft)
            .padding(.horizontal, 9).frame(height: 26)
            .background(on ? Color.accent.opacity(0.15) : Color.white.opacity(0.04), in: Capsule())
            .overlay(Capsule().strokeBorder(Color.line2, lineWidth: 1))
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder private func claims(cur: J?) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").font(.system(size: 12)).foregroundStyle(Color.mut)
            TextField("Search the claims", text: $q).textFieldStyle(.plain).font(.system(size: 13))
        }
        .padding(.horizontal, 10).frame(height: 34)
        .background(Color.black.opacity(0.35), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
        FlowLayout(spacing: 5) {
            filterChip("active", $status, "Kept", .accent)
            filterChip("proposed", $status, "Suggested", .mut)
            filterChip("superseded", $status, "Replaced", .open)
            filterChip("owner", $speaker, "You")
            filterChip("assistant", $speaker, "Claude")
        }
        .padding(.bottom, 4)
        let list = det?["claims"].arr ?? []
        let ev = det?["evidence"].arr ?? []
        if det != nil && list.isEmpty { Text("Nothing matches.").font(.system(size: 13)).foregroundStyle(Color.mut) }
        ForEach(list.prefix(shown)) { c in
            ClaimCard(c: c, ev: ev.filter { $0["claim_id"].str == c.id }, showEntities: cur == nil, kinds: Dictionary(uniqueKeysWithValues: nodes.map { ($0["name"].str, $0["kind"].str) }), pick: pick)
        }
        if list.count > shown {
            Button("Show \(min(8, list.count - shown)) more") { shown += 8 }.buttonStyle(BtnStyle(kind: .ghost))
        }
    }

    @ViewBuilder private func lineageList(_ cur: J?) -> some View {
        let ls = lineageFor(cur)
        if ls.isEmpty { Text("No lineage recorded\(cur == nil ? "" : " for this one").").font(.system(size: 13)).foregroundStyle(Color.mut) }
        ForEach(ls.prefix(40)) { l in
            VStack(alignment: .leading, spacing: 6) {
                FlowLayout(spacing: 5) {
                    ForEach(Array(l["path"].str.components(separatedBy: " → ").enumerated()), id: \.offset) { i, n in
                        HStack(spacing: 5) {
                            if i > 0 { Text("→").foregroundStyle(Color.mut) }
                            Button { pick(name: n) } label: {
                                Text(shortName(n)).padding(.horizontal, 8).frame(height: 22)
                                    .overlay(Capsule().strokeBorder(Color.line2, lineWidth: 1))
                            }
                            .buttonStyle(.plain)
                        }
                        .font(.system(size: 12.5))
                    }
                }
                HStack(spacing: 8) {
                    Chip(text: l["type"].str.replacingOccurrences(of: "relation:", with: ""), color: .soft)
                    Text(dayFmt(l["asserted_at"].s))
                }
                .font(.system(size: 11.5)).foregroundStyle(Color.mut)
                if let quote = l["quote"].s {
                    Text("“\(quote)”").font(.system(size: 11.5)).italic().foregroundStyle(Color.soft).lineLimit(3)
                }
            }
            .padding(12).frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.white.opacity(0.025), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.line, lineWidth: 1))
        }
    }
}

private struct ClaimCard: View {
    @EnvironmentObject var store: Store
    let c: J
    let ev: [J]
    let showEntities: Bool
    let kinds: [String: String]
    let pick: (String) -> Void
    @State private var quote = false

    var body: some View {
        let st = CLAIM_STATUS[c["status"].str] ?? (c["status"].str, Color.mut)
        let names = showEntities ? c["entities"].str.components(separatedBy: " | ").filter { !$0.isEmpty } : []
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 7) {
                Text(dayFmt(c["asserted_at"].s)).monospacedDigit()
                HStack(spacing: 3) {
                    Image(systemName: c["speaker"].str == "owner" ? "person.fill" : "sparkles").font(.system(size: 9))
                    Text(c["speaker"].str == "owner" ? "You" : c["speaker"].str == "assistant" ? "Claude" : "Unknown")
                }
                .foregroundStyle(Color.soft).padding(.horizontal, 6).padding(.vertical, 1).background(Color.white.opacity(0.06), in: Capsule())
                Spacer()
                Circle().fill(st.1).frame(width: 6, height: 6)
                Text(st.0).foregroundStyle(st.1)
            }
            .font(.system(size: 11.5)).foregroundStyle(Color.mut)
            Text(c["text"].str).font(.system(size: 14)).foregroundStyle(c["status"].str == "superseded" ? Color.soft : Color.ink)
                .lineLimit(3).fixedSize(horizontal: false, vertical: true)
            if !names.isEmpty {
                FlowLayout(spacing: 4) {
                    ForEach(names.prefix(4), id: \.self) { n in
                        Button { pick(n) } label: {
                            HStack(spacing: 4) {
                                Circle().fill(KIND_COLOR[kinds[n] ?? ""] ?? .mut).frame(width: 6, height: 6)
                                Text(shortName(n))
                            }
                            .font(.system(size: 11.5)).foregroundStyle(Color.soft)
                            .padding(.horizontal, 8).frame(height: 22)
                            .overlay(Capsule().strokeBorder(Color.line2, lineWidth: 1))
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            if !ev.isEmpty {
                Button { withAnimation(.easeOut(duration: 0.15)) { quote.toggle() } } label: {
                    HStack(spacing: 4) {
                        Image(systemName: "chevron.right").font(.system(size: 8, weight: .bold)).rotationEffect(.degrees(quote ? 90 : 0))
                        Text("Quote")
                    }
                    .font(.system(size: 12)).foregroundStyle(Color.mut)
                }
                .buttonStyle(.plain)
                if quote {
                    ForEach(Array(ev.prefix(2).enumerated()), id: \.offset) { _, v in
                        VStack(alignment: .leading, spacing: 3) {
                            Text("“\(v["quote"].str)”").font(.system(size: 12)).foregroundStyle(Color.soft).fixedSize(horizontal: false, vertical: true)
                            Button { store.go(.source(v["source_id"].int, chunk: nil)) } label: {
                                Text((v["stated"].str == "inferred" ? "inferred, " : "") + (v["source_title"].s ?? v["source"].str))
                                    .font(.system(size: 11)).foregroundStyle(Color.mut).underline()
                            }
                            .buttonStyle(.plain)
                        }
                        .padding(.leading, 10).overlay(alignment: .leading) { Rectangle().fill(Color.line2).frame(width: 2) }
                    }
                }
            }
        }
        .padding(.vertical, 10).padding(.leading, 14).padding(.trailing, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.white.opacity(0.025), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.line, lineWidth: 1))
        .overlay(alignment: .leading) {
            UnevenRoundedRectangle(topLeadingRadius: 14, bottomLeadingRadius: 14).fill(st.1).frame(width: 3)
        }
    }
}
