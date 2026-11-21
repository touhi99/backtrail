import SwiftUI

struct WhoChip: View {
    let i: J
    var body: some View {
        let you = i["origin"].str == "owner"
        Chip(text: you ? "You" : "Claude", color: you ? .ink : .moving)
            .help(you ? "You typed this in the app" : "Claude filed this from a session")
    }
}

struct ReasonAsk {
    let title: String
    let run: (String) -> Void
}

struct ReasonPrompt: ViewModifier {
    @Binding var ask: ReasonAsk?
    @State private var text = ""

    func body(content: Content) -> some View {
        content.alert(ask?.title ?? "", isPresented: Binding(get: { ask != nil }, set: { if !$0 { ask = nil } })) {
            TextField("Reason", text: $text)
            Button("Cancel", role: .cancel) { ask = nil; text = "" }
            Button("OK") { ask?.run(text); ask = nil; text = "" }
        }
    }
}

extension View {
    func reasonPrompt(_ ask: Binding<ReasonAsk?>) -> some View { modifier(ReasonPrompt(ask: ask)) }
}

struct ItemActions: View {
    @EnvironmentObject var store: Store
    let i: J
    @State private var ask: ReasonAsk?

    private var state: String { i["state"].str }
    private var review: String { i["review"].str }
    private var replaceable: Bool { i["type"].str == "decision" && !(i["stale"].s ?? "").hasPrefix("superseded") }

    var body: some View {
        HStack(spacing: 6) {
            if review == "auto" && state == "settled" {
                Text("Kept automatically").font(.system(size: 12)).foregroundStyle(Color.mut)
                    .help("You said this in a session in so many words, so it was kept without review.")
                reject
                if replaceable { replace }
            } else if state == "proposed" {
                if review.hasPrefix("held|") {
                    Chip(text: "May repeat", color: .open).fixedSize().help("Not kept automatically: it \(review.dropFirst(5))")
                }
                Button("Accept") { run("/api/review", ["accept": true]) }.buttonStyle(BtnStyle(kind: .primary))
                Button("Not valid now") {
                    ask = ReasonAsk(title: "Why is it not valid now?") { run("/api/review", ["accept": false, "park": true, "reason": $0]) }
                }
                .buttonStyle(BtnStyle()).help("Was true once, no longer applies. Kept on file, out of the way.")
                reject
                move
            } else if i["type"].str == "decision" && state == "settled" && replaceable {
                replace
            } else {
                move
            }
        }
        .reasonPrompt($ask)
    }

    private var reject: some View {
        Button("Reject") {
            ask = ReasonAsk(title: "Why reject? (kept with the item)") { run("/api/review", ["accept": false, "reason": $0]) }
        }
        .buttonStyle(BtnStyle()).help("Wrong or not wanted.")
    }

    private var replace: some View {
        Button("Mark replaced") {
            ask = ReasonAsk(title: "What replaced it?") { run("/api/supersede", ["reason": $0]) }
        }
        .buttonStyle(BtnStyle())
    }

    @ViewBuilder private var move: some View {
        if i["origin"].str == "owner" && state != "done" {
            Menu("Move to…") {
                ForEach(store.threads.map { $0["name"].str }.filter { $0 != i["thread"].str && $0 != "Unsorted" }, id: \.self) { t in
                    Button(t) { run("/api/move", ["thread": t]) }
                }
            }
            .menuStyle(.borderlessButton).fixedSize()
            .font(.system(size: 12)).foregroundStyle(Color.mut)
            .help("Typed in the wrong thread? Move it.")
        }
    }

    private func run(_ path: String, _ body: [String: Any]) {
        Task {
            _ = try? await API.post(path, body.merging(["id": i.id]) { a, _ in a })
            store.changed()
        }
    }
}
