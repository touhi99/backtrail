import SwiftUI

private enum MapSel: Equatable {
    case note(Int)
    case item(String)
}

private struct MapNode: Identifiable {
    enum Kind { case center, note, item, linked }
    let id: String
    let kind: Kind
    let pos: CGPoint
    let size: CGSize
    let data: J
}

private struct MapEdge {
    let a: String
    let b: String
    let color: Color
    let width: CGFloat
    var dash: [CGFloat] = []
}


struct MapView: View {
    @EnvironmentObject var store: Store
    let thread: String
    @State private var f: J?
    @State private var sel: MapSel?
    @State private var allItems = false
    @State private var k: CGFloat = 1

    var body: some View {
        ZStack(alignment: .topLeading) {
            if let f, f["thread"]["name"].str == thread {
                let (nodes, edges) = layout(f)
                canvas(nodes, edges)
                if let sel { panel(f, sel).frame(width: 340).padding(12).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing) }
                let notes = f["notes"].arr.count
                if !allItems && notes > 8 {
                    Text("\(notes - 8) more notes hidden").font(.system(size: 12)).foregroundStyle(Color.mut)
                        .padding(.horizontal, 16).padding(.vertical, 8)
                        .background(Color(hex: 0x141416).opacity(0.9), in: Capsule())
                        .overlay(Capsule().strokeBorder(Color.line2, lineWidth: 1))
                        .padding(16).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                }
            } else {
                Text("Loading the map").foregroundStyle(Color.mut).frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            toolbar.padding(16)
            HStack(spacing: 6) {
                Button { k = min(1.6, k * 1.25) } label: { Image(systemName: "plus") }.buttonStyle(BtnStyle()).help("Zoom in")
                Button { k = max(0.3, k / 1.25) } label: { Image(systemName: "minus") }.buttonStyle(BtnStyle()).help("Zoom out")
            }
            .padding(16).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)
        }
        .background(Color.bg)
        .task(id: store.tick) { f = try? await API.get("/api/focus?thread=\(API.q(thread))") }
        .background(Button("Deselect") { sel = nil }.keyboardShortcut(.escape, modifiers: []).opacity(0))
    }

    private var toolbar: some View {
        HStack(spacing: 8) {
            Button { store.go(.graph) } label: { Label("All threads", systemImage: "arrow.left") }.buttonStyle(BtnStyle())
            Button("Open page") { store.go(.thread(thread)) }.buttonStyle(BtnStyle())
            Button { store.capture = .some(thread) } label: { Label("Add to \(thread)", systemImage: "plus") }.buttonStyle(BtnStyle())
            if let f, f["items"].arr.count > 7 || f["notes"].arr.count > 8 {
                Button(allItems ? "Show fewer" : "Show all \(f["items"].arr.count) items and \(f["notes"].arr.count + f["hidden_notes"].int) notes") { allItems.toggle() }
                    .buttonStyle(BtnStyle())
            }
        }
    }

    private func layout(_ F: J) -> ([MapNode], [MapEdge]) {
        let linked = F["linked"].arr.sorted { a, b in
            let ba = a["kind"].str == "built on" ? 1 : 0, bb = b["kind"].str == "built on" ? 1 : 0
            return ba != bb ? ba > bb : a["w"].int > b["w"].int
        }.prefix(5)
        let notes = allItems ? F["notes"].arr : Array(F["notes"].arr.prefix(8))
        let noteIds = Set(notes.map { $0["id"].int })
        let order = ["question": 0, "step": 1, "decision": 2, "turn": 3]
        let items = F["items"].arr.sorted { a, b in
            let oa = a["state"].str == "open" ? 0 : 1, ob = b["state"].str == "open" ? 0 : 1
            return oa != ob ? oa < ob : (order[a["type"].str] ?? 4) < (order[b["type"].str] ?? 4)
        }
        let shownItems = allItems ? items : Array(items.prefix(7))
        var nodes: [MapNode] = [MapNode(id: "center", kind: .center, pos: CGPoint(x: -160, y: -70), size: CGSize(width: 320, height: 210), data: F)]
        let iy0 = -CGFloat(shownItems.count * 84) / 2
        for (i, it) in shownItems.enumerated() {
            nodes.append(MapNode(id: "I:\(it.id)", kind: .item, pos: CGPoint(x: -560, y: iy0 + CGFloat(i * 84)), size: CGSize(width: 250, height: 70), data: it))
        }
        let rows = (notes.count + 1) / 2
        let ny0 = -CGFloat(rows * 168) / 2
        for (i, n) in notes.enumerated() {
            nodes.append(MapNode(id: "N:\(n["id"].int)", kind: .note, pos: CGPoint(x: 300 + CGFloat(i % 2) * 330, y: ny0 + CGFloat(i / 2 * 168)), size: CGSize(width: 300, height: 140), data: n))
        }
        var edges: [MapEdge] = []
        for (i, l) in linked.enumerated() {
            let x = -160 + (CGFloat(i) - CGFloat(linked.count - 1) / 2) * 215 + 60
            let id = "L:\(l["name"].str):\(l["kind"].str)"
            nodes.append(MapNode(id: id, kind: .linked, pos: CGPoint(x: x, y: -max(CGFloat(rows * 168), 300) / 2 - 90), size: CGSize(width: 210, height: 36), data: l))
            let built = l["kind"].str == "built on"
            edges.append(MapEdge(a: "center", b: id, color: built ? .moving : Color(hex: 0x3a3a42), width: built ? 2 : 1.2))
        }
        for it in shownItems { edges.append(MapEdge(a: "I:\(it.id)", b: "center", color: Color(hex: 0x2c2c32), width: 1.2)) }
        for n in notes { edges.append(MapEdge(a: "center", b: "N:\(n["id"].int)", color: Color(hex: 0x2c2c32), width: 1.2)) }
        if case .note(let id) = sel {
            for e in F["note_edges"].arr where noteIds.contains(e["a"].int) && noteIds.contains(e["b"].int) && (e["a"].int == id || e["b"].int == id) {
                edges.append(MapEdge(a: "N:\(e["a"].int)", b: "N:\(e["b"].int)", color: .accent, width: 1.6, dash: [5, 4]))
            }
        }
        if sel != nil {
            for it in shownItems where noteIds.contains(it["source_id"].int) {
                edges.append(MapEdge(a: "I:\(it.id)", b: "N:\(it["source_id"].int)", color: .accent, width: 1.4))
            }
        }
        return (nodes, edges)
    }

    private func related(_ F: J) -> Set<String>? {
        guard let sel else { return nil }
        var rel: Set<String> = ["center"]
        switch sel {
        case .note(let id):
            rel.insert("N:\(id)")
            for i in F["items"].arr where i["source_id"].int == id { rel.insert("I:\(i.id)") }
            for e in F["note_edges"].arr {
                if e["a"].int == id { rel.insert("N:\(e["b"].int)") }
                if e["b"].int == id { rel.insert("N:\(e["a"].int)") }
            }
        case .item(let id):
            rel.insert("I:\(id)")
            if let it = F["items"].arr.first(where: { $0.id == id }), it["source_id"].int > 0 { rel.insert("N:\(it["source_id"].int)") }
        }
        return rel
    }

    private func canvas(_ nodes: [MapNode], _ edges: [MapEdge]) -> some View {
        let minX = nodes.map(\.pos.x).min() ?? 0, minY = nodes.map(\.pos.y).min() ?? 0
        let maxX = nodes.map { $0.pos.x + $0.size.width }.max() ?? 0, maxY = nodes.map { $0.pos.y + $0.size.height }.max() ?? 0
        let pad: CGFloat = 80
        let origin = CGPoint(x: minX - pad, y: minY - pad - 40)
        let full = CGSize(width: maxX - minX + pad * 2, height: maxY - minY + pad * 2 + 40)
        let byId = Dictionary(uniqueKeysWithValues: nodes.map { ($0.id, $0) })
        let rel = f.flatMap(related)
        let selId: String? = sel.map { if case .note(let n) = $0 { "N:\(n)" } else if case .item(let i) = $0 { "I:\(i)" } else { nil } } ?? nil
        func center(_ n: MapNode) -> CGPoint { CGPoint(x: n.pos.x - origin.x + n.size.width / 2, y: n.pos.y - origin.y + n.size.height / 2) }
        return GeometryReader { g in
            ScrollView([.horizontal, .vertical]) {
                ZStack(alignment: .topLeading) {
                    Canvas { ctx, _ in
                        for e in edges {
                            guard let a = byId[e.a], let b = byId[e.b] else { continue }
                            var p = Path()
                            let pa = center(a), pb = center(b)
                            p.move(to: pa)
                            p.addCurve(to: pb, control1: CGPoint(x: (pa.x + pb.x) / 2, y: pa.y), control2: CGPoint(x: (pa.x + pb.x) / 2, y: pb.y))
                            ctx.stroke(p, with: .color(e.color), style: StrokeStyle(lineWidth: e.width, dash: e.dash))
                        }
                    }
                    .frame(width: full.width, height: full.height)
                    ForEach(nodes) { n in
                        let dim = rel.map { !$0.contains(n.id) && n.kind != .center && n.kind != .linked } ?? false
                        nodeView(n, selected: n.id == selId)
                            .frame(width: n.size.width, alignment: .topLeading)
                            .opacity(dim ? 0.3 : 1)
                            .offset(x: n.pos.x - origin.x, y: n.pos.y - origin.y)
                            .onTapGesture { tap(n) }
                    }
                }
                .frame(width: full.width, height: full.height, alignment: .topLeading)
                .contentShape(Rectangle())
                .onTapGesture { sel = nil }
                .scaleEffect(k, anchor: .topLeading)
                .frame(width: full.width * k, height: full.height * k, alignment: .topLeading)
                .frame(width: max(full.width * k, g.size.width), height: max(full.height * k, g.size.height), alignment: .center)
            }
            .scrollIndicators(.never)
            .onAppear { k = min(1, max(0.3, min(g.size.width / full.width, g.size.height / full.height))) }
        }
    }

    private func tap(_ n: MapNode) {
        switch n.kind {
        case .linked: store.go(.map(n.data["name"].str))
        case .note: sel = .note(n.data["id"].int)
        case .item: sel = .item(n.data.id)
        case .center: break
        }
    }

    @ViewBuilder private func nodeView(_ n: MapNode, selected: Bool) -> some View {
        let border = selected ? Color.accent : Color.line
        switch n.kind {
        case .center: centerCard(n.data)
        case .note:
            let x = n.data
            let art = x["kind"].str == "artifact"
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 10) {
                    Image(systemName: art ? "sparkles" : "doc.text").font(.system(size: 14)).foregroundStyle(art ? Color.purple : Color(hex: 0xa1a1aa))
                        .frame(width: 32, height: 32).background((art ? Color.purple : Color.mut).opacity(0.12), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                    Text(x["title"].str).font(.system(size: 13.5, weight: .semibold)).lineLimit(1)
                }
                Text(x["gist"].s.flatMap { $0.isEmpty ? nil : $0 } ?? "No summary line yet.").font(.system(size: 12.5)).foregroundStyle(Color.soft)
                    .lineLimit(3).frame(minHeight: 48, alignment: .topLeading)
                HStack(spacing: 6) {
                    let o = x["outline"].arr.count
                    Text(o > 0 ? "\(o) sections" : "one section")
                    if x["cited"].int > 0 {
                        Text("\(x["cited"].int) \(x["cited"].int == 1 ? "item" : "items") cite it").foregroundStyle(Color.accent)
                            .padding(.horizontal, 6).padding(.vertical, 2).background(Color.accent.opacity(0.15), in: Capsule())
                    }
                    if x["links"].int > 0 { Text("\(x["links"].int) linked notes") }
                }
                .font(.system(size: 11.5)).foregroundStyle(Color.mut)
            }
            .padding(14)
            .background(LinearGradient(colors: [Color(hex: 0x171719), Color(hex: 0x111113)], startPoint: .top, endPoint: .bottom), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(border, lineWidth: selected ? 1.5 : 1))
        case .item:
            let i = n.data
            let icon = i["tag"].str == "bug" ? "ladybug" : ["question": "questionmark.circle", "step": "checklist", "decision": "scale.3d"][i["type"].str] ?? "checklist"
            let col: Color = i["type"].str == "question" ? .open : i["state"].str == "done" ? .accent : i["tag"].str == "bug" ? .dropped : .mut
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: icon).font(.system(size: 12)).foregroundStyle(col)
                        .frame(width: 24, height: 24).background(col.opacity(0.16), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                    Text(i["title"].str).font(.system(size: 12.5)).lineLimit(2)
                        .foregroundStyle(i["state"].str == "done" ? Color.mut : Color.ink).strikethrough(i["state"].str == "done", color: .mut)
                }
                HStack(spacing: 6) {
                    Text(i["type"].str.capitalized)
                    if i["state"].str == "doing" { Text("doing").foregroundStyle(Color.moving) }
                    if i["state"].str == "looks_done" { Text("looks done?").foregroundStyle(Color.accent) }
                    if i["state"].str == "proposed" { Text("suggested").foregroundStyle(Color.moving) }
                }
                .font(.system(size: 11)).foregroundStyle(Color.mut)
            }
            .padding(.horizontal, 12).padding(.vertical, 10).frame(width: n.size.width, alignment: .leading)
            .background(LinearGradient(colors: [Color(hex: 0x171719), Color(hex: 0x111113)], startPoint: .top, endPoint: .bottom), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(border, lineWidth: selected ? 1.5 : 1))
        case .linked:
            let l = n.data
            HStack(spacing: 8) {
                Circle().fill(STATE_COLOR[l["state"].str] ?? Color(hex: 0x52525b)).frame(width: 8, height: 8)
                Text(l["name"].str).font(.system(size: 12.5, weight: .medium)).lineLimit(1)
                Spacer(minLength: 0)
                Text(l["kind"].str == "built on" ? (l["dir"].str == "out" ? "built on" : "builds on it") : "\(l["w"].int) mentions")
                    .font(.system(size: 11)).foregroundStyle(Color.mut)
            }
            .padding(.horizontal, 14).frame(width: n.size.width, height: 36)
            .background(Color(hex: 0x151517), in: Capsule())
            .overlay(Capsule().strokeBorder(Color.line2, lineWidth: 1))
        }
    }

    private func centerCard(_ F: J) -> some View {
        let t = F["thread"]
        let c = STATE_COLOR[t["state"].str] ?? Color(hex: 0x52525b)
        let cnt = { (k: String) in F["counts"].arr.first { $0["kind"].str == k }?["n"].int ?? 0 }
        return VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 12) {
                Glyph(name: t["name"].str, color: c, size: 44)
                HStack(spacing: 8) {
                    Circle().fill(c).frame(width: 10, height: 10)
                    Text(t["state"].str.capitalized)
                    if let k = t["kind"].s, !k.isEmpty { Chip(text: k, color: .mut) }
                }
                .font(.system(size: 12)).foregroundStyle(Color.mut)
            }
            Text(t["name"].str).font(.system(size: 24, weight: .semibold)).tracking(-0.4)
            Text(t["summary"].str).font(.system(size: 13)).foregroundStyle(Color.soft).fixedSize(horizontal: false, vertical: true)
            FlowLayout(spacing: 6) {
                Chip(text: "\(F["notes"].arr.count + F["hidden_notes"].int) notes and artifacts", color: .mut)
                Chip(text: "\(cnt("session")) conversations", color: .mut)
                if cnt("commit") > 0 { Chip(text: "\(cnt("commit")) commit lines", color: .mut) }
            }
        }
        .padding(20)
        .background(
            ZStack(alignment: .topTrailing) {
                LinearGradient(colors: [Color(hex: 0x1c1c20), Color(hex: 0x121214)], startPoint: .top, endPoint: .bottom)
                RadialGradient(colors: [c.opacity(0.25), .clear], center: .topTrailing, startRadius: 0, endRadius: 160)
            }
            .clipShape(RoundedRectangle(cornerRadius: 26, style: .continuous))
        )
        .overlay(RoundedRectangle(cornerRadius: 26, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
        .shadow(color: .black.opacity(0.55), radius: 30, y: 24)
    }

    @ViewBuilder private func panel(_ F: J, _ s: MapSel) -> some View {
        switch s {
        case .note(let id):
            if let n = F["notes"].arr.first(where: { $0["id"].int == id }) { NotePanel(f: F, n: n, sel: $sel) }
        case .item(let id):
            if let it = F["items"].arr.first(where: { $0.id == id }) { ItemPanel(f: F, it: it, sel: $sel) }
        }
    }
}

