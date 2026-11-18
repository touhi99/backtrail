import SwiftUI

/// The first open step by creation time, as `bt brief` picks it.
func nextStep(_ detail: J?) -> String? {
    let steps = (detail?["items"].arr ?? []).filter { $0["type"].str == "step" && $0["state"].str == "open" }
    return steps.min { $0["created"].str < $1["created"].str }?["title"].s
}

/// Steps on the project page: built (done or waiting for your OK) and total. Nil until the detail loads.
func stepCounts(_ detail: J?) -> (built: Int, total: Int)? {
    guard let detail else { return nil }
    let steps = detail["items"].arr.filter { $0["type"].str == "step" && !["proposed", "rejected", "parked", "superseded"].contains($0["state"].str) }
    return (steps.filter { ["done", "looks_done"].contains($0["state"].str) }.count, steps.count)
}

/// Home: the most recently active project large, the other projects in motion as cards.
struct HomeView: View {
    @EnvironmentObject var store: Store
    @State private var details: [String: J] = [:]

    init() {
        var d: [String: J] = [:]
        for t in API.cached("/api/today")?["threads"].arr ?? [] where t["state"].str == "moving" {
            let n = t["name"].str
            if let x = API.cached("/api/thread?name=\(API.q(n))") { d[n] = x }
        }
        _details = State(initialValue: d)
    }

    var body: some View {
        let moving = store.moving.sorted { $0["last"].str > $1["last"].str }
        let asks = store.today?["asks"].arr.count ?? 0
        ScrollView {
            VStack(alignment: .leading, spacing: 26) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(Date.now.formatted(Date.FormatStyle(locale: Locale(identifier: "en_GB")).weekday(.wide).day().month(.wide)))
                        .font(.system(size: 13)).foregroundStyle(Color.mut)
                    Text("Home").font(.system(size: 28, weight: .semibold)).tracking(-0.4)
                }

                if let first = moving.first {
                    NowCard(t: first, detail: details[first["name"].str])
                }

                if asks > 0 {
                    Text("\(asks) \(asks == 1 ? "question" : "questions") typed in the app, waiting for Claude's next session.")
                        .font(.system(size: 13.5)).foregroundStyle(Color.soft)
                        .padding(.horizontal, 20).padding(.vertical, 14)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .panel(radius: 16)
                }

                if moving.count > 1 {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 300), spacing: 16)], spacing: 16) {
                        ForEach(moving.dropFirst(), id: \.["name"].str) { t in
                            ProjectCard(t: t, detail: details[t["name"].str])
                        }
                    }
                }

                HStack(spacing: 14) {
                    legend(.accent, "you said it")
                    legend(.moving, "you accepted it")
                    legend(.mut, "from a note")
                }
                .font(.system(size: 12)).foregroundStyle(Color.mut)
            }
            .padding(.horizontal, 32).padding(.top, 30).padding(.bottom, 44)
            .frame(maxWidth: 1080)
            .frame(maxWidth: .infinity)
        }
        .background(PageGlow())
        .task(id: store.tick) { await load(moving.map { $0["name"].str }) }
    }

    private func legend(_ c: Color, _ s: String) -> some View {
        HStack(spacing: 6) { Circle().fill(c).frame(width: 7, height: 7); Text(s) }
    }

    private func load(_ names: [String]) async {
        await withTaskGroup(of: (String, J?).self) { g in
            for n in names { g.addTask { (n, try? await API.get("/api/thread?name=\(API.q(n))")) } }
            for await (n, d) in g { if let d { details[n] = d } }
        }
    }
}

/// The one loud card: the project worked on last, with its next step.
struct NowCard: View {
    @EnvironmentObject var store: Store
    let t: J
    let detail: J?

