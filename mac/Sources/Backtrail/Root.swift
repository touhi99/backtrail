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
    }

    @ViewBuilder private var main: some View {
        if store.offline {
            Offline()
        } else if store.today == nil {
            Color.bg.frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ZStack(alignment: .topLeading) {
                switch store.page {
                case .home: HomeView()
                case .graph: GraphView()
                case .thread(let name): ThreadView(name: name).id(name)
                case .search(let q): SearchView(initial: q).id(q)
                case .source(let id, let c): SourceView(id: id, chunk: c).id("\(id)-\(c ?? 0)")
                }
                if !store.back.isEmpty {
                    Button { store.goBack() } label: { Image(systemName: "chevron.left").font(.system(size: 12, weight: .semibold)) }
                        .buttonStyle(BtnStyle(kind: .ghost)).help("Back (⌘[)").padding(.leading, 8).padding(.top, 8)
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
            .padding(.horizontal, 8).padding(.top, 30).padding(.bottom, 14)

            NavRow(icon: "house", label: "Home", active: store.page == .home) { store.go(.home) }
            NavRow(icon: "point.3.connected.trianglepath.dotted", label: "Graph", active: store.page == .graph) { store.go(.graph) }
            NavRow(icon: "magnifyingglass", label: "Search", active: { if case .search = store.page { return true } else { return false } }()) {
                store.go(.search(""))
            }

            Text("Threads").font(.system(size: 12)).foregroundStyle(Color.mut)
                .padding(.horizontal, 10).padding(.top, 18).padding(.bottom, 4)
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(store.moving, id: \.["name"].str) { t in threadRow(t) }
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
                            ForEach(others, id: \.["name"].str) { t in threadRow(t) }
                        }
                    }
                }
            }
            .scrollIndicators(.never)

            Spacer(minLength: 8)
            if let t = store.today?["totals"] {
                Text("\(t["sessions"].int) sessions, \(t["notes"].int) notes")
                    .font(.system(size: 11.5)).foregroundStyle(Color.mut).padding(.horizontal, 10).padding(.bottom, 14)
            }
        }
        .padding(.horizontal, 10)
        .frame(width: 228)
    }

    private func threadRow(_ t: J) -> some View {
        let name = t["name"].str
        return NavRow(dot: STATE_COLOR[t["state"].str] ?? .mut, label: name, active: store.page == .thread(name), small: true) {
            store.go(.thread(name))
        }
    }
}

struct NavRow: View {
    var icon: String? = nil
    var dot: Color? = nil
    var label: String
    var active: Bool
    var small = false
    var external = false
    var action: () -> Void
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                if let icon { Image(systemName: icon).font(.system(size: 13)).frame(width: 16) }
                if let dot { Circle().fill(dot).frame(width: 7, height: 7).frame(width: 16) }
                Text(label).lineLimit(1)
                Spacer(minLength: 0)
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
