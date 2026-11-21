import Charts
import SwiftUI

struct Story {
    struct Session { let day: String; let title: String; let id: Int }
    struct Made { let day: String; let title: String; let id: Int; let kind: String; let n: Int }
    struct Event: Identifiable { let id: String; let day: String; let text: String; let quote: String?; let you: Bool; let type: String }
    struct Turn { let day: String; let old: String; let new: String }
    struct Milestone: Identifiable { var id: String { name }; let name: String; let items: [J]; let done: Int; let check: Int; let d0: String?; let d1: String? }
    struct Settled: Identifiable { let id = UUID(); let text: String; let quote: String?; let you: Bool; let ok: Bool; let day: String; let answer: Bool; var item: J? = nil }

    var sessions: [Session] = []
    var made: [Made] = []
    var steps: [J] = []
    var milestones: [Milestone] = []
    var events: [Event] = []
    var turns: [Turn] = []
    var settled: [Settled] = []
    var first: String?
    var last: String?

    static func isYou(_ who: String?) -> Bool { who == "owner" || who == "you" }
    static func isDone(_ s: String) -> Bool { s == "done" || s == "looks_done" }

    private static func words(_ s: String) -> Set<String> {
        Set(s.lowercased().split { !$0.isLetter && !$0.isNumber }.map(String.init).filter { $0.count > 2 })
    }
    private static func similar(_ a: String, _ b: String) -> Double {
        let A = words(a), B = words(b)
        return Double(A.intersection(B).count) / Double(max(1, min(A.count, B.count)))
    }
    private static func firstNumber(_ s: String) -> Int {
        Int(s.drop { !$0.isNumber }.prefix { $0.isNumber }) ?? 0
    }

    init(_ d: J) {
        let live = d["items"].arr.filter { !["proposed", "rejected", "parked"].contains($0["state"].str) }
        sessions = d["sources"].arr.filter { $0["kind"].str == "session" && $0["first_ts"].s != nil }
            .map { Session(day: String($0["first_ts"].str.prefix(10)), title: $0["title"].str, id: $0["id"].int) }
        made = d["sources"].arr.filter { ["artifact", "commit"].contains($0["kind"].str) && ($0["first_ts"].s != nil || $0["last_ts"].s != nil) }
            .map { s in
                let t = s["kind"].str == "commit" ? (s["last_ts"].s ?? s["first_ts"].str) : (s["first_ts"].s ?? s["last_ts"].str)
                return Made(day: String(t.prefix(10)), title: s["title"].str, id: s["id"].int, kind: s["kind"].str, n: s["nchunks"].int)
            }
            .sorted { $0.day < $1.day }
        steps = live.filter { $0["type"].str == "step" }

        var groups: [String: [J]] = [:]
        var order: [String] = []
        for s in steps {
            let t = s["title"].str
            let prefix = t.prefix { $0.isUppercase && $0.isLetter }
            var key = "other"
            if !prefix.isEmpty, t.dropFirst(prefix.count).hasPrefix("-"), t.dropFirst(prefix.count + 1).first?.isNumber == true { key = String(prefix) }
            else if t.first?.isNumber == true, t.contains(" ·") { key = "#" }
            if groups[key] == nil { order.append(key) }
            groups[key, default: []].append(s)
        }
        milestones = order.map { k in
            var items = groups[k]!
            if k != "other" { items.sort { Self.firstNumber($0["title"].str) < Self.firstNumber($1["title"].str) } }
            let ds = items.map { String(($0["updated"].s ?? $0["created"].str).prefix(10)) }.filter { !$0.isEmpty }.sorted()
            let name = k == "other" ? "Other steps" : k == "#" ? "Numbered plan"
                : "\(k)-\(Self.firstNumber(items.first!["title"].str)) to \(k)-\(Self.firstNumber(items.last!["title"].str))"
            return Milestone(name: name, items: items, done: items.filter { $0["state"].str == "done" }.count,
                             check: items.filter { $0["state"].str == "looks_done" }.count, d0: ds.first, d1: ds.last)
        }
        .sorted { a, b in
            if a.name == "Other steps" { return false }
            if b.name == "Other steps" { return true }
            return (a.d0 ?? "") < (b.d0 ?? "")
        }

        let claims = d["claims"].arr
        events = claims.filter { ["state", "rationale"].contains($0["type"].str) && !$0["d"].str.isEmpty }
            .map { Event(id: $0.id, day: $0["d"].str, text: $0["text"].str, quote: $0["q"].s, you: Self.isYou($0["speaker"].s), type: $0["type"].str) }
        turns = claims.filter { $0["type"].str == "decision" && $0["status"].str == "superseded" && $0["newer"].s != nil
            && Self.similar($0["text"].str, $0["newer"].str) < 0.6 }
            .map { Turn(day: $0["d"].str, old: $0["text"].str, new: $0["newer"].str) }

        var seen = Set<String>()
        func add(_ x: Settled) { if seen.insert(x.text.lowercased()).inserted { settled.append(x) } }
        for i in live where i["type"].str == "decision" && !(i["stale"].s ?? "").hasPrefix("superseded") {
            add(Settled(text: i["title"].str, quote: i["quote"].s, you: i["origin"].str == "owner", ok: true, day: String(i["created"].str.prefix(10)), answer: false, item: i))
        }
        for i in live where i["type"].str == "question" && i["state"].str != "open" && i["answer"].s != nil {
            add(Settled(text: i["title"].str, quote: i["answer"].s, you: i["origin"].str != "owner", ok: true, day: String(i["created"].str.prefix(10)), answer: true))
        }
        for c in claims where c["type"].str == "decision" && c["status"].str == "active" {
            add(Settled(text: c["text"].str, quote: c["q"].s, you: Self.isYou(c["speaker"].s), ok: true, day: c["d"].str, answer: false))
        }
        let proposed = claims.filter { $0["type"].str == "decision" && $0["status"].str == "proposed" }
            .sorted { a, b in
                let ya = Self.isYou(a["speaker"].s), yb = Self.isYou(b["speaker"].s)
                return ya != yb ? ya : a["d"].str > b["d"].str
            }
        for c in proposed { add(Settled(text: c["text"].str, quote: c["q"].s, you: Self.isYou(c["speaker"].s), ok: false, day: c["d"].str, answer: false)) }

        let dates = (sessions.map(\.day) + made.filter { $0.kind == "artifact" }.map(\.day) + events.map(\.day)).filter { $0 > "2000" }.sorted()
        first = dates.first
        last = dates.last
    }
}

