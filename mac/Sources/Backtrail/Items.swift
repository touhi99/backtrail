import SwiftUI

private let OPT: [String: [(String, String)]] = [
    "type": [("", "All types"), ("decision", "Decisions"), ("step", "Steps"), ("question", "Questions"), ("turn", "Turns")],
    "state": [("", "Any state"), ("open", "To do"), ("doing", "In progress"), ("done", "Done"), ("looks_done", "Built, check it"), ("settled", "Settled"),
              ("answered", "Answered"), ("proposed", "Suggested"), ("rejected", "Rejected"), ("parked", "Not valid now")],
    "kind": [("", "Any kind"), ("idea", "Idea"), ("plan", "Plan"), ("build", "Build"), ("study", "Study"), ("discussion", "Discussion")],
]
private let TYPE_ICON: [String: (String, Color)] = [
    "question": ("questionmark.circle", .open), "step": ("checklist", .moving), "decision": ("scale.3d", .purple), "turn": ("sparkles", .mut),
]
private let ITEM_HELP: [String: String] = [
    "": "Every decision, step and question filed across threads. The tag shows who filed it, You or Claude. Filter to narrow it; open a title for its thread, or act on a row at the right.",
    "decision": "Choices made in sessions. A suggested decision is accepted to keep it or rejected to drop it. A kept decision stays standing until marked replaced.",
    "step": "Todos and bugs. Open the thread to start, tick or confirm them.",
    "question": "Questions that wait for an answer. Answer them on Today or on the thread page.",
]
private let STATE_LABEL = ["proposed": "suggested", "open": "to do", "doing": "in progress", "looks_done": "built, check it", "parked": "not valid now"]
private let CLOSED: Set<String> = ["done", "answered", "settled", "rejected", "parked"]
private let ROW_STATE_COLOR: [String: Color] = [
    "open": .open, "doing": .moving, "done": .accent, "looks_done": .accent, "settled": .purple, "answered": .accent,
    "proposed": .mut, "rejected": .dropped, "parked": .mut,
]

struct ItemsPage: View {
    @EnvironmentObject var store: Store
    @State private var f = ["type": "", "state": "", "kind": "", "thread": ""]
    @State private var all: [J]?
    @State private var closed = false
    @State private var shown = 15

    var body: some View {
        let showClosed = closed || !(f["state"] ?? "").isEmpty
        let rows = (all ?? []).filter { showClosed || !CLOSED.contains($0["state"].str) }
        let nClosed = (all ?? []).filter { CLOSED.contains($0["state"].str) }.count
        let mix = Dictionary(grouping: rows, by: { $0["state"].str }).map { ($0.key, $0.value.count) }.sorted { $0.1 > $1.1 }
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack(alignment: .bottom) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Everything filed").font(.system(size: 13)).foregroundStyle(Color.mut)
                        Text("Items").font(.system(size: 30, weight: .semibold)).tracking(-0.5).help(ITEM_HELP[f["type"] ?? ""] ?? ITEM_HELP[""]!)
                    }
                    Spacer()
                    Text("\(rows.count)").font(.system(size: 34, weight: .semibold)).monospacedDigit()
                }
                HStack(spacing: 8) {
                    picker("type")
                    picker("state")
                    picker("kind")
                    Picker("", selection: bind("thread")) {
                        Text("All threads").tag("")
                        ForEach(store.threads.map { $0["name"].str }, id: \.self) { Text($0).tag($0) }
                    }
                    .labelsHidden().fixedSize()
                }
                if !mix.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        GeometryReader { g in
                            HStack(spacing: 3) {
                                ForEach(mix, id: \.0) { s, n in
                                    Capsule().fill(ROW_STATE_COLOR[s] ?? Color(hex: 0x52525b))
                                        .frame(width: max(2, (g.size.width - CGFloat(mix.count - 1) * 3) * CGFloat(n) / CGFloat(rows.count)))
                                        .help("\(s.replacingOccurrences(of: "_", with: " ")): \(n)")
                                }
                            }
                        }
                        .frame(height: 8).clipShape(Capsule())
                        FlowLayout(spacing: 14) {
                            ForEach(mix, id: \.0) { s, n in
                                HStack(spacing: 6) {
                                    Circle().fill(ROW_STATE_COLOR[s] ?? Color(hex: 0x52525b)).frame(width: 8, height: 8)
                                    Text(s.replacingOccurrences(of: "_", with: " "))
                                    Text("\(n)").monospacedDigit()
                                }
                                .font(.system(size: 12)).foregroundStyle(Color.mut)
                            }
                        }
                    }
                }
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(rows.prefix(shown).enumerated()), id: \.offset) { n, i in
                        if n > 0 { Divider().overlay(Color.line) }
                        row(i)
                    }
                    if rows.count > shown {
                        Divider().overlay(Color.line)
                        Button("Show \(min(15, rows.count - shown)) more") { shown += 15 }
                            .buttonStyle(BtnStyle(kind: .ghost)).padding(.horizontal, 12).padding(.vertical, 8)
                    }
                    if all != nil && !showClosed && nClosed > 0 {
                        Divider().overlay(Color.line)
                        footer("Show closed (\(nClosed)): done, answered, settled, rejected, not valid now") { closed = true }
                    }
                    if all != nil && closed && (f["state"] ?? "").isEmpty {
                        Divider().overlay(Color.line)
                        footer("Hide closed") { closed = false }
                    }
                    if all != nil && rows.isEmpty {
                        Text("No items match.").font(.system(size: 13.5)).foregroundStyle(Color.mut).padding(.horizontal, 20).padding(.vertical, 18)
                    }
                }
                .panel(radius: 18)
            }
            .padding(.horizontal, 32).padding(.top, 30).padding(.bottom, 44)
            .frame(maxWidth: 1080)
            .frame(maxWidth: .infinity)
        }
        .background(PageGlow())
        .task(id: "\(f)|\(store.tick)") {
            shown = 15
            let q = ["type", "state", "kind", "thread"].map { "\($0)=\(API.q(f[$0] ?? ""))" }.joined(separator: "&")
            if let r = try? await API.get("/api/items?\(q)") { all = r.arr }
        }
    }

    private func bind(_ k: String) -> Binding<String> {
        Binding(get: { f[k] ?? "" }, set: { f[k] = $0 })
    }

    private func picker(_ k: String) -> some View {
        Picker("", selection: bind(k)) {
            ForEach(OPT[k] ?? [], id: \.0) { v, l in Text(l).tag(v) }
        }
        .labelsHidden().fixedSize()
    }

    private func footer(_ label: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label).font(.system(size: 13)).foregroundStyle(Color.mut)
                .padding(.horizontal, 20).padding(.vertical, 12).frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func row(_ i: J) -> some View {
        let t = TYPE_ICON[i["tag"].str == "bug" ? "step" : i["type"].str] ?? TYPE_ICON["turn"]!
        let st = staleOf(i)
        let state = i["state"].str
        return HStack(spacing: 12) {
            Image(systemName: t.0).font(.system(size: 15)).foregroundStyle(t.1)
                .frame(width: 36, height: 36).background(t.1.opacity(0.12), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            Button { store.go(.thread(i["thread"].str)) } label: {
                Text(i["title"].str).font(.system(size: 14, weight: .medium)).lineLimit(1)
            }
            .buttonStyle(.plain)
            .help([i["body"].s, i["answer"].s].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · "))
            .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 8) {
                if let st { Chip(text: st.label, color: .open).fixedSize() }
                WhoChip(i: i).fixedSize()
                HStack(spacing: 5) {
                    Circle().fill(ROW_STATE_COLOR[state] ?? Color(hex: 0x52525b)).frame(width: 6, height: 6)
                    Text(STATE_LABEL[state] ?? state)
                }
                .padding(.horizontal, 8).frame(height: 22)
                .background(Color.white.opacity(0.04), in: Capsule())
                .overlay(Capsule().strokeBorder(Color.line2, lineWidth: 1))
                .fixedSize()
                Text(i["thread"].str).lineLimit(1).frame(width: 120, alignment: .trailing)
                Text(dayFmt(i["source_ts"].s ?? i["created"].s)).monospacedDigit().frame(width: 52, alignment: .trailing)
            }
            .font(.system(size: 12)).foregroundStyle(Color.mut)
            .layoutPriority(1)
            ItemActions(i: i).fixedSize().frame(minWidth: 150, alignment: .trailing).layoutPriority(2)
        }
        .padding(.horizontal, 18).padding(.vertical, 10)
        .opacity((i["stale"].s ?? "").hasPrefix("superseded") ? 0.55 : 1)
    }
}

