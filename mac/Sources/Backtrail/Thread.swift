import SwiftUI

/// What a step's state means.
let STEP_STATUS: [String: (String, Color, String)] = [
    "doing": ("In progress", .moving, "You marked it in progress. Claude does not set this."),
    "looks_done": ("Built, check it", .accent, "Claude or a commit says it is built. Confirm it works, or say not yet."),
]

/// A project page: latest decisions, progress, the next three steps, then all open steps.
struct ThreadView: View {
    @EnvironmentObject var store: Store
    @Environment(\.openURL) private var openURL
    let name: String
    @State private var d: J?

    init(name: String) {
        self.name = name
        let c = API.cached("/api/thread?name=\(API.q(name))")
        _d = State(initialValue: c)
        _story = State(initialValue: c.map(Story.init))
    }
    @State private var showDone = false
    @State private var shown = 10
    @State private var tab = Tab(rawValue: Snapshot.env["BACKTRAIL_TAB"] ?? "") ?? .steps
    @State private var story: Story?

    enum Tab: String, CaseIterable { case steps = "Steps", journey = "Journey", decisions = "Decisions", made = "Made", artifacts = "Artifacts", sources = "Sources" }

    var body: some View {
        ScrollView {
            if let d, !d["thread"].isNull { content(d) } else { Color.clear.frame(height: 400) }
        }
        .background(PageGlow())
        .task(id: store.tick) {
            if let fresh = try? await API.get("/api/thread?name=\(API.q(name))") {
                d = fresh
                story = Story(fresh)
            }
        }
    }