private struct PanelShell<C: View>: View {
    let kind: String
    let title: String
    let close: () -> Void
    @ViewBuilder let content: C

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(kind).font(.system(size: 12)).foregroundStyle(Color.mut)
                    Text(title).font(.system(size: 16, weight: .semibold)).fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
                Button(action: close) { Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)) }.buttonStyle(BtnStyle(kind: .ghost))
            }
            .padding(16).padding(.bottom, -8)
            ScrollView { VStack(alignment: .leading, spacing: 4) { content }.padding(16) }
        }
        .fixedSize(horizontal: false, vertical: true)
        .background(Color(hex: 0x141416).opacity(0.94), in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
        .shadow(color: .black.opacity(0.55), radius: 30, y: 24)
    }
}

private func panelRow(_ label: String, icon: String? = nil, _ action: @escaping () -> Void) -> some View {
    Button(action: action) {
        HStack(spacing: 8) {
            if let icon { Image(systemName: icon).font(.system(size: 12)).foregroundStyle(Color.mut) }
            Text(label).font(.system(size: 13)).lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.vertical, 6).contentShape(Rectangle())
    }
    .buttonStyle(.plain)
}

private struct NotePanel: View {
    @EnvironmentObject var store: Store
    let f: J
    let n: J
    @Binding var sel: MapSel?

