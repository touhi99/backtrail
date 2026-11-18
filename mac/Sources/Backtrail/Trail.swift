import SwiftUI

/// One thread's lane over the last two weeks: sessions, things made and items done or decided, by day.
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

/// The lanes the web app's Home shows: threads with a session in the last `days` days, the `limit` most
/// recently worked on first. Mirrors `useTrail(14)` and `TrailCard` in app/src/pages/Trail.tsx.
func trailLanes(_ data: J?, days: Int = 14, limit: Int = 8) -> (lanes: [TrailLane], total: Int, days: [String]) {
    let cal = Calendar(identifier: .gregorian)
    let f = DateFormatter()
    f.calendar = cal
    f.locale = Locale(identifier: "en_US_POSIX")
    f.dateFormat = "yyyy-MM-dd"
    let today = cal.startOfDay(for: .now)
    let axis = (0..<days).reversed().compactMap { cal.date(byAdding: .day, value: -$0, to: today) }.map { f.string(from: $0) }
    guard let data, let from = axis.first else { return ([], 0, axis) }

    var per: [String: TrailLane] = [:]
    for t in data["threads"].arr {
        per[t["name"].str] = TrailLane(name: t["name"].str, state: t["state"].str, summary: t["summary"].str)
    }
    for s in data["sources"].arr {
        let d = s["d"].str
        guard d >= from, per[s["thread"].str] != nil else { continue }
        if s["kind"].str == "session" { per[s["thread"].str]!.sessions[d, default: []].append(s) }
        else { per[s["thread"].str]!.made[d, default: []].append(s) }
    }
    for i in data["items"].arr {
        let d = i["d"].str
        guard d >= from, per[i["thread"].str] != nil else { continue }
        per[i["thread"].str]!.done[d, default: []].append(i)
    }
    var lanes = per.values.filter { !$0.sessions.isEmpty }
    for k in lanes.indices {
        let ds = (Array(lanes[k].sessions.keys) + Array(lanes[k].made.keys)).sorted()
        lanes[k].first = ds.first ?? from
        lanes[k].last = ds.last ?? from
    }
    lanes.sort { $0.last == $1.last ? $0.name < $1.name : $0.last > $1.last }
    return (Array(lanes.prefix(limit)), lanes.count, axis)
}

/// Home's "Last two weeks" card: one lane per recently worked thread, as on the web Home.
struct TrailCard: View {
    @EnvironmentObject var store: Store
    @Environment(\.openURL) private var openURL
    @State private var data: J? = API.cached("/api/trail")
    @State private var hover: String?

    private let rowH: CGFloat = 46, labelW: CGFloat = 180, stateW: CGFloat = 128, top: CGFloat = 30

    var body: some View {
        let m = trailLanes(data)
        Group {
            if !m.lanes.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    HStack(alignment: .firstTextBaseline) {
                        Text("Last two weeks").font(.system(size: 15, weight: .semibold))
                        Text("\(m.total) threads worked on\(m.total > m.lanes.count ? ", the \(m.lanes.count) most recent shown" : "")")
                            .font(.system(size: 12.5)).foregroundStyle(Color.mut)
                        Spacer()
                        Button("Full trail") { openURL(API.web("/graph?view=trail")) }
                            .buttonStyle(.plain).font(.system(size: 12.5, weight: .medium)).foregroundStyle(Color.accent)
                    }
                    GeometryReader { geo in lanes(m, width: max(640, geo.size.width)) }
                        .frame(height: top + CGFloat(m.lanes.count) * rowH + 8)
                }
                .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 10)
                .panel(radius: 16)
            } else {
                Color.clear.frame(height: 0) // a real view, so the fetch below runs before there is data
            }
        }
        .task(id: store.tick) { if let d = try? await API.get("/api/trail") { data = d } }
    }

    private func lanes(_ m: (lanes: [TrailLane], total: Int, days: [String]), width w: CGFloat) -> some View {
        let dw = (w - labelW - stateW) / CGFloat(m.days.count)
        let col = Dictionary(uniqueKeysWithValues: m.days.enumerated().map { ($1, $0) })
        func x(_ d: String) -> CGFloat { labelW + (CGFloat(col[d] ?? 0) + 0.5) * dw }
        return ZStack(alignment: .topLeading) {
            // day grid with labels
            ForEach(Array(m.days.enumerated()), id: \.offset) { k, d in
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
                    // span from first to last activity
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
