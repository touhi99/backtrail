import SwiftUI

let STEP_STATUS: [String: (String, Color, String)] = [
    "doing": ("In progress", .moving, "You marked it in progress. Claude does not set this."),
    "looks_done": ("Built, check it", .accent, "Claude or a commit says it is built. Confirm it works, or say not yet."),
]

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
    @State private var showRejected = false

    enum Tab: String, CaseIterable { case steps = "Steps", journey = "Journey", decisions = "Decisions", made = "Made", artifacts = "Artifacts", sources = "Sources", archive = "Archive", agent = "Coding agent" }

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
            .sorted { a, b in
                let da = a["state"].str == "doing" ? 0 : 1, db = b["state"].str == "doing" ? 0 : 1
                return da != db ? da < db : a["created"].str > b["created"].str
            }
        let gains = lastGains(items)
        let inCard = Set(gains.shown.map(\.id))
        let openSteps = steps.filter { $0["state"].str != "done" }
            .sorted { ($0["state"].str == "looks_done" ? 0 : $0["state"].str == "doing" ? 1 : 2) < ($1["state"].str == "looks_done" ? 0 : $1["state"].str == "doing" ? 1 : 2) }
        let doneSteps = steps.filter { $0["state"].str == "done" }.sorted { $0["updated"].str > $1["updated"].str }

        return VStack(alignment: .leading, spacing: 26) {
            HStack(spacing: 14) {
                Glyph(name: th["name"].str, color: c, size: 44).shadow(color: c.opacity(0.25), radius: 14)
                VStack(alignment: .leading, spacing: 2) {
                    Text(th["name"].str).font(.system(size: 26, weight: .semibold)).tracking(-0.4)
                    if let s = th["summary"].s { Text(s).font(.system(size: 13.5)).foregroundStyle(Color.mut).lineLimit(1) }
                    if let p = th["parent"].s, !p.isEmpty {
                        Button { store.go(.thread(p)) } label: {
                            Text("Sub-thread of \(p)").font(.system(size: 12.5)).foregroundStyle(Color.mut)
                        }
                        .buttonStyle(.plain)
                    }
                }
                Spacer()
                Button { store.go(.decisions(name)) } label: { Label("Decisions", systemImage: "point.3.connected.trianglepath.dotted") }
                    .buttonStyle(BtnStyle(kind: .ghost))
                Button { store.go(.map(name)) } label: { Text("Map") }
                    .buttonStyle(BtnStyle(kind: .ghost))
                Button { store.capture = .some(name) } label: { Label("Add", systemImage: "plus") }
                    .buttonStyle(BtnStyle())
            }

            if !d["children"].arr.isEmpty {
                ScrollView(.horizontal) { HStack(spacing: 8) {
                    Text("Sub-threads").font(.system(size: 12)).foregroundStyle(Color.mut)
                    ForEach(d["children"].arr, id: \.["name"].str) { k in
                        Button { store.go(.thread(k["name"].str)) } label: {
                            HStack(spacing: 6) {
                                Circle().fill(STATE_COLOR[k["state"].str] ?? .mut).frame(width: 7, height: 7)
                                Text(k["name"].str)
                                Text("\(k["open_s"].int) open · \(k["done_s"].int) done").foregroundStyle(Color.mut)
                            }
                        }
                        .buttonStyle(BtnStyle(kind: .ghost))
                    }
                } }
                .scrollIndicators(.never)
            }

            LeftOffCard(sources: d["sources"].arr, gains: gains, next: todo.first) { s in
                if let sid = Optional(s["source_id"].int), sid > 0 { store.go(.source(sid, chunk: s["chunk_id"].int > 0 ? s["chunk_id"].int : nil)) }
            }

            let prs = d["prs"].arr
            if !prs.isEmpty {
                HStack(spacing: 8) {
                    Text("Pull requests").font(.system(size: 12.5)).foregroundStyle(Color.mut)
                    ForEach(Array(prs.prefix(6).enumerated()), id: \.offset) { _, p in
                        let repo = p["repo"].str
                        let short = repo.split(separator: "/").last.map(String.init) ?? repo
                        let look = prLook(p["state"].str)
                        Button { if let u = URL(string: p["url"].str) { openURL(u) } } label: {
                            HStack(spacing: 5) {
                                Image(systemName: look.icon).foregroundStyle(look.color)
                                Text("\(short)#\(p["number"].int)")
                                if p["state"].str != "MERGED" {
                                    Text(look.label).font(.system(size: 11)).foregroundStyle(look.color)
                                }
                            }
                        }
                        .buttonStyle(BtnStyle(kind: .ghost))
                        .help("\(p["title"].str.isEmpty ? repo : p["title"].str) · \(look.label) \(p["merged_on"].str)")
                    }
                    if prs.count > 6 {
                        Text("+\(prs.count - 6) more").font(.system(size: 12)).foregroundStyle(Color.mut)
                    }
                }
            }

            HStack(alignment: .top, spacing: 20) {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Earlier decisions").font(.system(size: 15, weight: .semibold))
                        .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 8)
                    let decided = d["decided"].arr.filter { !inCard.contains($0["id"].str) }
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
                                HStack(spacing: 8) {
                                    Circle().fill(n == 0 ? Color.accent : Color(hex: 0x3f3f46)).frame(width: 8, height: 8)
                                        .shadow(color: n == 0 ? Color.accent : .clear, radius: 5)
                                    if let sub = s["sub"].s { Text(sub).font(.system(size: 12)).foregroundStyle(Color.mut) }
                                }
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
                ForEach(items.filter { $0["type"].str == "question" && $0["state"].str == "open" && $0["origin"].str == "owner" }) { q in
                    HStack(alignment: .top, spacing: 12) {
                        Image(systemName: "questionmark.circle").font(.system(size: 17)).foregroundStyle(Color.soft)
                        VStack(alignment: .leading, spacing: 4) {
                            HStack(spacing: 8) { WhoChip(i: q); Text("your question · \(dayFmt(q["created"].s))") }
                                .font(.system(size: 12)).foregroundStyle(Color.mut)
                            Text(q["title"].str).font(.system(size: 16, weight: .semibold)).fixedSize(horizontal: false, vertical: true)
                            if let b = q["body"].s, !b.isEmpty { Text(b).font(.system(size: 13)).foregroundStyle(Color.soft) }
                        }
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 20).padding(.vertical, 16).panel(radius: 18)
                }
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
            case .archive:
                archive(items)
            case .agent:
                agentView(d["agent"])
            }
        }
        .padding(.horizontal, 32).padding(.top, 30).padding(.bottom, 44)
        .frame(maxWidth: 1080)
        .frame(maxWidth: .infinity)
    }

    private func itemTime(_ i: J) -> String { i["source_ts"].s ?? i["created"].str }

    @ViewBuilder private func archive(_ items: [J]) -> some View {
        let prop = items.filter { $0["state"].str == "proposed" }.sorted { itemTime($0) > itemTime($1) }
        let rej = items.filter { ["rejected", "parked"].contains($0["state"].str) || ($0["state"].str == "superseded" && $0["type"].str != "decision") }
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Unconfirmed suggestions").font(.system(size: 15, weight: .semibold))
                Spacer()
                Text("\(prop.count)").font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.mut)
            }
            .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 8)
            if prop.isEmpty { Text("Nothing here.").font(.system(size: 13)).foregroundStyle(Color.mut).padding(.horizontal, 20).padding(.bottom, 16) }
            ForEach(prop) { i in
                Divider().overlay(Color.line)
                HStack(alignment: .center, spacing: 12) {
                    Image(systemName: "sparkles").font(.system(size: 14)).foregroundStyle(Color.moving)
                        .frame(width: 32, height: 32).background(Color.moving.opacity(0.12), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(i["title"].str).font(.system(size: 14, weight: .medium)).fixedSize(horizontal: false, vertical: true)
                        HStack(spacing: 4) {
                            Text(dayFmt(itemTime(i)))
                            if let st = staleOf(i) { Text("·"); Text(st.label).foregroundStyle(Color.open).help(st.reason) }
                        }
                        .font(.system(size: 12)).foregroundStyle(Color.mut)
                        if let b = i["body"].s, !b.isEmpty { Text(b).font(.system(size: 12.5)).foregroundStyle(Color.mut).lineLimit(3) }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    ItemActions(i: i)
                }
                .padding(.horizontal, 20).padding(.vertical, 12)
            }
        }
        .panel(radius: 18)
        if !rej.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                Button { withAnimation(.easeOut(duration: 0.15)) { showRejected.toggle() } } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold)).rotationEffect(.degrees(showRejected ? 90 : 0))
                        Text("Rejected or not valid now")
                        Text("\(rej.count)").monospacedDigit().foregroundStyle(Color.mut)
                    }
                    .font(.system(size: 13.5)).foregroundStyle(Color.soft)
                    .padding(.horizontal, 20).padding(.vertical, 12).contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                if showRejected {
                    ForEach(rej) { i in
                        Divider().overlay(Color.line)
                        VStack(alignment: .leading, spacing: 3) {
                            Text(i["title"].str).font(.system(size: 14)).foregroundStyle(Color.mut).strikethrough(color: .mut)
                            (Text(i["state"].str == "parked" ? "Why not valid: " : "Why rejected: ") + Text(i["answer"].s.flatMap { $0.isEmpty ? nil : $0 } ?? "No reason given."))
                                .font(.system(size: 12.5)).foregroundStyle(Color.mut)
                        }
                        .padding(.horizontal, 20).padding(.vertical, 10)
                    }
                }
            }
            .panel(radius: 18)
        }
    }

    private func agentView(_ agent: J) -> some View {
        let cmd = agent["start"].str
        return VStack(alignment: .leading, spacing: 10) {
            Text("Work on it in \(agent["name"].str)").font(.system(size: 15, weight: .semibold))
            HStack(spacing: 8) {
                Text(cmd).font(.system(size: 12, design: .monospaced)).foregroundStyle(Color.soft).lineLimit(1).truncationMode(.middle)
                    .textSelection(.enabled)
                    .padding(.horizontal, 14).frame(maxWidth: .infinity, minHeight: 36, alignment: .leading)
                    .background(Color(hex: 0x0c0c0e), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color.line, lineWidth: 1))
                Button { copy(cmd) } label: { Label("Copy", systemImage: "doc.on.doc") }.buttonStyle(BtnStyle())
                ForEach(["Review", "Extend", "Explore"], id: \.self) { v in
                    Button(v) { copy("\(v) the Backtrail thread \"\(name)\". Use backtrail brief, search and show; file proposed items with a source and a verbatim quote (skill/SKILL.md).") }
                        .buttonStyle(BtnStyle())
                }
            }
        }
        .padding(20).panel(radius: 18)
    }

    private func copy(_ t: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(t, forType: .string)
    }
}