    var body: some View {
        let id = n["id"].int
        let cites = f["items"].arr.filter { $0["source_id"].int == id }
        var seen = Set<Int>()
        let linked = f["note_edges"].arr.filter { $0["a"].int == id || $0["b"].int == id }
            .compactMap { e in f["notes"].arr.first { $0["id"].int == (e["a"].int == id ? e["b"].int : e["a"].int) } }
            .filter { seen.insert($0["id"].int).inserted }
        return PanelShell(kind: n["kind"].str == "artifact" ? "Artifact" : "Note", title: n["title"].str, close: { sel = nil }) {
            Text(n["gist"].str).font(.system(size: 13)).foregroundStyle(Color.soft).fixedSize(horizontal: false, vertical: true)
            if !n["outline"].arr.isEmpty {
                Text("What is inside").font(.system(size: 12, weight: .medium)).foregroundStyle(Color.mut).padding(.top, 12)
                ForEach(n["outline"].arr, id: \.["chunk_id"].int) { o in
                    let h = o["heading"].str.trimmingCharacters(in: CharacterSet(charactersIn: "#§"))
                    panelRow(String(h.prefix(60)), icon: "arrow.up.right") { store.go(.source(id, chunk: o["chunk_id"].int)) }
                }
            }
            if !cites.isEmpty {
                Text("Items filed from it").font(.system(size: 12, weight: .medium)).foregroundStyle(Color.mut).padding(.top, 12)
                ForEach(cites) { i in panelRow(i["title"].str) { sel = .item(i.id) } }
            }
            if !linked.isEmpty {
                Text("Linked notes").font(.system(size: 12, weight: .medium)).foregroundStyle(Color.mut).padding(.top, 12)
                ForEach(linked, id: \.["id"].int) { x in panelRow(x["title"].str) { sel = .note(x["id"].int) } }
            }
            Button { store.go(.source(id, chunk: nil)) } label: { Text("Read the whole \(n["kind"].str)").frame(maxWidth: .infinity) }
                .buttonStyle(BtnStyle()).padding(.top, 12)
        }
    }
}