    var body: some View {
        let name = t["name"].str
        let (done, tot) = stepCounts(detail) ?? (0, 0)
        let decided = Array((detail?["decided"].arr ?? []).prefix(3))
        Button { store.go(.thread(name)) } label: {
            HStack(alignment: .top, spacing: 28) {
                VStack(alignment: .leading, spacing: 16) {
                    HStack(spacing: 12) {
                        Glyph(name: name, color: .moving, size: 40)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(name).font(.system(size: 18, weight: .semibold))
                            Text("Last worked on \(dayFmt(t["last"].s))").font(.system(size: 12.5)).foregroundStyle(Color.mut)
                        }
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Next").font(.system(size: 13)).foregroundStyle(Color.mut)
                        Text(nextStep(detail) ?? (detail == nil ? " " : "No open step"))
                            .font(.system(size: 26, weight: .semibold)).tracking(-0.4)
                            .lineLimit(3).multilineTextAlignment(.leading)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    if !decided.isEmpty {
                        VStack(alignment: .leading, spacing: 7) {
                            Text("Decided last").font(.system(size: 12.5)).foregroundStyle(Color.mut)
                            ForEach(decided) { d in DecidedLine(d: d) }
                        }
                        .padding(.top, 4)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                VStack(spacing: 8) {
                    ZStack {
                        Ring(value: tot > 0 ? Double(done) / Double(tot) : 0, size: 112, stroke: 10)
                        VStack(spacing: 0) {
                            Text("\(done)").font(.system(size: 28, weight: .semibold)).monospacedDigit()
                            Text("of \(tot)").font(.system(size: 12)).foregroundStyle(Color.mut)
                        }
                    }
                    Text("steps built").font(.system(size: 12.5)).foregroundStyle(Color.mut)
                }
            }
            .padding(.horizontal, 28).padding(.vertical, 26)
            .background(alignment: .topTrailing) {
                Circle().fill(RadialGradient(colors: [Color.moving.opacity(0.2), .clear], center: .center, startRadius: 0, endRadius: 210))
                    .frame(width: 420, height: 420).offset(x: 120, y: -200).allowsHitTesting(false)
            }
            .background(LinearGradient(colors: [Color(hex: 0x18181c), Color(hex: 0x121214)], startPoint: .topLeading, endPoint: .bottomTrailing))
            .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 24, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
            .shadow(color: .black.opacity(0.35), radius: 25, y: 18)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .modifier(HoverLift())
    }
}

struct DecidedLine: View {
    let d: J
    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text(d["day"].str == "undated" ? "undated" : dayFmt(d["day"].s))
                .font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.mut).frame(width: 48, alignment: .leading)
            Text(d["title"].str).font(.system(size: 13)).foregroundStyle(Color.soft).lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
            Circle().fill(HOW_COLOR[d["how"].str] ?? .mut).frame(width: 7, height: 7)
                .help(HOW_NAME[d["how"].str] ?? d["how"].str)
        }
    }
}

struct ProjectCard: View {
    @EnvironmentObject var store: Store
    let t: J
    let detail: J?

    var body: some View {
        let name = t["name"].str
        let (done, tot) = stepCounts(detail) ?? (0, 0)
        let decided = Array((detail?["decided"].arr ?? []).prefix(2))
        Button { store.go(.thread(name)) } label: {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 12) {
                    Glyph(name: name, color: .moving)
                    Text(name).font(.system(size: 16, weight: .semibold)).lineLimit(1)
                    Spacer(minLength: 0)
                    if tot > 0 {
                        ZStack {
                            Ring(value: Double(done) / Double(tot))
                            Text("\(done)/\(tot)").font(.system(size: 10, weight: .semibold)).monospacedDigit().foregroundStyle(Color.soft)
                        }
                    }
                }
                .frame(height: 46)
                if let nx = nextStep(detail) {
                    (Text("Next: ").foregroundStyle(Color.mut) + Text(nx).foregroundStyle(Color.ink))
                        .font(.system(size: 13.5)).lineLimit(2)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                if !decided.isEmpty {
                    Divider().overlay(Color.line)
                    VStack(alignment: .leading, spacing: 6) { ForEach(decided) { d in DecidedLine(d: d) } }
                }
                Spacer(minLength: 0)
            }
            .padding(20)
            .frame(maxWidth: .infinity, minHeight: 168, alignment: .topLeading)
            .panel(radius: 18)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .modifier(HoverLift())
    }
}
