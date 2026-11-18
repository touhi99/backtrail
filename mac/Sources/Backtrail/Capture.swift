import SwiftUI

/// Add a todo, bug, question or decision to a thread (POST /api/add).
struct CaptureSheet: View {
    @EnvironmentObject var store: Store
    @Environment(\.dismiss) private var dismiss
    @State private var thread: String
    @State private var kind = "step|todo"
    @State private var text = ""
    @State private var detail = ""
    @State private var error = ""
    @FocusState private var focused: Bool

    private static let kinds: [(String, String, String, String)] = [
        ("step|todo", "Todo", "checklist", "A todo you own. Claude marks it built when it is, and you confirm."),
        ("step|bug", "Bug", "ladybug", "Something broken. Same flow as a todo."),
        ("question|", "Ask Claude", "questionmark.circle", "Claude answers this in its next session. It does not wait on you."),
        ("decision|", "Decision", "scale.3d", "A choice you made, kept as standing."),
    ]

    init(thread: String?) { _thread = State(initialValue: thread ?? "") }

    var body: some View {
        let threads = store.threads.filter { $0["name"].str != "Unsorted" }.map { $0["name"].str }
        let hint = Self.kinds.first { $0.0 == kind }?.3 ?? ""
        VStack(alignment: .leading, spacing: 14) {
            Text("Add to a thread").font(.system(size: 18, weight: .semibold))
            Picker("Thread", selection: $thread) {
                ForEach(threads, id: \.self) { Text($0).tag($0) }
            }
            HStack(spacing: 4) {
                ForEach(Self.kinds, id: \.0) { k in
                    Button { kind = k.0 } label: {
                        Label(k.1, systemImage: k.2)
                            .font(.system(size: 13, weight: kind == k.0 ? .medium : .regular))
                            .foregroundStyle(kind == k.0 ? Color.ink : Color.mut)
                            .frame(maxWidth: .infinity).frame(height: 30)
                            .background(kind == k.0 ? Color.white.opacity(0.1) : .clear, in: Capsule())
                            .contentShape(Capsule())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(4)
            .background(Color.white.opacity(0.03), in: Capsule())
            .overlay(Capsule().strokeBorder(Color.line2, lineWidth: 1))

            TextField(kind == "step|bug" ? "What is broken?" : kind == "question|" ? "Your question for Claude" : "One line", text: $text)
                .textFieldStyle(.roundedBorder).controlSize(.large).focused($focused)
            TextField("Details (optional)", text: $detail, axis: .vertical)
                .textFieldStyle(.roundedBorder).lineLimit(3...5)
            Text(hint).font(.system(size: 12)).foregroundStyle(Color.mut)
            if !error.isEmpty { Text(error).font(.system(size: 12)).foregroundStyle(Color.dropped) }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.buttonStyle(BtnStyle()).keyboardShortcut(.cancelAction)
                Button("Add") { submit() }.buttonStyle(BtnStyle(kind: .primary))
                    .keyboardShortcut(.defaultAction)
                    .disabled(text.trimmingCharacters(in: .whitespaces).isEmpty || thread.isEmpty)
            }
        }
        .padding(24)
        .frame(width: 520)
        .background(Color(hex: 0x141416))
        .onAppear {
            if thread.isEmpty { thread = store.moving.first?["name"].str ?? threads.first ?? "" }
            focused = true
        }
    }

    private func submit() {
        let parts = kind.split(separator: "|", omittingEmptySubsequences: false).map(String.init)
        Task {
            do {
                let r = try await API.post("/api/add", [
                    "thread": thread, "type": parts[0], "tag": parts[1],
                    "text": String(text.prefix(110)), "body": String(detail.prefix(320)),
                ])
                if r["ok"].bool { store.changed(); dismiss() } else { error = r["error"].s ?? "Not added" }
            } catch {
                self.error = "The store did not answer."
            }
        }
    }
}