private struct ItemPanel: View {
    @EnvironmentObject var store: Store
    let f: J
    let it: J
    @Binding var sel: MapSel?
    @State private var ans = ""
    @State private var ask: ReasonAsk?

    var body: some View {
        let state = it["state"].str, type = it["type"].str
        let src = f["notes"].arr.first { $0["id"].int == it["source_id"].int }
        PanelShell(kind: (it["tag"].str == "bug" ? "Bug" : type.capitalized) + (state != "open" ? " · \(state.replacingOccurrences(of: "_", with: " "))" : ""),
                   title: it["title"].str, close: { sel = nil }) {
            if let b = it["body"].s, !b.isEmpty { Text(b).font(.system(size: 13)).foregroundStyle(Color.soft).fixedSize(horizontal: false, vertical: true) }
            if let a = it["answer"].s, !a.isEmpty { Text(a).font(.system(size: 13)).foregroundStyle(Color.accent).padding(.top, 4) }
            if let q = it["quote"].s, !q.isEmpty, q != it["title"].str {
                Text(q).font(.system(size: 12.5)).foregroundStyle(Color.mut).fixedSize(horizontal: false, vertical: true)
                    .padding(.leading, 12).overlay(alignment: .leading) { Rectangle().fill(Color.line2).frame(width: 2) }.padding(.top, 8)
            }
            if let src { panelRow("From \(src["title"].str)", icon: "doc.text") { sel = .note(src["id"].int) }.padding(.top, 8) }
            if it["source_id"].int > 0 {
                Button("Open the cited passage") { store.go(.source(it["source_id"].int, chunk: it["chunk_id"].int > 0 ? it["chunk_id"].int : nil)) }
                    .buttonStyle(.plain).font(.system(size: 12)).foregroundStyle(Color.mut)
            }
            HStack(spacing: 8) {
                if type == "step" && state != "done" { Button("Mark done") { run("/api/tick") }.buttonStyle(BtnStyle(kind: .primary)) }
                if type == "step" && (state == "open" || state == "doing") { Button(state == "doing" ? "Stop" : "Start") { run("/api/doing") }.buttonStyle(BtnStyle()) }
                if state == "proposed" {
                    Button("Accept") { run("/api/review", ["accept": true]) }.buttonStyle(BtnStyle(kind: .primary))
                    Button("Reject") { ask = ReasonAsk(title: "Why reject?") { run("/api/review", ["accept": false, "reason": $0]) } }.buttonStyle(BtnStyle())
                }
            }
            .padding(.top, 12)
            if type == "question" && state == "open" {
                HStack(spacing: 8) {
                    TextField("Your answer", text: $ans).textFieldStyle(.roundedBorder).onSubmit(answer)
                        .onChange(of: ans) { _, v in if v.count > 320 { ans = String(v.prefix(320)) } }
                    Button("Answer", action: answer).buttonStyle(BtnStyle(kind: .primary))
                }
                .padding(.top, 12)
            }
        }
        .reasonPrompt($ask)
    }

    private func answer() {
        Task {
            if let r = try? await API.post("/api/answer", ["id": it.id, "text": ans]), r["ok"].bool { ans = ""; store.changed() }
        }
    }

    private func run(_ path: String, _ extra: [String: Any] = [:]) {
        Task {
            _ = try? await API.post(path, extra.merging(["id": it.id]) { a, _ in a })
            store.changed()
        }
    }
}