struct LogPage: View {
    @EnvironmentObject var store: Store
    @State private var rows: [J] = []
    @State private var shown = 20

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Every change, newest first").font(.system(size: 13)).foregroundStyle(Color.mut)
                    Text("Log").font(.system(size: 30, weight: .semibold)).tracking(-0.5)
                }
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(rows.prefix(shown).enumerated()), id: \.offset) { n, x in
                        if n > 0 { Divider().overlay(Color.line) }
                        let you = x["actor"].str == "owner"
                        HStack(spacing: 12) {
                            Text(String(x["ts"].str.replacingOccurrences(of: "T", with: " ").dropFirst(5).prefix(11)))
                                .font(.system(size: 12.5)).monospacedDigit().foregroundStyle(Color.mut).frame(width: 92, alignment: .leading)
                            HStack(spacing: 5) {
                                Circle().fill(you ? Color.accent : Color.moving).frame(width: 6, height: 6)
                                Text(you ? "You" : x["actor"].str == "claude" ? "Claude" : x["actor"].str)
                            }
                            .font(.system(size: 12)).padding(.horizontal, 8).frame(height: 22)
                            .background(Color.white.opacity(0.04), in: Capsule())
                            .overlay(Capsule().strokeBorder(Color.line2, lineWidth: 1))
                            .frame(width: 84, alignment: .leading)
                            let detail = x["detail"].s.flatMap { $0.isEmpty ? nil : $0 } ?? x["target"].str
                            (Text(x["action"].str + " ") + Text(detail).foregroundColor(.soft))
                                .font(.system(size: 13.5)).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .padding(.horizontal, 18).padding(.vertical, 11)
                    }
                    if rows.count > shown {
                        Divider().overlay(Color.line)
                        Button("Show \(min(20, rows.count - shown)) more") { shown += 20 }
                            .buttonStyle(BtnStyle(kind: .ghost)).padding(.horizontal, 12).padding(.vertical, 8)
                    }
                }
                .panel(radius: 18)
            }
            .padding(.horizontal, 32).padding(.top, 30).padding(.bottom, 44)
            .frame(maxWidth: 1080)
            .frame(maxWidth: .infinity)
        }
        .background(PageGlow())
        .task(id: store.tick) { if let r = try? await API.get("/api/log") { rows = r.arr } }
    }
}