    private func content(_ d: J) -> some View {
        let th = d["thread"]
        let c = STATE_COLOR[th["state"].str] ?? .mut
        let items = d["items"].arr
        let steps = items.filter { $0["type"].str == "step" && !["proposed", "rejected", "parked", "superseded"].contains($0["state"].str) }
        let doneN = steps.filter { $0["state"].str == "done" }.count
        let checkN = steps.filter { $0["state"].str == "looks_done" }.count
        let todo = steps.filter { ["open", "doing"].contains($0["state"].str) }
            .sorted { ($0["state"].str == "doing" ? 0 : 1) < ($1["state"].str == "doing" ? 0 : 1) }
        let openSteps = steps.filter { $0["state"].str != "done" }
            .sorted { ($0["state"].str == "looks_done" ? 0 : $0["state"].str == "doing" ? 1 : 2) < ($1["state"].str == "looks_done" ? 0 : $1["state"].str == "doing" ? 1 : 2) }
        let doneSteps = steps.filter { $0["state"].str == "done" }.sorted { $0["updated"].str > $1["updated"].str }
        let since = sinceYesterday(d, steps: steps, items: items)

        return VStack(alignment: .leading, spacing: 26) {
            HStack(spacing: 14) {
                Glyph(name: th["name"].str, color: c, size: 44).shadow(color: c.opacity(0.25), radius: 14)
                VStack(alignment: .leading, spacing: 2) {
                    Text(th["name"].str).font(.system(size: 26, weight: .semibold)).tracking(-0.4)
                    if let s = th["summary"].s { Text(s).font(.system(size: 13.5)).foregroundStyle(Color.mut).lineLimit(1) }
                }
                Spacer()
                Button { store.go(.search("")) } label: { Label("Search", systemImage: "magnifyingglass") }
                    .buttonStyle(BtnStyle(kind: .ghost))
                Button { store.capture = .some(name) } label: { Label("Add", systemImage: "plus") }
                    .buttonStyle(BtnStyle())
            }

            HStack(alignment: .top, spacing: 20) {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Latest decisions").font(.system(size: 15, weight: .semibold))
                        .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 8)
                    let decided = d["decided"].arr
                    if decided.isEmpty {
                        Text("No settled decisions yet.").font(.system(size: 13)).foregroundStyle(Color.mut).padding(.horizontal, 20).padding(.bottom, 18)
                    }
                    ForEach(Array(decided.enumerated()), id: \.offset) { n, x in
                        if n > 0 { Divider().overlay(Color.line) }
                        DecisionRow(x: x)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .panel(radius: 18)

                VStack(alignment: .leading, spacing: 10) {
                    HStack(spacing: 16) {
                        ZStack {
                            SegRing(segs: [(Double(doneN), .accent), (Double(checkN), .open), (Double(max(steps.count - doneN - checkN, 0)), .clear)])
                            Text("\(steps.isEmpty ? 0 : Int((Double(doneN + checkN) / Double(steps.count) * 100).rounded()))%")
                                .font(.system(size: 17, weight: .semibold)).monospacedDigit()
                        }
                        .help("\(doneN) confirmed, \(checkN) built and waiting for your OK, \(steps.count - doneN - checkN) open")
                        VStack(alignment: .leading, spacing: 0) {
                            (Text("\(doneN + checkN)") + Text("/\(steps.count)").font(.system(size: 18)).foregroundStyle(Color.mut))
                                .font(.system(size: 26, weight: .semibold)).monospacedDigit()
                            Text("steps built").font(.system(size: 13)).foregroundStyle(Color.mut)
                        }
                    }
                    .padding(18).frame(maxWidth: .infinity, alignment: .leading).panel(radius: 18)

                    Text("Since yesterday").font(.system(size: 12.5)).foregroundStyle(Color.mut).padding(.horizontal, 4).padding(.top, 4)
                    if since.isEmpty {
                        Text("Quiet").font(.system(size: 14)).foregroundStyle(Color.soft)
                            .padding(.horizontal, 16).padding(.vertical, 13).frame(maxWidth: .infinity, alignment: .leading).panel(radius: 16)
                    }
                    ForEach(since, id: \.label) { x in
                        HStack(spacing: 12) {
                            Image(systemName: x.icon).font(.system(size: 14, weight: .semibold)).foregroundStyle(x.color)
                                .frame(width: 34, height: 34).background(x.color.opacity(0.14), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                            Text("\(x.n) \(x.label)").font(.system(size: 15, weight: .semibold))
                        }
                        .padding(.horizontal, 16).padding(.vertical, 11).frame(maxWidth: .infinity, alignment: .leading).panel(radius: 16)
                    }
                }
                .frame(width: 290)
            }

            VStack(alignment: .leading, spacing: 12) {
                Text("Up next").font(.system(size: 17, weight: .semibold))
                if todo.isEmpty {
                    Text("No open steps.").font(.system(size: 13.5)).foregroundStyle(Color.mut)
                } else {
                    HStack(alignment: .top, spacing: 12) {
                        ForEach(Array(todo.prefix(3).enumerated()), id: \.offset) { n, s in
                            VStack(alignment: .leading, spacing: 10) {
                                Circle().fill(n == 0 ? Color.accent : Color(hex: 0x3f3f46)).frame(width: 8, height: 8)
                                    .shadow(color: n == 0 ? Color.accent : .clear, radius: 5)
                                Text(s["title"].str).font(.system(size: 15, weight: .semibold)).lineLimit(3)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                            .padding(.horizontal, 16).padding(.vertical, 14)
                            .frame(maxWidth: .infinity, minHeight: 108, alignment: .topLeading)
                            .panel(radius: 16, border: n == 0 ? Color(hex: 0x2b3a33) : .line)
                        }
                        ForEach(0..<max(0, 3 - todo.count), id: \.self) { _ in Color.clear.frame(maxWidth: .infinity) }
                    }
                }
            }

            HStack(spacing: 4) {
                ForEach(Tab.allCases, id: \.self) { t in
                    Button { withAnimation(.easeOut(duration: 0.15)) { tab = t } } label: {
                        Text(t.rawValue)
                            .font(.system(size: 13, weight: tab == t ? .medium : .regular))
                            .foregroundStyle(tab == t ? Color.ink : Color.mut)
                            .padding(.horizontal, 14).frame(height: 30)
                            .background(tab == t ? Color.white.opacity(0.1) : .clear, in: Capsule())
                            .contentShape(Capsule())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(4)
            .background(Color.white.opacity(0.03), in: Capsule())
            .overlay(Capsule().strokeBorder(Color.line2, lineWidth: 1))

            switch tab {
            case .steps:
                VStack(alignment: .leading, spacing: 0) {
                    HStack {
                        Text("Steps").font(.system(size: 15, weight: .semibold))
                        Spacer()
                        Text("\(doneN) of \(steps.count) done").font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.mut)
                    }
                    .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 8)
                    if openSteps.isEmpty {
                        Text("No open steps.").font(.system(size: 13)).foregroundStyle(Color.mut).padding(.horizontal, 20).padding(.bottom, 16)
                    }
                    ForEach(openSteps.prefix(shown)) { s in
                        Divider().overlay(Color.line)
                        StepRow(s: s, color: c)
                    }
                    if openSteps.count > shown {
                        Divider().overlay(Color.line)
                        Button("Show \(min(10, openSteps.count - shown)) more") { shown += 10 }
                            .buttonStyle(BtnStyle(kind: .ghost)).padding(.horizontal, 12).padding(.vertical, 8)
                    }
                    if !doneSteps.isEmpty {
                        Divider().overlay(Color.line)
                        Button { withAnimation(.easeOut(duration: 0.15)) { showDone.toggle() } } label: {
                            HStack(spacing: 6) {
                                Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold)).rotationEffect(.degrees(showDone ? 90 : 0))
                                Text("Done")
                                Text("\(doneSteps.count)").monospacedDigit().foregroundStyle(Color.mut)
                            }
                            .font(.system(size: 13.5)).foregroundStyle(Color.soft)
                            .padding(.horizontal, 20).padding(.vertical, 12).contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        if showDone {
                            ForEach(doneSteps) { s in
                                Divider().overlay(Color.line)
                                StepRow(s: s, color: c)
                            }
                        }
                    }
                }
                .panel(radius: 18)
            case .journey:
                if let story {
                    JourneyView(s: story, color: c)
                    MilestonesView(s: story, color: c)
                }
            case .decisions:
                if let story { SettledView(s: story) }
            case .made:
                if let story { MadeView(s: story) }
            case .artifacts:
                ArtifactsView(artifacts: d["artifacts"].arr)
            case .sources:
                SourcesView(sources: d["sources"].arr)
            }
        }
        .padding(.horizontal, 32).padding(.top, 30).padding(.bottom, 44)
        .frame(maxWidth: 1080)
        .frame(maxWidth: .infinity)
    }

    private struct Since { let n: Int; let label: String; let icon: String; let color: Color }

    private func sinceYesterday(_ d: J, steps: [J], items: [J]) -> [Since] {
        let cut = Date.now.addingTimeInterval(-86400)
        let recent = { (t: String?) in (parseTS(t) ?? .distantPast) > cut }
        let a = steps.filter { $0["state"].str == "done" && recent($0["updated"].s) }.count
        let b = d["sources"].arr.filter { $0["kind"].str == "session" && recent($0["last_ts"].s) }.count
        let c = items.filter { $0["type"].str == "decision" && $0["state"].str != "rejected" && recent($0["source_ts"].s) }.count
        return [
            Since(n: a, label: a == 1 ? "step finished" : "steps finished", icon: "checkmark", color: .accent),
            Since(n: b, label: b == 1 ? "session" : "sessions", icon: "bubble.left.and.bubble.right", color: .purple),
            Since(n: c, label: c == 1 ? "decision noted" : "decisions noted", icon: "lightbulb", color: .moving),
        ].filter { $0.n > 0 }
    }
}

struct DecisionRow: View {
    @EnvironmentObject var store: Store
    let x: J
    @State private var hover = false

    var body: some View {
        Button {
            let sid = x["source_id"].int
            if sid > 0 { store.go(.source(sid, chunk: x["chunk_id"].int > 0 ? x["chunk_id"].int : nil)) }
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: 14) {
                Text(x["day"].str == "undated" ? "undated" : dayFmt(x["day"].s))
                    .font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.mut).frame(width: 52, alignment: .leading)
                Text(x["title"].str).font(.system(size: 14)).frame(maxWidth: .infinity, alignment: .leading)
                    .fixedSize(horizontal: false, vertical: true)
                let how = x["how"].str
                Chip(text: HOW_NAME[how] ?? how, color: HOW_COLOR[how] ?? .mut)
            }
            .padding(.horizontal, 20).padding(.vertical, 12)
            .background(hover ? Color.white.opacity(0.025) : .clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .help("Open the source")
    }
}

struct StepRow: View {
    @EnvironmentObject var store: Store
    @Environment(\.openURL) private var openURL
    let s: J
    let color: Color
    @State private var busy = false

    var body: some View {
        let state = s["state"].str
        let done = state == "done"
        HStack(alignment: .top, spacing: 14) {
            Button { act("/api/tick") } label: {
                ZStack {
                    Circle().strokeBorder(done ? color : Color(hex: 0x3a3a42), lineWidth: 1.5)
                        .background(Circle().fill(done ? color : .clear))
                    if done { Image(systemName: "checkmark").font(.system(size: 9, weight: .heavy)).foregroundStyle(Color(hex: 0x04130c)) }
                }
                .frame(width: 20, height: 20).contentShape(Circle())
            }
            .buttonStyle(.plain).padding(.top, 1).help(done ? "Mark not done" : "Mark done")

            VStack(alignment: .leading, spacing: 5) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    if s["tag"].str == "bug" { Image(systemName: "ladybug").font(.system(size: 12)).foregroundStyle(Color.dropped) }
                    Text(s["title"].str)
                        .font(.system(size: 15, weight: done ? .regular : .medium))
                        .foregroundStyle(done ? Color.mut : Color.ink)
                        .strikethrough(done, color: .mut)
                        .fixedSize(horizontal: false, vertical: true)
                    if let st = STEP_STATUS[state] { Chip(text: st.0, color: st.1).help(st.2) }
                }
                if state == "looks_done", let a = s["answer"].s {
                    (Text("Claude says it built: ").foregroundStyle(Color.mut) + Text(a).foregroundStyle(Color.soft))
                        .font(.system(size: 13)).fixedSize(horizontal: false, vertical: true)
                }
                if !done, let b = s["body"].s, !b.isEmpty {
                    Text(b).font(.system(size: 13)).foregroundStyle(Color.soft).lineLimit(3)
                }
                if s["source_id"].int > 0 {
                    Button { store.go(.source(s["source_id"].int, chunk: s["chunk_id"].int > 0 ? s["chunk_id"].int : nil)) } label: {
                        Label(s["source_title"].str, systemImage: "doc.text").font(.system(size: 12)).foregroundStyle(Color.mut)
                    }
                    .buttonStyle(.plain)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            if !done {
                HStack(spacing: 6) {
                    if state == "looks_done" {
                        Button("Not yet") { act("/api/notyet") }.buttonStyle(BtnStyle(kind: .ghost))
                    } else {
                        Button(state == "doing" ? "Back to to-do" : "Start") { act("/api/doing") }
                            .buttonStyle(BtnStyle(kind: .ghost))
                    }
                    Button("Done") { act("/api/tick") }.buttonStyle(BtnStyle())
                }
                .disabled(busy)
            }
        }
        .padding(.horizontal, 20).padding(.vertical, 13)
        .opacity(busy ? 0.5 : 1)
    }

    private func act(_ path: String) {
        busy = true
        Task {
            _ = try? await API.post(path, ["id": s.id])
            busy = false
            store.changed()
        }
    }
}