private func dayDate(_ s: String) -> Date {
    let f = DateFormatter()
    f.dateFormat = "yyyy-MM-dd"
    f.timeZone = TimeZone(identifier: "UTC")
    return f.date(from: String(s.prefix(10))) ?? .distantPast
}

private func clip(_ s: String, _ n: Int) -> String {
    guard s.count > n else { return s }
    let cut = s.prefix(n)
    return String(cut[..<(cut.lastIndex(of: " ") ?? cut.endIndex)]) + "…"
}

struct JourneyView: View {
    let s: Story
    let color: Color
    @State private var hoverDay: Date?

    private struct Day: Identifiable { var id: Date { day }; let day: Date; let n: Int; let titles: [String] }

    var body: some View {
        let days = perDay()
        let maxN = max(1, days.map(\.n).max() ?? 1)
        let moments = s.events.suffix(40)
        let arts = s.made.filter { $0.kind == "artifact" }
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text("Journey").font(.system(size: 15, weight: .semibold))
                Text("\(s.sessions.count) sessions\(s.first != nil ? " since \(dayFmt(s.first))" : "")").font(.system(size: 12.5)).foregroundStyle(Color.mut)
            }
            Chart {
                ForEach(days) { d in
                    AreaMark(x: .value("Day", d.day, unit: .day), y: .value("Sessions", d.n))
                        .interpolationMethod(.monotone)
                        .foregroundStyle(LinearGradient(colors: [color.opacity(0.5), color.opacity(0.04)], startPoint: .top, endPoint: .bottom))
                    LineMark(x: .value("Day", d.day, unit: .day), y: .value("Sessions", d.n))
                        .interpolationMethod(.monotone).foregroundStyle(color).lineStyle(StrokeStyle(lineWidth: 1.5))
                }
                ForEach(moments) { e in
                    PointMark(x: .value("Day", dayDate(e.day), unit: .day), y: .value("Sessions", -Double(maxN) * 0.12))
                        .symbolSize(28).foregroundStyle(e.you ? Color.accent : Color.mut)
                }
                ForEach(Array(arts.enumerated()), id: \.offset) { _, m in
                    PointMark(x: .value("Day", dayDate(m.day), unit: .day), y: .value("Sessions", -Double(maxN) * 0.26))
                        .symbol(.diamond).symbolSize(55).foregroundStyle(Color.purple)
                }
                if let hoverDay {
                    RuleMark(x: .value("Day", hoverDay, unit: .day)).foregroundStyle(Color.white.opacity(0.18))
                }
            }
            .chartYAxis(.hidden)
            .chartYScale(domain: -Double(maxN) * 0.36 ... Double(maxN) * 1.08)
            .chartXAxis {
                AxisMarks(values: .stride(by: .day, count: strideDays())) { _ in
                    AxisValueLabel(format: .dateTime.day().month(.abbreviated)).foregroundStyle(Color.mut)
                }
            }
            .chartOverlay { proxy in
                GeometryReader { _ in
                    Rectangle().fill(.clear).contentShape(Rectangle())
                        .onContinuousHover { phase in
                            switch phase {
                            case .active(let p):
                                if let d: Date = proxy.value(atX: p.x) { hoverDay = Calendar.utc.startOfDay(for: d) }
                            case .ended: hoverDay = nil
                            }
                        }
                }
            }
            .environment(\.locale, Locale(identifier: "en_GB"))
            .frame(height: 200)