let STALE_LABEL = ["in_effect": "Already in effect", "superseded": "Replaced later", "done": "Already done"]

func staleOf(_ i: J) -> (label: String, reason: String)? {
    let parts = (i["stale"].s ?? "").split(separator: "|", omittingEmptySubsequences: false).map(String.init)
    guard let k = parts.first, let l = STALE_LABEL[k] else { return nil }
    return (l, parts.dropFirst().joined(separator: "|"))
}

func alsoText(_ items: [J]) -> String {
    items.compactMap { a -> String? in
        let t = a["title"].str
        guard !t.isEmpty else { return nil }
        guard let note = [a["reason"].s, a["body"].s].compactMap({ $0 }).first(where: { !$0.isEmpty }) else { return t }
        return "\(t) (\(note))"
    }.joined(separator: ", ")
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
                VStack(alignment: .leading, spacing: 3) {
                    if let sub = x["sub"].s { Text(sub).font(.system(size: 12)).foregroundStyle(Color.mut) }
                    Text(x["title"].str).font(.system(size: 14))
                        .fixedSize(horizontal: false, vertical: true)
                    let also = x["also"].arr
                    if !also.isEmpty {
                        Text("Also considered: " + alsoText(also))
                            .font(.system(size: 12.5)).foregroundStyle(Color.mut).lineLimit(2)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                let how = x["how"].str
                Circle().fill(HOW_COLOR[how] ?? .mut).frame(width: 8, height: 8).help(HOW_NAME[how] ?? how)
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
    @State private var ask: ReasonAsk?

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
                    if let sub = s["sub"].s { Chip(text: sub, color: .mut) }
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
                let blocked = s["blocked_by"].arr
                if !blocked.isEmpty {
                    Text("Blocked by " + blocked.map { $0["title"].str }.joined(separator: ", "))
                        .font(.system(size: 12.5)).foregroundStyle(Color.mut).lineLimit(2)
                }
                if s["source_id"].int > 0 {
                    Button { store.go(.source(s["source_id"].int, chunk: s["chunk_id"].int > 0 ? s["chunk_id"].int : nil)) } label: {
                        Label(s["source_title"].str, systemImage: "doc.text").font(.system(size: 12)).foregroundStyle(Color.mut)
                    }
                    .buttonStyle(.plain)
                }
                if s["origin"].str == "owner" && !done { ItemActions(i: s).padding(.top, 2) }
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
                    Button("Ignore") {
                        ask = ReasonAsk(title: "Why ignore it?") { act("/api/review", ["accept": false, "park": true, "reason": $0]) }
                    }
                    .buttonStyle(BtnStyle(kind: .ghost)).help("Not a problem or not needed any more. Kept on file, out of the list.")
                }
                .disabled(busy)
            }
        }
        .padding(.horizontal, 20).padding(.vertical, 13)
        .opacity(busy ? 0.5 : 1)
        .reasonPrompt($ask)
    }

    private func act(_ path: String, _ extra: [String: Any] = [:]) {
        busy = true
        Task {
            _ = try? await API.post(path, extra.merging(["id": s.id]) { a, _ in a })
            busy = false
            store.changed()
        }
    }
}

