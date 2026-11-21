import SwiftUI

struct RootView: View {
    @EnvironmentObject var store: Store
    @Environment(\.openURL) private var openURL

    var body: some View {
        HStack(spacing: 0) {
            Sidebar()
            main
                .background(Color.bg)
                .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Color.line, lineWidth: 1))
                .padding([.top, .bottom, .trailing], 8)
        }
        .background(Color.side)
        .foregroundStyle(Color.ink)
        .task { await store.reload() }
        .sheet(isPresented: Binding(get: { store.capture != nil }, set: { if !$0 { store.capture = nil } })) {
            CaptureSheet(thread: store.capture ?? nil)
        }
        .sheet(item: $store.sheet) { s in
            VStack(spacing: 0) {
                HStack {
                    Spacer()
                    Button { store.sheet = nil } label: { Image(systemName: "xmark").font(.system(size: 11, weight: .semibold)) }
                        .buttonStyle(BtnStyle(kind: .ghost)).keyboardShortcut(.cancelAction).help("Close (Esc)")
                }
                .padding(.horizontal, 12).padding(.top, 10)
                switch s {
                case .search(let q, let t): SearchView(initial: q, scope: t)
                case .items: ItemsPage()
                case .log: LogPage()
                }
            }
            .frame(width: 1000, height: 720)
            .background(Color.bg)
            .environmentObject(store)
        }
    }

    @ViewBuilder private var main: some View {
        if store.offline {
            Offline()
        } else if store.today == nil {
            Color.bg.frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            VStack(alignment: .leading, spacing: 0) {
                if let prev = store.back.last {
                    Button { store.goBack() } label: {
                        HStack(spacing: 4) {
                            Image(systemName: "chevron.left").font(.system(size: 11, weight: .semibold))
                            Text(prev.label).font(.system(size: 12.5)).lineLimit(1)
                        }
                    }
                    .buttonStyle(BtnStyle(kind: .ghost)).foregroundStyle(Color.mut).help("Back (⌘[)")
                    .padding(.leading, 12).padding(.top, 8)
                }
                switch store.page {
                case .home: HomeView()
                case .graph: GraphView()
                case .decisions(let name): GraphView(project: name).id(name)
                case .thread(let name): ThreadView(name: name).id(name)
                case .map(let name): MapView(thread: name).id(name)
                case .source(let id, let c): SourceView(id: id, chunk: c).id("\(id)-\(c ?? 0)")
                }
            }
        }
    }
}