            detail(days)
                .frame(minHeight: 64, alignment: .topLeading)

            HStack(spacing: 16) {
                legend(AnyView(RoundedRectangle(cornerRadius: 2).fill(color.opacity(0.6)).frame(width: 10, height: 10)), "Sessions per day")
                legend(AnyView(Rectangle().fill(Color.purple).frame(width: 8, height: 8).rotationEffect(.degrees(45))), "Made")
                legend(AnyView(Circle().fill(Color.accent).frame(width: 8, height: 8)), "Something you said")
                legend(AnyView(Circle().fill(Color.mut).frame(width: 8, height: 8)), "Something Claude found")
            }
            .font(.system(size: 12)).foregroundStyle(Color.mut)
        }
        .padding(20)
        .panel(radius: 18)
    }

    @ViewBuilder private func detail(_ days: [Day]) -> some View {
        if let hoverDay {
            let key = ISO8601DateFormatter.day.string(from: hoverDay)
            let day = days.first { $0.day == hoverDay }
            let said = s.events.filter { $0.day.hasPrefix(key) }
            let madeThen = s.made.filter { $0.day == key }
            VStack(alignment: .leading, spacing: 4) {
                Text("\(dayFmt(key)), \(day?.n ?? 0) session\(day?.n == 1 ? "" : "s")").font(.system(size: 13, weight: .semibold))
                ForEach(Array((day?.titles ?? []).prefix(4).enumerated()), id: \.offset) { _, t in
                    Text("• \(t)").font(.system(size: 12.5)).foregroundStyle(Color.soft).lineLimit(1)
                }
                ForEach(madeThen.prefix(2), id: \.id) { m in
                    Text("◆ Made: \(m.title)").font(.system(size: 12.5)).foregroundStyle(Color.purple).lineLimit(1)
                }
                ForEach(said.prefix(3)) { e in
                    Text("\(e.you ? "You" : "Claude"): \(clip(e.text, 120))").font(.system(size: 12.5))
                        .foregroundStyle(e.you ? Color.accent : Color.mut).lineLimit(1)
                }
            }
        } else if let e = s.events.last(where: { $0.you }) ?? s.events.last {
            VStack(alignment: .leading, spacing: 4) {
                Text("Latest moment, \(dayFmt(e.day))").font(.system(size: 12.5)).foregroundStyle(Color.mut)
                Text(e.text).font(.system(size: 14, weight: .medium)).lineLimit(2)
            }
        }
    }

    private func legend(_ mark: AnyView, _ s: String) -> some View { HStack(spacing: 6) { mark; Text(s) } }

    private func strideDays() -> Int {
        guard let f = s.first, let l = s.last else { return 1 }
        let span = dayDate(l).timeIntervalSince(dayDate(f)) / 86400
        return span > 60 ? 14 : span > 21 ? 7 : span > 8 ? 2 : 1
    }

    private func perDay() -> [Day] {
        guard let f = s.first, let l = s.last else { return [] }
        var by: [String: [String]] = [:]
        for q in s.sessions { by[q.day, default: []].append(q.title) }
        var out: [Day] = []
        var d = dayDate(f)
        let end = dayDate(l)
        while d <= end {
            let k = ISO8601DateFormatter.day.string(from: d)
            out.append(Day(day: d, n: by[k]?.count ?? 0, titles: by[k] ?? []))
            d = d.addingTimeInterval(86400)
        }
        return out
    }
}