struct Gains {
    let day: String?
    let shown: [J]
}

private func outcome(_ i: J) -> String? {
    let t = i["type"].str, st = i["state"].str
    if t == "decision" && st == "settled" { return "decided" }
    if t == "step" && (st == "done" || st == "looks_done") { return "finished" }
    return nil
}

private func gainedAt(_ i: J) -> String {
    let t = outcome(i) == "finished" ? i["updated"].str : i["source_ts"].str
    return t.isEmpty ? i["created"].str : t
}

func lastGains(_ items: [J]) -> Gains {
    let gained = items.filter { outcome($0) != nil }
    let day = gained.map { String(gainedAt($0).prefix(10)) }.max()
    let shown = gained.filter { String(gainedAt($0).prefix(10)) == day }
        .sorted { a, b in
            let fa = outcome(a) == "finished" ? 0 : 1, fb = outcome(b) == "finished" ? 0 : 1
            return fa != fb ? fa < fb : gainedAt(a) > gainedAt(b)
        }
    return Gains(day: day, shown: Array(shown.prefix(3)))
}

private func agoText(_ t: String) -> String {
    guard let d = parseTS(t) ?? {
        let f = DateFormatter(); f.dateFormat = "yyyy-MM-dd"; return f.date(from: String(t.prefix(10)))
    }() else { return "" }
    let n = Calendar.current.dateComponents([.day], from: Calendar.current.startOfDay(for: d), to: Calendar.current.startOfDay(for: Date())).day ?? 0
    return n <= 0 ? "Today" : n == 1 ? "Yesterday" : "\(n) days ago"
}

