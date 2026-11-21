import SwiftUI

struct TrailLane: Identifiable {
    let name: String
    let state: String
    let summary: String
    var sessions: [String: [J]] = [:]
    var made: [String: [J]] = [:]
    var done: [String: [J]] = [:]
    var first = ""
    var last = ""
    var id: String { name }
}

func trailLanes(_ data: J?, days: Int? = 14) -> (lanes: [TrailLane], days: [String]) {
    var cal = Calendar(identifier: .gregorian)
    cal.timeZone = TimeZone(identifier: "UTC")!
    let f = DateFormatter()
    f.calendar = cal
    f.timeZone = TimeZone(identifier: "UTC")
    f.locale = Locale(identifier: "en_US_POSIX")
    f.dateFormat = "yyyy-MM-dd"
    let local = DateFormatter()
    local.locale = f.locale
    local.dateFormat = f.dateFormat
    let today = local.string(from: .now)
    guard let data, let end = f.date(from: today) else { return ([], []) }
    var from = days.flatMap { cal.date(byAdding: .day, value: -($0 - 1), to: end) }.map { f.string(from: $0) } ?? ""

    var per: [String: TrailLane] = [:]
    for t in data["threads"].arr {
        per[t["name"].str] = TrailLane(name: t["name"].str, state: t["state"].str, summary: t["summary"].str)
    }
    for s in data["sources"].arr {
        let d = s["d"].str
        guard d > "2000", d >= from, per[s["thread"].str] != nil else { continue }
        if s["kind"].str == "session" { per[s["thread"].str]!.sessions[d, default: []].append(s) }
        else { per[s["thread"].str]!.made[d, default: []].append(s) }
    }
    for i in data["items"].arr {
        let d = i["d"].str
        guard d >= from, per[i["thread"].str] != nil else { continue }
        per[i["thread"].str]!.done[d, default: []].append(i)
    }
    var lanes = per.values.filter { !$0.sessions.isEmpty }
    if from.isEmpty { from = lanes.flatMap { Array($0.sessions.keys) + Array($0.made.keys) }.min() ?? today }
    for k in lanes.indices {
        for (d, v) in lanes[k].done where d < from { lanes[k].done[d] = nil; lanes[k].done[from, default: []] += v }
        let ds = (Array(lanes[k].sessions.keys) + Array(lanes[k].made.keys)).sorted()
        lanes[k].first = ds.first ?? from
        lanes[k].last = ds.last ?? from
    }
    lanes.sort { $0.last == $1.last ? $0.first > $1.first : $0.last > $1.last }
    var axis: [String] = []
    var d = f.date(from: from) ?? end
    while d <= end { axis.append(f.string(from: d)); d = cal.date(byAdding: .day, value: 1, to: d)! }
    return (lanes, axis)
}

struct TrailCard: View {
    @EnvironmentObject var store: Store
    @State private var data: J? = API.cached("/api/trail")

    var body: some View {
        let all = trailLanes(data)
        let lanes = Array(all.lanes.prefix(8))
        Group {
            if !lanes.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    HStack(alignment: .firstTextBaseline) {
                        Text("Last two weeks").font(.system(size: 15, weight: .semibold))
                        Text("\(all.lanes.count) threads worked on\(all.lanes.count > lanes.count ? ", the \(lanes.count) most recent shown" : "")")
                            .font(.system(size: 12.5)).foregroundStyle(Color.mut)
                        Spacer()
                        Button("Full trail") { store.graphMode = "trail"; store.go(.graph) }
                            .buttonStyle(.plain).font(.system(size: 12.5, weight: .medium)).foregroundStyle(Color.accent)
                    }
                    TrailChart(lanes: lanes, days: all.days, labelW: 180)
                }
                .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 10)
                .panel(radius: 16)
            } else {
                Color.clear.frame(height: 0)
            }
        }
        .task(id: store.tick) { if let d = try? await API.get("/api/trail") { data = d } }
    }
}

struct TrailChart: View {
    @EnvironmentObject var store: Store
    let lanes: [TrailLane]
    let days: [String]
    var labelW: CGFloat = 210
    @State private var hover: String?

