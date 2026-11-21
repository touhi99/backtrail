import SwiftUI

let ITEM_STATE_COLOR: [String: Color] = [
    "open": .open, "doing": .moving, "done": .accent, "looks_done": .accent, "settled": .purple,
    "answered": .accent, "proposed": .mut, "rejected": .dropped,
]
let SOURCE_KIND_NAME: [String: String] = ["session": "Conversation", "note": "Note", "artifact": "Artifact", "commit": "Commits", "owner": "Typed in the app"]
let SOURCE_KIND_COLOR: [String: Color] = ["session": .moving, "note": .mut, "artifact": .purple, "commit": .accent, "owner": .ink]

func marked(_ html: String) -> AttributedString {
    var out = AttributedString()
    var rest = Substring(html)
    while let open = rest.range(of: "<mark>") {
        out += AttributedString(String(rest[..<open.lowerBound]))
        rest = rest[open.upperBound...]
        let close = rest.range(of: "</mark>") ?? (rest.endIndex..<rest.endIndex)
        var hit = AttributedString(String(rest[..<close.lowerBound]))
        hit.foregroundColor = .ink
        hit.backgroundColor = Color.open.opacity(0.3)
        out += hit
        rest = rest[close.upperBound...]
    }
    out += AttributedString(String(rest))
    return out
}

struct SearchView: View {
    @EnvironmentObject var store: Store
    let initial: String
    var scope = ""
    @State private var text = ""
    @State private var r: J?
    @State private var loading = false
    @State private var pending = ""
    @State private var thread = ""
    @State private var kind = ""
    @State private var keywords = false
    @State private var shown = 10
    @FocusState private var focused: Bool

