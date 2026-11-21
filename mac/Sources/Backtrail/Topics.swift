import SwiftUI

let TOPIC_KINDS: [(String, String)] = [
    ("topic", "a subject or idea that comes up"),
    ("project", "something being built"),
    ("tool", "software that gets used"),
    ("service", "something that runs in the background, own or hosted"),
    ("model", "a named AI model, like Haiku or Qwen"),
    ("site", "a website or company, like Google or Wikipedia"),
]

private struct Added {
    let entity: String
    let linked: Int
    let unread: Int
    let callsEach: Double
    var started: Int?
}

struct TopicsPanel: View {
    @EnvironmentObject var store: Store
    let sug: J?
    let changed: () -> Void
    @State private var pick = ""
    @State private var kind = "topic"
    @State private var more = false
    @State private var added: Added?
    @State private var hid = ""
    @State private var showHidden = false

    private var terms: [J] { sug?["entities"].arr ?? [] }
    private var aliases: [J] { sug?["aliases"].arr ?? [] }
    private var hidden: [String] { sug?["hidden"].arr.map(\.str) ?? [] }
    private var cur: J? { terms.first { $0["term"].str == pick } ?? terms.first }
    private var maxTurns: Int { max(1, terms.map { $0["turns"].int }.max() ?? 1) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let added { addedBanner(added) }
            if !hid.isEmpty { hiddenBanner }
            if !aliases.isEmpty { aliasBox }
            if let cur { reviewCard(cur) } else {
                Text("Nothing to review. New names show up here when they come up often.")
                    .font(.system(size: 13)).foregroundStyle(Color.soft)
                    .padding(14).frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.white.opacity(0.025), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.line, lineWidth: 1))
            }
            upNext
            if !hidden.isEmpty { hiddenList }
        }
        .onChange(of: cur?["term"].str) { _, _ in more = false; kind = "topic" }
    }

    private func send(_ name: String, _ k: String, aliasOf: String = "") async {
        let r = try? await API.post("/api/kg/entity", ["name": name, "kind": k, "alias_of": aliasOf])
        if let r, !r["entity"].str.isEmpty {
            added = Added(entity: r["entity"].str, linked: r["linked"].int, unread: r["unread"].int,
                          callsEach: (r["calls_each"].v as? NSNumber)?.doubleValue ?? 5)
            hid = ""
        }
        if k == "ignore" { hid = name; added = nil }
        pick = ""
        changed()
    }

    private func restore(_ term: String) async {
        _ = try? await API.post("/api/kg/entity", ["name": term, "kind": "unignore"])
        hid = ""
        pick = term
        changed()
    }

    private func read() async {
        guard let a = added else { return }
        let r = try? await API.post("/api/kg/extract", ["entity": a.entity, "limit": 30])
        if r?["ok"].bool == true { added?.started = r?["started"].int }
    }

    private func bar(_ n: Int, _ color: Color = .mut) -> some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.white.opacity(0.06))
                Capsule().fill(color).frame(width: max(g.size.width * 0.06, g.size.width * CGFloat(n) / CGFloat(maxTurns)))
            }
        }
        .frame(height: 6)
    }

    private func addedBanner(_ a: Added) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "checkmark").font(.system(size: 10, weight: .heavy)).foregroundStyle(Color(hex: 0x04130c))
                    .frame(width: 20, height: 20).background(Color.accent, in: Circle())
                Group {
                    if let s = a.started {
                        Text("**\(a.entity)** is in the graph with \(a.linked) \(a.linked == 1 ? "claim" : "claims"). ") + Text("Reading \(s) sessions in the background; reload in a minute.").foregroundColor(.soft)
                    } else if a.unread > 0 {
                        Text("**\(a.entity)** is in the graph with \(a.linked) \(a.linked == 1 ? "claim" : "claims"). ") + Text("\(a.unread) sessions mention it and are not read yet.").foregroundColor(.soft)
                    } else {
                        Text("**\(a.entity)** is in the graph with \(a.linked) \(a.linked == 1 ? "claim" : "claims"). ") + Text("No unread sessions mention it.").foregroundColor(.soft)
                    }
                }
                .font(.system(size: 13)).fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                Button { added = nil } label: { Image(systemName: "xmark").font(.system(size: 10, weight: .semibold)) }
                    .buttonStyle(.plain).foregroundStyle(Color.mut).help("Dismiss")
            }
            if a.unread > 0 && a.started == nil {
                let n = min(a.unread, 30)
                Button { Task { await read() } } label: {
                    Text("Read \(n) sessions now · about \(Int((Double(n) * a.callsEach).rounded())) model calls").frame(maxWidth: .infinity)
                }
                .buttonStyle(BtnStyle(kind: .primary))
            }
        }
        .padding(12)
        .background(Color.accent.opacity(0.1), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.accent.opacity(0.27), lineWidth: 1))
    }

    private var hiddenBanner: some View {
        HStack(spacing: 8) {
            (Text(hid).bold() + Text(" hidden.")).font(.system(size: 13)).lineLimit(1)
            Spacer(minLength: 0)
            Button("Undo") { Task { await restore(hid) } }.buttonStyle(BtnStyle())
            Button { hid = "" } label: { Image(systemName: "xmark").font(.system(size: 10, weight: .semibold)) }
                .buttonStyle(.plain).foregroundStyle(Color.mut).help("Dismiss")
        }
        .padding(.horizontal, 12).padding(.vertical, 8)
        .background(Color.white.opacity(0.04), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
    }

    private var aliasBox: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Same thing, spelled differently?").font(.system(size: 12.5, weight: .semibold))
            ForEach(aliases, id: \.["alias"].str) { a in
                HStack(spacing: 8) {
                    (Text(a["alias"].str).bold() + Text(" = \(a["entity"].str)").foregroundColor(.mut))
                        .font(.system(size: 13)).lineLimit(1)
                    Spacer(minLength: 0)
                    Button("Merge") { Task { await send(a["alias"].str, "project", aliasOf: a["entity"].str) } }.buttonStyle(BtnStyle())
                    Button("Keep apart") { Task { await send(a["alias"].str, "ignore") } }
                        .buttonStyle(BtnStyle(kind: .ghost)).help("Not the same: hide this suggestion")
                }
            }
        }
        .padding(12)
        .background(Color.white.opacity(0.025), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.line, lineWidth: 1))
    }

    private func marked(_ text: String, _ term: String) -> Text {
        guard !term.isEmpty, let r = text.range(of: term, options: .caseInsensitive) else { return Text(text) }
        return Text(text[..<r.lowerBound]) + Text(text[r]).bold().foregroundColor(.ink) + marked(String(text[r.upperBound...]), term)
    }

    private func reviewCard(_ c: J) -> some View {
        let term = c["term"].str
        let examples = c["examples"].arr
        return VStack(alignment: .leading, spacing: 0) {
            Text("Should this be in the graph?").font(.system(size: 12)).foregroundStyle(Color.mut)
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(term).font(.system(size: 26, weight: .semibold)).tracking(-0.4).lineLimit(1)
                Button { store.sheet = .search(term, thread: "") } label: {
                    Label("all mentions", systemImage: "magnifyingglass").font(.system(size: 12))
                }
                .buttonStyle(.plain).foregroundStyle(Color.mut).help("Search every mention")
            }
            .padding(.top, 2)
            HStack(spacing: 12) {
                bar(c["turns"].int, .accent).frame(width: 128)
                (Text("\(c["turns"].int)").bold().foregroundColor(.ink) + Text(" mentions in ") + Text("\(c["sessions"].int)").bold().foregroundColor(.ink) + Text(" sessions"))
                    .font(.system(size: 12.5)).foregroundStyle(Color.soft).monospacedDigit()
            }
            .padding(.top, 10)

            VStack(alignment: .leading, spacing: 8) {
                ForEach(Array((more ? examples : Array(examples.prefix(2))).enumerated()), id: \.offset) { _, x in
                    Button { store.go(.source(x["source_id"].int, chunk: nil)) } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(x["date"].str).font(.system(size: 11)).foregroundStyle(Color.mut)
                            (Text("“") + marked(x["text"].str, term) + Text("”"))
                                .font(.system(size: 12.5)).foregroundStyle(Color.soft).multilineTextAlignment(.leading)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        .padding(.horizontal, 12).padding(.vertical, 8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.white.opacity(0.02), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                        .overlay(alignment: .leading) { Rectangle().fill(Color.line2).frame(width: 2) }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                if examples.count > 2 {
                    Button(more ? "Show fewer quotes" : "Show \(examples.count - 2) more quotes") { more.toggle() }
                        .buttonStyle(.plain).font(.system(size: 12)).foregroundStyle(Color.mut).underline()
                }
            }
            .padding(.top, 14)

            Text("Add it as").font(.system(size: 12)).foregroundStyle(Color.mut).padding(.top, 16)
            LazyVGrid(columns: [GridItem(.flexible(), spacing: 6), GridItem(.flexible(), spacing: 6)], spacing: 6) {
                ForEach(TOPIC_KINDS, id: \.0) { k, d in
                    let on = kind == k
                    let col = KIND_COLOR[k] ?? .mut
                    Button { kind = k } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            HStack(spacing: 6) { Circle().fill(col).frame(width: 8, height: 8); Text(k).font(.system(size: 13, weight: .medium)) }
                            Text(d).font(.system(size: 11)).foregroundStyle(Color.mut).lineLimit(2).multilineTextAlignment(.leading)
                        }
                        .padding(.horizontal, 10).padding(.vertical, 6)
                        .frame(maxWidth: .infinity, minHeight: 52, alignment: .topLeading)
                        .background(on ? col.opacity(0.12) : Color.white.opacity(0.02), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(on ? col : Color.line2, lineWidth: 1))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.top, 6)
            HStack(spacing: 8) {
                Button { Task { await send(term, kind) } } label: { Text("Add “\(term)” to the graph").lineLimit(1).frame(maxWidth: .infinity) }
                    .buttonStyle(BtnStyle(kind: .primary, big: true))
                Button("Not a topic") { Task { await send(term, "ignore") } }
                    .buttonStyle(BtnStyle(big: true)).help("Hide it for good")
            }
            .padding(.top, 12)
        }
        .padding(16)
        .background(Color.white.opacity(0.035), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
        .shadow(color: .black.opacity(0.25), radius: 15, y: 8)
    }

    @ViewBuilder private var upNext: some View {
        let rest = terms.filter { $0["term"].str != cur?["term"].str }
        if !rest.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                Text("Up next").font(.system(size: 12)).foregroundStyle(Color.mut).padding(.horizontal, 4).padding(.bottom, 6)
                ForEach(rest, id: \.["term"].str) { t in
                    Button { pick = t["term"].str } label: {
                        HStack(spacing: 12) {
                            Text(t["term"].str).font(.system(size: 13)).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
                            bar(t["turns"].int).frame(width: 84)
                            Text("\(t["turns"].int)").font(.system(size: 12)).monospacedDigit().foregroundStyle(Color.mut).frame(width: 34, alignment: .trailing)
                        }
                        .padding(.horizontal, 8).padding(.vertical, 6).contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private var hiddenList: some View {
        DisclosureGroup(isExpanded: $showHidden) {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(hidden, id: \.self) { h in
                    HStack {
                        Text(h).font(.system(size: 13)).lineLimit(1)
                        Spacer(minLength: 0)
                        Button("Restore") { Task { await restore(h) } }.buttonStyle(BtnStyle(kind: .ghost))
                    }
                }
            }
            .padding(.top, 6)
        } label: {
            Text("Hidden (\(hidden.count))").font(.system(size: 12.5)).foregroundStyle(Color.mut)
        }
        .padding(.horizontal, 12).padding(.vertical, 8)
        .background(Color.white.opacity(0.02), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.line, lineWidth: 1))
    }
}