    private let rowH: CGFloat = 46, stateW: CGFloat = 128, top: CGFloat = 30

    var body: some View {
        GeometryReader { geo in chart(width: max(640, geo.size.width)) }
            .frame(height: top + CGFloat(lanes.count) * rowH + 8)
    }

    private func chart(width w: CGFloat) -> some View {
        let m = (lanes: lanes, days: days)
        let dw = (w - labelW - stateW) / CGFloat(m.days.count)
        let tick = m.days.count > 21 ? 7 : 1
        let col = Dictionary(uniqueKeysWithValues: m.days.enumerated().map { ($1, $0) })
        func x(_ d: String) -> CGFloat { labelW + (CGFloat(col[d] ?? 0) + 0.5) * dw }
        return ZStack(alignment: .topLeading) {
            ForEach(Array(m.days.enumerated()).filter { $0.offset % tick == 0 }, id: \.offset) { k, d in
                Rectangle().fill(Color(hex: 0x222226)).frame(width: 1, height: CGFloat(m.lanes.count) * rowH + 16)
                    .position(x: labelW + CGFloat(k) * dw, y: top - 8 + (CGFloat(m.lanes.count) * rowH + 16) / 2)
                Text(dayFmt(d)).font(.system(size: 11)).foregroundStyle(Color(hex: 0x6b6b75)).fixedSize()
                    .position(x: labelW + CGFloat(k) * dw + 4, y: top - 16).offset(x: 14)
            }
            ForEach(Array(m.lanes.enumerated()), id: \.element.id) { i, p in
                let c = STATE_COLOR[p.state] ?? .mut
                let y = top + CGFloat(i) * rowH + rowH / 2
                let dim = hover != nil && hover != p.name
                ZStack(alignment: .topLeading) {
                    RoundedRectangle(cornerRadius: 8).fill(hover == p.name ? Color.white.opacity(0.035) : .clear)
                        .frame(width: w, height: rowH).position(x: w / 2, y: y)
                    HStack(spacing: 8) {
                        Circle().fill(c).frame(width: 8, height: 8)
                        Text(p.name).font(.system(size: 13, weight: .medium)).foregroundStyle(Color(hex: 0xe4e4e7)).lineLimit(1)
                    }
                    .frame(width: labelW - 12, alignment: .leading).position(x: (labelW - 12) / 2 + 6, y: y)
                    .help(p.summary)
                    let x0 = x(p.first) - 7, x1 = x(p.last) + 7
                    Capsule().fill(c.opacity(0.12)).overlay(Capsule().strokeBorder(c.opacity(0.35), lineWidth: 1))
                        .frame(width: max(14, x1 - x0), height: 14).position(x: (x0 + x1) / 2, y: y)
                    ForEach(p.sessions.keys.sorted(), id: \.self) { d in
                        let v = p.sessions[d]!
                        let r = min(6.5, 2.6 + CGFloat(v.count) * 0.9)
                        Circle().fill(c).frame(width: r * 2, height: r * 2).position(x: x(d), y: y)
                            .help(tip(p.name, d, v, p.made[d] ?? [], p.done[d] ?? []))
                    }
                    ForEach(p.made.keys.sorted(), id: \.self) { d in
                        Rectangle().fill(Color.purple).frame(width: 8, height: 8).rotationEffect(.degrees(45))
                            .position(x: x(d), y: y - 15)
                            .help("Made, \(dayFmt(d))\n" + (p.made[d] ?? []).map { "• " + $0["title"].str }.joined(separator: "\n"))
                    }
                    ForEach(p.done.keys.sorted(), id: \.self) { d in
                        let n = p.done[d]!.count
                        Text("✓\(n > 1 ? "\(n)" : "")").font(.system(size: 10, weight: .bold)).foregroundStyle(Color.accent).fixedSize()
                            .position(x: x(d), y: y + 17)
                            .help("Done or decided, \(dayFmt(d))\n" + p.done[d]!.map { "• " + $0["title"].str }.joined(separator: "\n"))
                    }
                    Text(STATE_NAME[p.state] ?? p.state).font(.system(size: 12, weight: .semibold)).foregroundStyle(c).fixedSize()
                        .position(x: w - stateW + 10, y: y).offset(x: 40)
                }
                .opacity(dim ? 0.45 : 1)
                .contentShape(Rectangle().offset(y: y - rowH / 2).size(width: w, height: rowH))
                .onHover { hover = $0 ? p.name : (hover == p.name ? nil : hover) }
                .onTapGesture { store.go(.thread(p.name)) }
            }
        }
        .frame(width: w, alignment: .topLeading)
    }