    private static let kinds = [("", "Any source"), ("session", "Conversations"), ("note", "Notes"), ("artifact", "Artifacts"), ("commit", "Commits")]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 12) {
                    Image(systemName: "magnifyingglass").font(.system(size: 16)).foregroundStyle(Color.mut)
                    TextField("Search notes, artifacts, conversations and commits", text: $text)
                        .textFieldStyle(.plain).font(.system(size: 16)).focused($focused)
                        .onSubmit { run() }
                    if loading { ProgressView().controlSize(.small) }
                }
                .padding(.horizontal, 18).frame(height: 54)
                .background(Color.black.opacity(0.35), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .strokeBorder(focused ? Color.accent.opacity(0.6) : Color.line2, lineWidth: 1))

                HStack(spacing: 10) {
                    Picker("", selection: $thread) {
                        Text("All threads").tag("")
                        ForEach(store.threads.map { $0["name"].str }, id: \.self) { Text($0).tag($0) }
                    }
                    .labelsHidden().fixedSize()
                    Picker("", selection: $kind) {
                        ForEach(Self.kinds, id: \.0) { Text($0.1).tag($0.0) }
                    }
                    .labelsHidden().fixedSize()
                    Toggle("Keywords only", isOn: $keywords).toggleStyle(.checkbox).font(.system(size: 12.5)).foregroundStyle(Color.mut)
                    Spacer()
                    if let r {
                        Text(r["mode"].str == "hybrid" ? "Meaning and keyword match" : "Keyword match")
                            .font(.system(size: 12)).foregroundStyle(Color.mut)
                    }
                }
                .onChange(of: thread) { run() }
                .onChange(of: kind) { run() }
                .onChange(of: keywords) { run() }

                if let r { results(r) } else if !loading {
                    Text("Type a word or a question.").font(.system(size: 13.5)).foregroundStyle(Color.mut).padding(.top, 4)
                }
            }
            .padding(.horizontal, 32).padding(.top, 30).padding(.bottom, 44)
            .frame(maxWidth: 1080)
            .frame(maxWidth: .infinity)
        }
        .background(PageGlow())
        .onAppear {
            text = initial
            thread = scope
            focused = initial.isEmpty
            if !initial.isEmpty { run() }
        }
    }

    @ViewBuilder private func results(_ r: J) -> some View {
        let items = r["items"].arr
        let passages = r["results"].arr
        if !items.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                Text("Filed items").font(.system(size: 15, weight: .semibold))
                    .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 8)
                ForEach(items) { i in
                    Divider().overlay(Color.line)
                    RowButton { store.go(.thread(i["thread"].str)) } label: {
                        VStack(alignment: .leading, spacing: 4) {
                            HStack(spacing: 8) {
                                Chip(text: i["type"].str, color: .soft)
                                Text(i["thread"].str)
                                Spacer()
                                Circle().fill(ITEM_STATE_COLOR[i["state"].str] ?? .mut).frame(width: 7, height: 7)
                                Text(i["state"].str.replacingOccurrences(of: "_", with: " "))
                            }
                            .font(.system(size: 12)).foregroundStyle(Color.mut)
                            Text(i["title"].str).font(.system(size: 14, weight: .medium))
                        }
                    }
                }
            }
            .panel(radius: 18)
        }
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text("Passages").font(.system(size: 15, weight: .semibold))
                Text("\(passages.count) best matches").font(.system(size: 12)).foregroundStyle(Color.mut)
            }
            .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 8)
            if passages.isEmpty {
                Text("No match.").font(.system(size: 13)).foregroundStyle(Color.mut).padding(.horizontal, 20).padding(.bottom, 16)
            }
            ForEach(passages.prefix(shown), id: \.["chunk_id"].int) { x in
                Divider().overlay(Color.line)
                RowButton { store.go(.source(x["source_id"].int, chunk: x["chunk_id"].int)) } label: { passage(x) }
            }
            if passages.count > shown {
                Divider().overlay(Color.line)
                Button("Show \(min(10, passages.count - shown)) more") { shown += 10 }
                    .buttonStyle(BtnStyle(kind: .ghost)).padding(.horizontal, 12).padding(.vertical, 8)
            }
        }
        .panel(radius: 18)
    }

    private func passage(_ x: J) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 8) {
                Chip(text: SOURCE_KIND_NAME[x["kind"].str] ?? x["kind"].str, color: SOURCE_KIND_COLOR[x["kind"].str] ?? .mut)
                if let t = x["thread"].s { Text(t) }
                Spacer()
                Text(dayFmt(x["last_ts"].s))
            }
            .font(.system(size: 12)).foregroundStyle(Color.mut)
            Text(x["title"].str).font(.system(size: 14, weight: .medium))
            Text(marked(x["snip"].str)).font(.system(size: 13)).foregroundStyle(Color.soft).lineLimit(2)
            let linked = Array(x["items"].arr.prefix(2))
            if !linked.isEmpty {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(linked) { i in
                        HStack(spacing: 6) {
                            Circle().fill(i["dead"].bool ? Color.dropped : ITEM_STATE_COLOR[i["state"].str] ?? .mut).frame(width: 6, height: 6)
                            Text("\(i["type"].str): \(i["title"].str)").lineLimit(1)
                        }
                    }
                }
                .font(.system(size: 11.5)).foregroundStyle(Color.mut).padding(.top, 2)
            }
        }
    }

    private func run() {
        let q = text.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { r = nil; return }
        var path = "/api/search?q=\(API.q(q))"
        if !thread.isEmpty { path += "&thread=\(API.q(thread))" }
        if !kind.isEmpty { path += "&kind=\(kind)" }
        if keywords { path += "&mode=text" }
        loading = true
        shown = 10
        let started = path
        pending = started
        if !keywords {
            Task {
                let quick = try? await API.get(path + "&mode=text")
                if pending == started, loading, let quick { r = quick }
            }
        }
        Task {
            let full = try? await API.get(path)
            guard pending == started else { return }
            if let full { r = full }
            loading = false
        }
    }
}

struct RowButton<L: View>: View {
    var action: () -> Void
    @ViewBuilder var label: () -> L
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            label()
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 20).padding(.vertical, 12)
                .background(hover ? Color.white.opacity(0.025) : .clear)
                .contentShape(Rectangle())
        }
        .buttonStyle(PressFade())
        .onHover { hover = $0 }
    }
}

struct PressFade: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.background(configuration.isPressed ? Color.white.opacity(0.04) : .clear)
    }
}
