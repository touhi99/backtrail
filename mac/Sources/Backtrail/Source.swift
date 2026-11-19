import SwiftUI

struct SourceView: View {
    @EnvironmentObject var store: Store
    @Environment(\.openURL) private var openURL
    let id: Int
    let chunk: Int?
    @State private var d: J?

    init(id: Int, chunk: Int?) {
        self.id = id
        self.chunk = chunk
        _d = State(initialValue: API.cached("/api/source?id=\(id)"))
    }

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                if let d, !d["source"].isNull { content(d) } else { Color.clear.frame(height: 400) }
            }
            .background(PageGlow())
            .task {
                if let fresh = try? await API.get("/api/source?id=\(id)") { d = fresh }
                if let chunk {
                    try? await Task.sleep(nanoseconds: 150_000_000)
                    withAnimation { proxy.scrollTo(chunk, anchor: .center) }
                }
            }
        }
    }

    private func content(_ d: J) -> some View {
        let s = d["source"]
        let chunks = d["chunks"].arr
        let cited = chunk.flatMap { c in chunks.first { $0["id"].int == c } }
        let first = s["first_ts"].s, last = s["last_ts"].s
        let span = dayFmt(first) + (last != nil && last?.prefix(10) != first?.prefix(10) ? " to \(dayFmt(last))" : "")
        let items = d["items"].arr

        return VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 8) {
                    Chip(text: SOURCE_KIND_NAME[s["kind"].str] ?? s["kind"].str, color: SOURCE_KIND_COLOR[s["kind"].str] ?? .mut)
                    Chip(text: span, color: .soft)
                    Chip(text: "\(s["nchunks"].int) passages", color: .soft)
                    Spacer()
                    if s["kind"].str == "artifact" || s["kind"].str == "note" {
                        Button { openURL(API.web("/source/\(id)")) } label: { Label("Formatted", systemImage: "arrow.up.right") }
                            .buttonStyle(BtnStyle(kind: .ghost)).help("Opens the rendered page in the web app")
                    }
                }
                Text(s["title"].str).font(.system(size: 28, weight: .semibold)).tracking(-0.4)
                    .frame(maxWidth: 720, alignment: .leading)
                if !items.isEmpty {
                    FlowLayout(spacing: 6) {
                        ForEach(items.prefix(6)) { i in
                            Button { store.go(.thread(i["thread"].str)) } label: {
                                HStack(spacing: 6) {
                                    Circle().fill(ITEM_STATE_COLOR[i["state"].str] ?? .mut).frame(width: 6, height: 6)
                                    Text(i["title"].str).lineLimit(1)
                                }
                                .font(.system(size: 12)).foregroundStyle(Color.soft)
                                .padding(.horizontal, 10).frame(height: 26).frame(maxWidth: 340)
                                .background(Color(hex: 0x1c1c21), in: Capsule())
                            }
                            .buttonStyle(.plain).modifier(HoverLift())
                        }
                        if items.count > 6 {
                            Text("\(items.count - 6) more filed from this").font(.system(size: 12)).foregroundStyle(Color.mut)
                                .padding(.horizontal, 10).frame(height: 26)
                        }
                    }
                }
            }

            if let cited {
                VStack(alignment: .leading, spacing: 6) {
                    Text("The passage that was cited").font(.system(size: 12, weight: .medium)).foregroundStyle(Color.accent)
                    Text(String(cited["text"].str.prefix(900))).font(.system(size: 13.5)).foregroundStyle(Color.soft)
                        .textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                }
                .padding(20).frame(maxWidth: .infinity, alignment: .leading)
                .background(Color.accent.opacity(0.05), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Color.accent.opacity(0.4), lineWidth: 1))
            }

            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(Array(chunks.enumerated()), id: \.offset) { n, x in
                    if n > 0 { Divider().overlay(Color.line) }
                    let role = x["role"].str
                    let hit = x["id"].int == chunk
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(spacing: 6) {
                            if role == "user" { Text("You").foregroundStyle(Color.accent) }
                            if role == "assistant" { Text("Claude").foregroundStyle(Color.purple) }
                            Text(x["locator"].str.trimmingCharacters(in: CharacterSet(charactersIn: "#§")))
                        }
                        .font(.system(size: 12)).foregroundStyle(Color.mut)
                        Text(md(x["text"].str)).font(.system(size: 13.5)).lineSpacing(4).foregroundStyle(Color.soft)
                            .textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(.horizontal, 20).padding(.vertical, 13)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(hit ? Color.open.opacity(0.08) : .clear)
                    .id(x["id"].int)
                }
            }
            .panel(radius: 18)
        }
        .padding(.horizontal, 32).padding(.top, 30).padding(.bottom, 44)
        .frame(maxWidth: 1080)
        .frame(maxWidth: .infinity)
    }
}

struct FlowLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(proposal.width ?? .infinity, subviews)
        let w = rows.map { $0.last.map { $0.x + $0.w } ?? 0 }.max() ?? 0
        let h = rows.last.flatMap { $0.first.map { $0.y + $0.h } } ?? 0
        return CGSize(width: w, height: h)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for row in arrange(bounds.width, subviews) {
            for p in row {
                subviews[p.i].place(at: CGPoint(x: bounds.minX + p.x, y: bounds.minY + p.y), proposal: ProposedViewSize(width: p.w, height: p.h))
            }
        }
    }

    private struct P { let i: Int; let x: CGFloat; let y: CGFloat; let w: CGFloat; let h: CGFloat }

    private func arrange(_ width: CGFloat, _ subviews: Subviews) -> [[P]] {
        var rows: [[P]] = [[]]
        var x: CGFloat = 0, y: CGFloat = 0, rowH: CGFloat = 0
        for (i, v) in subviews.enumerated() {
            let s = v.sizeThatFits(.unspecified)
            let w = min(s.width, width)
            if x > 0 && x + w > width {
                y += rowH + spacing; x = 0; rowH = 0; rows.append([])
            }
            rows[rows.count - 1].append(P(i: i, x: x, y: y, w: w, h: s.height))
            x += w + spacing
            rowH = max(rowH, s.height)
        }
        return rows
    }
}

func md(_ s: String) -> AttributedString {
    (try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(s)
}