extension Calendar {
    static let utc: Calendar = {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "UTC")!
        return c
    }()
}

extension ISO8601DateFormatter {
    static let day: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withFullDate]
        return f
    }()
}

struct ProgressRing: View {
    let done: Int, check: Int, total: Int
    var size: CGFloat = 44
    let color: Color
    var body: some View {
        ZStack {
            Circle().stroke(Color(hex: 0x26262b), lineWidth: 4)
            Circle().trim(from: 0, to: total > 0 ? Double(done + check) / Double(total) : 0)
                .stroke(Color(hex: 0xfacc15), style: StrokeStyle(lineWidth: 4, lineCap: .round)).rotationEffect(.degrees(-90))
            Circle().trim(from: 0, to: total > 0 ? Double(done) / Double(total) : 0)
                .stroke(color, style: StrokeStyle(lineWidth: 4, lineCap: .round)).rotationEffect(.degrees(-90))
        }
        .frame(width: size, height: size)
    }
}

struct MilestonesView: View {
    @EnvironmentObject var store: Store
    let s: Story
    let color: Color
    @State private var open: Set<String> = []

    var body: some View {
        let order = ["doing": 0, "open": 1, "looks_done": 2, "done": 3]
        HStack(alignment: .top, spacing: 16) {
            if !s.milestones.isEmpty {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Milestones").font(.system(size: 15, weight: .semibold))
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 250), spacing: 12, alignment: .top)], alignment: .leading, spacing: 12) {
                        ForEach(s.milestones) { m in
                            let its = m.items.sorted { (order[$0["state"].str] ?? 4) < (order[$1["state"].str] ?? 4) }
                            let all = open.contains(m.name)
                            VStack(alignment: .leading, spacing: 10) {
                                HStack(spacing: 12) {
                                    ProgressRing(done: m.done, check: m.check, total: m.items.count, color: color)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(m.name).font(.system(size: 15, weight: .semibold))
                                        Text("\(m.done + m.check) of \(m.items.count) done\(m.check > 0 ? ", \(m.check) for you to check" : "")")
                                            .font(.system(size: 12)).foregroundStyle(Color.mut)
                                    }
                                }
                                VStack(alignment: .leading, spacing: 4) {
                                    ForEach(all ? its : Array(its.prefix(4))) { x in
                                        HStack(alignment: .firstTextBaseline, spacing: 8) {
                                            let st = x["state"].str
                                            Circle().fill(st == "done" ? Color.accent : st == "looks_done" ? Color(hex: 0xfacc15) : .clear)
                                                .overlay(Circle().strokeBorder(Story.isDone(st) ? .clear : Color.mut, lineWidth: 1.5))
                                                .frame(width: 7, height: 7)
                                            Text(x["title"].str).font(.system(size: 12.5)).foregroundStyle(Color.mut)
                                        }
                                    }
                                }
                                if its.count > 4 {
                                    Button(all ? "Show fewer" : "Show \(its.count - 4) more") {
                                        if all { open.remove(m.name) } else { open.insert(m.name) }
                                    }
                                    .buttonStyle(.plain).font(.system(size: 12.5, weight: .medium)).foregroundStyle(Color.accent)
                                }
                            }
                            .padding(14).frame(maxWidth: .infinity, alignment: .topLeading)
                            .background(Color.white.opacity(0.035), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                        }
                    }
                }
                .padding(20).frame(maxWidth: .infinity, alignment: .topLeading).panel(radius: 18)
            }
            if !s.turns.isEmpty {
                VStack(alignment: .leading, spacing: 10) {
                    Text("Changed course").font(.system(size: 15, weight: .semibold))
                    ForEach(Array(s.turns.suffix(6).reversed().enumerated()), id: \.offset) { _, t in
                        HStack(alignment: .top, spacing: 12) {
                            Image(systemName: "arrow.uturn.backward").font(.system(size: 12, weight: .semibold)).foregroundStyle(Color.open)
                                .frame(width: 28, height: 28).background(Color.open.opacity(0.15), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                            VStack(alignment: .leading, spacing: 3) {
                                Text(t.old).font(.system(size: 13)).foregroundStyle(Color.mut).strikethrough(color: Color.dropped.opacity(0.6))
                                Text(t.new).font(.system(size: 13.5))
                                Text(dayFmt(t.day)).font(.system(size: 11.5)).foregroundStyle(Color.mut)
                            }
                        }
                        .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.white.opacity(0.035), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    }
                }
                .padding(20).frame(maxWidth: s.milestones.isEmpty ? .infinity : 380, alignment: .topLeading).panel(radius: 18)
            }
        }
    }
}