struct LeftOffCard: View {
    @EnvironmentObject var store: Store
    let sources: [J]
    let gains: Gains
    let next: J?
    let open: (J) -> Void

    var body: some View {
        let last = sources.filter { $0["kind"].str == "session" }.max { $0["last_ts"].str < $1["last_ts"].str }
        if let last {
            HStack(alignment: .center, spacing: 0) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Where you left off").font(.system(size: 12.5)).foregroundStyle(Color.mut)
                    Text(agoText(last["last_ts"].str)).font(.system(size: 26, weight: .semibold)).tracking(-0.4)
                    Text(last["title"].str).font(.system(size: 13)).foregroundStyle(Color.soft).lineLimit(2)
                }
                .padding(.horizontal, 22).padding(.vertical, 18).frame(width: 220, alignment: .leading)
                Divider().overlay(Color.line)
                VStack(alignment: .leading, spacing: 10) {
                    Text(gains.day.map { "What moved forward on \(dayFmt($0))" } ?? "Nothing decided or finished yet")
                        .font(.system(size: 12.5)).foregroundStyle(Color.mut)
                    ForEach(gains.shown, id: \.id) { i in
                        let finished = outcome(i) == "finished"
                        let c: Color = finished ? .accent : .moving
                        Button { open(i) } label: {
                            HStack(alignment: .firstTextBaseline, spacing: 10) {
                                Label(finished ? "Finished" : "Decided", systemImage: finished ? "checkmark" : "lightbulb")
                                    .font(.system(size: 11.5, weight: .medium)).padding(.horizontal, 8).padding(.vertical, 2)
                                    .background(c.opacity(0.14), in: Capsule()).foregroundStyle(c)
                                Text(i["title"].str).font(.system(size: 15, weight: .medium)).lineLimit(2)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 22).padding(.vertical, 18).frame(maxWidth: .infinity, alignment: .leading)
                Divider().overlay(Color.line)
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 6) {
                        Circle().fill(next == nil ? Color.line2 : Color.accent).frame(width: 8, height: 8).shadow(color: next == nil ? .clear : .accent, radius: 5)
                        Text("Next").font(.system(size: 12.5)).foregroundStyle(Color.mut)
                    }
                    Text(next?["title"].str ?? "No open step").font(.system(size: 16, weight: .semibold)).lineLimit(3)
                }
                .padding(.horizontal, 22).padding(.vertical, 18).frame(width: 270, alignment: .leading)
            }
            .background(LinearGradient(colors: [Color.accent.opacity(0.07), .clear], startPoint: .topLeading, endPoint: .center))
            .panel(radius: 18)
        }
    }
}

func prLook(_ state: String) -> (icon: String, color: Color, label: String) {
    switch state {
    case "MERGED": return ("arrow.triangle.merge", .accent, "merged")
    case "OPEN": return ("arrow.triangle.pull", .open, "open")
    case "CLOSED": return ("xmark.circle", .dropped, "closed unmerged")
    default: return ("questionmark.circle", .mut, "unchecked")
    }
}