    private func tip(_ name: String, _ d: String, _ s: [J], _ made: [J], _ done: [J]) -> String {
        ([ "\(name), \(dayFmt(d))" ] + s.map { "• " + $0["title"].str } + made.map { "◆ Made: " + $0["title"].str } + done.map { "✓ " + $0["title"].str })
            .joined(separator: "\n")
    }
}

struct TrailPage: View {
    @EnvironmentObject var store: Store
    let tabs: AnyView
    @State private var data: J? = API.cached("/api/trail")
    @State private var latest = true
    @State private var only: String?

    private static let order = ["built", "moving", "parked", "dropped"]

    var body: some View {
        let all = trailLanes(data, days: nil)
        var lanes = all.lanes.filter { only == nil || $0.state == only }
        if !latest { lanes.sort { $0.first == $1.first ? $0.name < $1.name : $0.first < $1.first } }
        return ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("All threads on one timeline, grouped by state, with the artifacts and commits each produced").font(.system(size: 13)).foregroundStyle(Color.mut)
                    HStack(spacing: 16) {
                        Text("Graph").font(.system(size: 28, weight: .semibold)).tracking(-0.4)
                        tabs
                    }
                }
                HStack(alignment: .top, spacing: 12) {
                    ForEach(Self.order, id: \.self) { st in tile(st, all.lanes.filter { $0.state == st }) }
                }
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 10) {
                        Text("Trail").font(.system(size: 15, weight: .semibold))
                        Text("\(only.map { "\(lanes.count) \(STATE_NAME[$0]?.lowercased() ?? $0), " } ?? "")click a thread to open its story")
                            .font(.system(size: 12.5)).foregroundStyle(Color.mut)
                        Spacer()
                        seg("Latest work", on: latest) { latest = true }
                        seg("First started", on: !latest) { latest = false }
                    }
                    TrailChart(lanes: lanes, days: all.days)
                }
                .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 10)
                .panel(radius: 16)
            }
            .padding(.horizontal, 32).padding(.top, 28).padding(.bottom, 32)
        }
        .background(Color.bg)
        .task(id: store.tick) { if let d = try? await API.get("/api/trail") { data = d } }
    }

    private func tile(_ st: String, _ g: [TrailLane]) -> some View {
        let c = STATE_COLOR[st] ?? .mut, on = only == st
        return Button { only = on ? nil : st } label: {
            VStack(alignment: .leading, spacing: 4) {
                Text("\(g.count)").font(.system(size: 34, weight: .semibold)).foregroundStyle(c)
                Text(STATE_NAME[st] ?? st).font(.system(size: 13)).foregroundStyle(Color.soft)
                Text(g.map(\.name).joined(separator: ", ")).font(.system(size: 12)).foregroundStyle(Color.mut).lineLimit(3)
            }
            .frame(maxWidth: .infinity, alignment: .topLeading).padding(16)
            .panel(radius: 14, border: on ? c : .line)
            .opacity(only != nil && !on ? 0.5 : 1)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain).help(on ? "Show all threads" : "Show only \(STATE_NAME[st] ?? st)")
    }

    private func seg(_ label: String, on: Bool, _ act: @escaping () -> Void) -> some View {
        Button(action: act) {
            Text(label).font(.system(size: 12, weight: .medium)).foregroundStyle(on ? Color.accent : Color.soft)
                .padding(.horizontal, 10).frame(height: 26)
                .background(Capsule().fill(on ? Color.accent.opacity(0.15) : Color.white.opacity(0.04)))
                .overlay(Capsule().strokeBorder(Color.line, lineWidth: 1))
        }
        .buttonStyle(.plain)
    }
}