struct SettledView: View {
    let s: Story
    @State private var all = false

    var body: some View {
        let ok = s.settled.filter(\.ok).count
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text("What you settled").font(.system(size: 15, weight: .semibold))
                Text("\(s.settled.count) decisions, \(ok) confirmed").font(.system(size: 12.5)).foregroundStyle(Color.mut)
            }
            if s.settled.isEmpty { Text("No decisions yet.").font(.system(size: 13)).foregroundStyle(Color.mut) }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 270), spacing: 12, alignment: .top)], alignment: .leading, spacing: 12) {
                ForEach(all ? s.settled : Array(s.settled.prefix(12))) { x in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(x.text).font(.system(size: 14.5, weight: .semibold)).fixedSize(horizontal: false, vertical: true)
                        if let q = x.quote, !q.isEmpty {
                            Text(x.answer ? q : "“\(clip(q, 150))”").font(.system(size: 12.5)).italic().foregroundStyle(Color.mut)
                                .padding(.leading, 10).overlay(alignment: .leading) { Rectangle().fill(Color.line2).frame(width: 2) }
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        let also = x.item?["also"].arr ?? []
                        if !also.isEmpty {
                            Text("Also considered: " + alsoText(also))
                                .font(.system(size: 12.5)).foregroundStyle(Color.mut).lineLimit(2)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        Spacer(minLength: 0)
                        HStack(spacing: 8) {
                            Chip(text: x.answer ? (x.you ? "You answered" : "Claude answered") : x.you ? "You said" : "Claude",
                                 color: x.you ? .accent : .moving)
                            if x.ok { Text("✓ confirmed").foregroundStyle(Color.accent) }
                            Text(dayFmt(x.day))
                            if let it = x.item { Spacer(minLength: 0); ItemActions(i: it) }
                        }
                        .font(.system(size: 11.5)).foregroundStyle(Color.mut)
                    }
                    .padding(14).frame(maxWidth: .infinity, minHeight: 120, alignment: .topLeading)
                    .background(Color.white.opacity(0.035), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(x.ok ? Color.accent.opacity(0.27) : .clear, lineWidth: 1))
                }
            }
            if s.settled.count > 12 {
                Button(all ? "Show fewer" : "Show all \(s.settled.count)") { all.toggle() }
                    .buttonStyle(.plain).font(.system(size: 12.5, weight: .medium)).foregroundStyle(Color.accent)
            }
        }
        .padding(20).panel(radius: 18)
    }
}

