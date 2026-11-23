import SwiftUI

private let amber = Color(hex: 0xfbbf24)

private let kindStyle: [(key: String, label: String, color: Color, hint: String)] = [
    ("misread", "Misread you", Color(hex: 0xc084fc), "Misunderstood the request or lost context it had"),
    ("unchecked", "Didn’t check", Color(hex: 0xfbbf24), "Said it was done or working without checking"),
    ("off_target", "Off target", Color(hex: 0xfb7185), "Not what was wanted, or not good enough"),
    ("overreach", "Did too much", Color(hex: 0xfb923c), "Did or planned something not asked for"),
    ("too_much", "Too long", Color(hex: 0x2dd4bf), "Too much text, too many steps or too costly"),
    ("unresearched", "Didn’t research", Color(hex: 0x38bdf8), "Answered from memory instead of checking"),
]

private func pct(_ n: Int, _ d: Int) -> Int { d > 0 ? n * 100 / d : 0 }

private func heat(_ rate: Int) -> Color {
    let a = min(1, max(0, Double(rate - 30) / 45))
    return Color(red: 251 / 255, green: (191 - a * 80) / 255, blue: (36 + a * 60) / 255).opacity(0.18 + a * 0.62)
}

private func treemapRows(_ items: [J], rows: Int = 3) -> [[J]] {
    let total = items.reduce(0) { $0 + $1["turns"].int }
    var out: [[J]] = [], cur: [J] = [], sum = 0
    for t in items {
        cur.append(t); sum += t["turns"].int
        if Double(sum) >= Double(total) / Double(rows) * Double(out.count + 1) && out.count < rows - 1 { out.append(cur); cur = [] }
    }
    if !cur.isEmpty { out.append(cur) }
    return out
}

struct CorrectionsView: View {
    @EnvironmentObject var store: Store
    @State private var data: J? = API.cached("/api/corrections")
    @State private var kind = Snapshot.env["BACKTRAIL_KIND"]
    @State fileprivate var thread = Snapshot.env["BACKTRAIL_THREAD"]
    @State private var week: String?
    @State private var msgs: J?

    private var active: Bool { kind != nil || thread != nil || week != nil }
    private var query: String {
        "/api/corrections/messages?kind=\(API.q(kind ?? ""))&thread=\(API.q(thread ?? ""))&week=\(API.q(week ?? ""))"
    }