struct Offline: View {
    @EnvironmentObject var store: Store
    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "bolt.horizontal.circle").font(.system(size: 34)).foregroundStyle(Color.mut)
            Text("Backtrail store not running").font(.system(size: 20, weight: .semibold))
            Text("Start it with `bt serve`, or load the dev.backtrail.serve login agent.")
                .font(.system(size: 13)).foregroundStyle(Color.mut)
            Button("Try again") { Task { await store.reload() } }.buttonStyle(BtnStyle(kind: .primary)).padding(.top, 6)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct Sidebar: View {
    @EnvironmentObject var store: Store
    @Environment(\.openURL) private var openURL
    @State private var showOthers = false
    @State private var query = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 10) {
                TrailMark().stroke(Color.accent, style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                    .frame(width: 16, height: 16)
                    .frame(width: 30, height: 30)
                    .background(Color(hex: 0x111111), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                    .shadow(color: Color.accent.opacity(0.4), radius: 7)
                Text("Backtrail").font(.system(size: 15, weight: .semibold))
            }
            .padding(.horizontal, 8).padding(.top, 30).padding(.bottom, 10)

            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").font(.system(size: 12)).foregroundStyle(Color.mut)
                TextField(store.scope.isEmpty ? "Search everything" : "Search \(store.scope)", text: $query)
                    .textFieldStyle(.plain).font(.system(size: 13))
                    .onSubmit {
                        let q = query.trimmingCharacters(in: .whitespaces)
                        if !q.isEmpty { store.sheet = .search(q, thread: store.scope); query = "" }
                    }
            }
            .padding(.horizontal, 10).frame(height: 32)
            .background(Color.black.opacity(0.3), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 9, style: .continuous).strokeBorder(Color.line2, lineWidth: 1))
            .padding(.bottom, 10)

            NavRow(icon: "house", label: "Home", active: store.page == .home) { store.go(.home) }
            NavRow(icon: "point.3.connected.trianglepath.dotted", label: "Graph", active: store.page == .graph,
                   badge: store.toReview > 0 ? "\(store.toReview)" : nil, onBadge: { store.openTopics = true; store.go(.graph) }) { store.go(.graph) }

            Text("Active threads").font(.system(size: 12)).foregroundStyle(Color.mut)
                .padding(.horizontal, 10).padding(.top, 18).padding(.bottom, 4)
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(store.moving, id: \.["name"].str) { t in threadRow(t, in: store.moving) }
                    let others = store.threads.filter { $0["state"].str != "moving" && $0["name"].str != "Unsorted" }
                    if !others.isEmpty {
                        Button { withAnimation(.easeOut(duration: 0.15)) { showOthers.toggle() } } label: {
                            HStack(spacing: 6) {
                                Image(systemName: "chevron.right").font(.system(size: 9, weight: .semibold))
                                    .rotationEffect(.degrees(showOthers ? 90 : 0))
                                Text("Other threads (\(others.count))")
                            }
                            .font(.system(size: 12)).foregroundStyle(Color.mut)
                            .padding(.horizontal, 10).padding(.top, 14).padding(.bottom, 4)
                        }
                        .buttonStyle(.plain)
                        if showOthers {
                            ForEach(others, id: \.["name"].str) { t in threadRow(t, in: others) }
                        }
                    }
                }
            }
            .scrollIndicators(.never)

            Spacer(minLength: 8)
            HStack(spacing: 2) {
                if let t = store.today?["totals"] {
                    Text("\(t["sessions"].int) sessions, \(t["notes"].int) notes")
                        .font(.system(size: 11.5)).foregroundStyle(Color.mut).lineLimit(1)
                }
                Spacer(minLength: 4)
                footButton("checklist", "Items: everything filed") { store.sheet = .items }
                footButton("scroll", "Log: every change") { store.sheet = .log }
            }
            .padding(.leading, 10).padding(.bottom, 12)
        }
        .padding(.horizontal, 10)
        .frame(width: 228)
    }

    private func footButton(_ icon: String, _ help: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: icon).font(.system(size: 12)).foregroundStyle(Color.mut)
                .frame(width: 26, height: 26).contentShape(Rectangle())
        }
        .buttonStyle(.plain).help(help)
    }

    private func threadRow(_ t: J, in list: [J]) -> some View {
        let name = t["name"].str
        let parent = t["parent"].str
        let nested = !parent.isEmpty && list.contains { $0["name"].str == parent }
        return NavRow(dot: STATE_COLOR[t["state"].str] ?? .mut, label: name, active: store.page == .thread(name), small: true) {
            store.go(.thread(name))
        }
        .padding(.leading, nested ? 14 : 0)
    }
}

struct NavRow: View {
    var icon: String? = nil
    var dot: Color? = nil
    var label: String
    var active: Bool
    var small = false
    var external = false
    var badge: String? = nil
    var onBadge: (() -> Void)? = nil
    var action: () -> Void
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                if let icon { Image(systemName: icon).font(.system(size: 13)).frame(width: 16) }
                if let dot { Circle().fill(dot).frame(width: 7, height: 7).frame(width: 16) }
                Text(label).lineLimit(1)
                Spacer(minLength: 0)
                if let badge {
                    Text(badge).font(.system(size: 11)).monospacedDigit().foregroundStyle(Color.open)
                        .padding(.horizontal, 6).background(Color.open.opacity(0.15), in: Capsule())
                        .onTapGesture { onBadge?() }.help("Topics to review")
                }
                if external && hover {
                    Image(systemName: "arrow.up.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color.mut)
                }
            }
            .font(.system(size: small ? 13 : 13.5, weight: active ? .medium : .regular))
            .foregroundStyle(active ? Color.ink : Color.soft)
            .padding(.horizontal, 10)
            .frame(height: small ? 28 : 32)
            .background(active ? Color.white.opacity(0.07) : hover ? Color.white.opacity(0.035) : .clear,
                        in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .help(external ? "Opens in the web app" : "")
    }
}