struct MadeView: View {
    @EnvironmentObject var store: Store
    let s: Story

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text("Made").font(.system(size: 15, weight: .semibold))
                Text("\(s.made.count)").font(.system(size: 12.5)).foregroundStyle(Color.mut)
            }
            if s.made.isEmpty { Text("Nothing made yet.").font(.system(size: 13)).foregroundStyle(Color.mut) }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 14)], alignment: .leading, spacing: 14) {
                ForEach(Array(s.made.reversed().enumerated()), id: \.offset) { _, m in
                    Button { store.go(.source(m.id, chunk: nil)) } label: {
                        VStack(alignment: .leading, spacing: 10) {
                            Image(systemName: m.kind == "commit" ? "arrow.triangle.branch" : "doc.richtext")
                                .font(.system(size: 18)).foregroundStyle(m.kind == "commit" ? Color.accent : Color.purple)
                                .frame(width: 38, height: 38)
                                .background((m.kind == "commit" ? Color.accent : Color.purple).opacity(0.13), in: RoundedRectangle(cornerRadius: 11, style: .continuous))
                            Text(m.title).font(.system(size: 13.5, weight: .medium)).lineLimit(2).multilineTextAlignment(.leading)
                            Text(m.kind == "commit" ? "Commits, \(dayFmt(m.day))" : dayFmt(m.day)).font(.system(size: 12)).foregroundStyle(Color.mut)
                        }
                        .padding(14).frame(maxWidth: .infinity, minHeight: 130, alignment: .topLeading)
                        .background(Color.white.opacity(0.035), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain).modifier(HoverLift())
                }
            }
        }
        .padding(20).panel(radius: 18)
    }
}

struct SourcesView: View {
    @EnvironmentObject var store: Store
    let sources: [J]
    @State private var shown = 15

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Sources").font(.system(size: 15, weight: .semibold))
                Spacer()
                Text("\(sources.count)").font(.system(size: 12.5)).foregroundStyle(Color.mut)
            }
            .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 8)
            ForEach(sources.prefix(shown)) { s in
                Divider().overlay(Color.line)
                RowButton { store.go(.source(s["id"].int, chunk: nil)) } label: {
                    HStack(spacing: 12) {
                        Circle().fill(SOURCE_KIND_COLOR[s["kind"].str] ?? .mut).frame(width: 8, height: 8)
                        Text(s["title"].str).font(.system(size: 13.5)).lineLimit(1)
                        Spacer()
                        Text(dayFmt(s["last_ts"].s)).font(.system(size: 12)).foregroundStyle(Color.mut)
                    }
                }
            }
            if sources.count > shown {
                Divider().overlay(Color.line)
                Button("Show \(min(15, sources.count - shown)) more") { shown += 15 }
                    .buttonStyle(BtnStyle(kind: .ghost)).padding(.horizontal, 12).padding(.vertical, 8)
            }
        }
        .panel(radius: 18)
    }
}

struct ArtifactsView: View {
    @Environment(\.openURL) private var openURL
    let artifacts: [J]
    @State private var shown = 15

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Artifacts").font(.system(size: 15, weight: .semibold))
                Spacer()
                Text("\(artifacts.count)").font(.system(size: 12.5)).foregroundStyle(Color.mut)
            }
            .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 8)
            if artifacts.isEmpty {
                Text("No artifact links found for this project.").font(.system(size: 13)).foregroundStyle(Color.mut)
                    .padding(.horizontal, 20).padding(.bottom, 20)
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 240), spacing: 18, alignment: .top)], alignment: .leading, spacing: 20) {
                ForEach(artifacts.prefix(shown)) { a in
                    Button { if let u = URL(string: a["url"].str) { openURL(u) } } label: {
                        VStack(alignment: .leading, spacing: 6) {
                            if a["source_id"].isNull {
                                RoundedRectangle(cornerRadius: 10).fill(Color(white: 0.08))
                                    .aspectRatio(16.0 / 10.0, contentMode: .fit)
                                    .overlay(Image(systemName: "doc").foregroundStyle(Color.mut))
                            } else {
                                ArtifactThumb(sourceID: a["source_id"].int)
                            }
                            Text(a["title"].str.isEmpty ? a["url"].str : a["title"].str)
                                .font(.system(size: 13, weight: .medium)).lineLimit(1)
                            Text(dayFmt(a["ts"].s)).font(.system(size: 12)).foregroundStyle(Color.mut)
                            if !a["about"].str.isEmpty {
                                Text(a["about"].str).font(.system(size: 12)).foregroundStyle(Color.mut).lineLimit(2)
                                    .multilineTextAlignment(.leading)
                            }
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 20).padding(.bottom, 16)
            if artifacts.count > shown {
                Divider().overlay(Color.line)
                Button("Show \(min(15, artifacts.count - shown)) more") { shown += 15 }
                    .buttonStyle(BtnStyle(kind: .ghost)).padding(.horizontal, 12).padding(.vertical, 8)
            }
        }
        .panel(radius: 18)
    }
}