    var body: some View {
        ScrollViewReader { proxy in
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text("Corrections").font(.system(size: 28, weight: .semibold)).tracking(-0.4)
                if let d = data {
                    if d["weeks"].arr.isEmpty {
                        Text("Nothing tagged yet. Run skill/corrections.sh; the sweep keeps it current.")
                            .font(.system(size: 14)).foregroundStyle(Color.soft)
                            .padding(24).frame(maxWidth: .infinity, alignment: .leading).panel(radius: 20)
                    } else {
                        HStack(alignment: .top, spacing: 16) {
                            hero(d["weeks"].arr).frame(width: 300)
                            kinds(d).frame(maxWidth: .infinity)
                        }
                        threadMap(d["threads"].arr)
                        if active { messages.id("messages") }
                    }
                }
            }
            .padding(.horizontal, 32).padding(.top, 30).padding(.bottom, 44)
            .frame(maxWidth: 1080)
            .frame(maxWidth: .infinity)
        }
        .background(PageGlow())
        .task(id: store.tick) { data = try? await API.get("/api/corrections") }
        .task(id: active ? query : "") {
            guard active else { msgs = nil; return }
            msgs = try? await API.get(query)
            withAnimation(.easeOut(duration: 0.25)) { proxy.scrollTo("messages", anchor: .top) }
        }
        }
    }

    fileprivate func pick(kind k: String? = nil, thread t: String? = nil, week w: String? = nil) {
        withAnimation(.easeOut(duration: 0.15)) {
            if (k == nil || kind == k) && (t == nil || thread == t) && (w == nil || week == w) {
                if k != nil { kind = nil }
                if t != nil { thread = nil }
                if w != nil { week = nil }
            } else {
                if let k { kind = k }
                if let t { thread = t }
                if let w { week = w }
            }
        }
    }

    private func chip(_ label: String, color: Color?, clear: @escaping () -> Void) -> some View {
        HStack(spacing: 8) {
            if let color { RoundedRectangle(cornerRadius: 3).fill(color).frame(width: 10, height: 10) }
            Text(label).font(.system(size: 13, weight: .medium)).lineLimit(1)
            Button(action: { withAnimation(.easeOut(duration: 0.15)) { clear() } }) {
                Image(systemName: "xmark").font(.system(size: 9, weight: .bold)).foregroundStyle(Color.mut)
                    .frame(width: 20, height: 20).contentShape(Circle())
            }
            .buttonStyle(.plain)
        }
        .padding(.leading, 12).padding(.trailing, 6).frame(height: 32)
        .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
    }

    private var messages: some View {
        let n = msgs?["n"].int
        return VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                if let k = kind { chip(kindStyle.first { $0.key == k }?.label ?? k, color: kindStyle.first { $0.key == k }?.color) { kind = nil } }
                if let t = thread { chip(t, color: nil) { thread = nil } }
                if let w = week { chip("week of " + dayFmt(w), color: nil) { week = nil } }
                Text(n.map { "\($0) message\($0 == 1 ? "" : "s")" } ?? "…").font(.system(size: 13)).monospacedDigit().foregroundStyle(Color.mut)
                Spacer()
                Button { withAnimation(.easeOut(duration: 0.15)) { kind = nil; thread = nil; week = nil } } label: {
                    Image(systemName: "xmark").font(.system(size: 11, weight: .bold)).foregroundStyle(Color.soft)
                        .frame(width: 28, height: 28).background(Color.white.opacity(0.06), in: Circle()).contentShape(Circle())
                }
                .buttonStyle(.plain).help("Clear")
            }
            if n == 0 {
                Text("No pushback matches all of these. Remove a filter to widen it.").font(.system(size: 13)).foregroundStyle(Color.mut)
            }
            VStack(alignment: .leading, spacing: 2) {
                ForEach(msgs?["items"].arr ?? [], id: \.["chunk_id"].int) { q in
                    let color = kindStyle.first { $0.key == q["kind"].str }?.color ?? Color.mut
                    Button { store.go(.source(q["source_id"].int, chunk: q["chunk_id"].int)) } label: {
                        HStack(alignment: .top, spacing: 10) {
                            RoundedRectangle(cornerRadius: 1.5).fill(color).frame(width: 3)
                            (Text(q["text"].str).foregroundColor(Color.ink)
                                + Text("   " + q["thread"].str + " · " + dayFmt(String(q["ts"].str.prefix(10)))).foregroundColor(Color.mut).font(.system(size: 12)))
                                .font(.system(size: 14)).fixedSize(horizontal: false, vertical: true)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 8).padding(.vertical, 7)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            if let n, n > (msgs?["items"].arr.count ?? 0) {
                Text("Latest \(msgs?["items"].arr.count ?? 0) of \(n)").font(.system(size: 12)).foregroundStyle(Color.mut)
            }
        }
        .padding(20).frame(maxWidth: .infinity, alignment: .leading).panel(radius: 20)
    }

    private func hero(_ weeks: [J]) -> some View {
        let rates = weeks.map { pct($0["corrections"].int, $0["turns"].int) }
        let rate = rates.last ?? 0
        let delta = rates.count > 1 ? rate - rates[rates.count - 2] : 0
        return VStack(spacing: 12) {
            Text("This week").font(.system(size: 13)).foregroundStyle(Color.mut)
            ZStack {
                Ring(value: Double(rate) / 100, color: amber, size: 168, stroke: 14)
                VStack(spacing: 2) {
                    Text("\(rate)%").font(.system(size: 40, weight: .semibold)).monospacedDigit()
                    Text("pushed back").font(.system(size: 12.5)).foregroundStyle(Color.mut)
                }
            }
            if rates.count > 1 {
                Text("\(delta <= 0 ? "▼" : "▲") \(abs(delta)) pts").font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(delta <= 0 ? Color.accent : Color.dropped)
            }
            spark(rates).frame(height: 46)
        }
        .padding(20).frame(maxWidth: .infinity).panel(radius: 20)
        .help("Share of your messages that pushed back on the agent this week")
    }

    private func spark(_ v: [Int]) -> some View {
        GeometryReader { g in
            let top = Double(max(v.max() ?? 1, 1))
            let pt = { (i: Int) in CGPoint(x: v.count < 2 ? 0 : g.size.width * CGFloat(i) / CGFloat(v.count - 1),
                                          y: 6 + (g.size.height - 12) * (1 - CGFloat(Double(v[i]) / top))) }
            ZStack {
                Path { p in
                    for i in v.indices { i == 0 ? p.move(to: pt(i)) : p.addLine(to: pt(i)) }
                    p.addLine(to: CGPoint(x: g.size.width, y: g.size.height)); p.addLine(to: CGPoint(x: 0, y: g.size.height))
                }
                .fill(LinearGradient(colors: [amber.opacity(0.35), amber.opacity(0)], startPoint: .top, endPoint: .bottom))
                Path { p in for i in v.indices { i == 0 ? p.move(to: pt(i)) : p.addLine(to: pt(i)) } }
                    .stroke(amber, style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                if !v.isEmpty { Circle().fill(amber).frame(width: 7, height: 7).position(pt(v.count - 1)) }
            }
        }
    }

    private func kinds(_ d: J) -> some View {
        let weeks = d["weeks"].arr.filter { w in kindStyle.contains { w["kinds"][$0.key].int > 0 } }
        let all = d["kinds"].arr
        let total = max(all.reduce(0) { $0 + $1["n"].int }, 1)
        return VStack(alignment: .leading, spacing: 12) {
            Text("What goes wrong").font(.system(size: 13)).foregroundStyle(Color.mut)
            HStack(alignment: .bottom, spacing: 10) {
                ForEach(weeks, id: \.["week"].str) { w in
                    let n = max(kindStyle.reduce(0) { $0 + w["kinds"][$1.key].int }, 1)
                    VStack(spacing: 4) {
                        GeometryReader { g in
                            VStack(spacing: 2) {
                                ForEach(kindStyle.reversed(), id: \.key) { k in
                                    let v = w["kinds"][k.key].int
                                    if v > 0 {
                                        let lit = (kind == nil || kind == k.key) && (week == nil || week == w["week"].str)
                                        Rectangle().fill(k.color).opacity(lit ? 1 : 0.18)
                                            .frame(height: max(0, (g.size.height - 10) * CGFloat(v) / CGFloat(n)))
                                            .contentShape(Rectangle())
                                            .onTapGesture { pick(kind: k.key, week: w["week"].str) }
                                            .help("\(k.label), week of \(dayFmt(w["week"].s)): \(pct(v, n))%")
                                    }
                                }
                            }
                            .frame(maxHeight: .infinity, alignment: .bottom)
                            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                        }
                        Button { pick(week: w["week"].str) } label: {
                            Text(dayFmt(w["week"].s)).font(.system(size: 11.5, weight: week == w["week"].str ? .semibold : .regular))
                                .foregroundStyle(week == w["week"].str ? Color.ink : Color.mut)
                                .frame(maxWidth: .infinity).contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                    .frame(maxWidth: .infinity)
                }
            }
            .frame(height: 220)
            FlowChips(items: all.filter { $0["n"].int > 0 }.sorted { $0["n"].int > $1["n"].int }) { k in
                let style = kindStyle.first { $0.key == k["kind"].str }
                Button { pick(kind: k["kind"].str) } label: {
                    HStack(spacing: 8) {
                        RoundedRectangle(cornerRadius: 3).fill(style?.color ?? .mut).frame(width: 10, height: 10)
                        Text(style?.label ?? k["kind"].str).foregroundStyle(kind == k["kind"].str ? Color.ink : Color.soft).lineLimit(1).fixedSize()
                        Text("\(pct(k["n"].int, total))%").fontWeight(.semibold).monospacedDigit()
                    }
                    .font(.system(size: 13)).padding(.horizontal, 12).frame(height: 32)
                    .background(Color.white.opacity(kind == k["kind"].str ? 0.1 : 0.04), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain).help(style?.hint ?? "")
            }
            Text("Click a bar, a week, a kind or a box below to see those messages").font(.system(size: 12)).foregroundStyle(Color.mut)
        }
        .padding(20).panel(radius: 20)
    }

    private func threadMap(_ threads: [J]) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Where").font(.system(size: 13)).foregroundStyle(Color.mut)
                .help("Box size is how many messages; colour is how often they pushed back")
            GeometryReader { g in
                let rows = treemapRows(threads)
                let total = CGFloat(max(threads.reduce(0) { $0 + $1["turns"].int }, 1))
                VStack(spacing: 6) {
                    ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                        let rowSum = CGFloat(row.reduce(0) { $0 + $1["turns"].int })
                        HStack(spacing: 6) {
                            ForEach(row, id: \.["thread"].str) { t in
                                let w = max(0, (g.size.width - 6 * CGFloat(row.count - 1)) * CGFloat(t["turns"].int) / rowSum)
                                let h = max(0, (g.size.height - 6 * CGFloat(rows.count - 1)) * rowSum / total)
                                box(t, width: w, height: h)
                            }
                        }
                    }
                }
            }
            .frame(height: 300)
        }
        .padding(20).panel(radius: 20)
    }
}

extension CorrectionsView {
    fileprivate func box(_ t: J, width: CGFloat, height: CGFloat) -> some View {
        let r = pct(t["corrections"].int, t["turns"].int)
        let on = thread == t["thread"].str
        return Button { pick(thread: t["thread"].str) } label: {
            VStack(alignment: .leading, spacing: 1) {
                Spacer(minLength: 0)
                Text(t["thread"].str).font(.system(size: 13, weight: .semibold)).foregroundStyle(Color.ink).lineLimit(1)
                Text("\(r)%").font(.system(size: 12)).monospacedDigit().foregroundStyle(Color.soft)
            }
            .padding(12)
            .frame(width: width, height: height, alignment: .bottomLeading)
            .background(heat(r), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color.white.opacity(on ? 0.8 : 0), lineWidth: 2))
            .opacity(thread == nil || on ? 1 : 0.45)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("\(r)% of \(t["turns"].int) messages")
    }
}

private struct FlowChips<Content: View>: View {
    let items: [J]
    @ViewBuilder let content: (J) -> Content

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 175), spacing: 8, alignment: .leading)], alignment: .leading, spacing: 8) {
            ForEach(items, id: \.["kind"].str) { content($0) }
        }
    }
}
