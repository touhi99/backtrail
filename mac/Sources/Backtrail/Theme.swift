import SwiftUI

extension Color {
    init(hex: UInt32, opacity: Double = 1) {
        self.init(.sRGB, red: Double((hex >> 16) & 0xff) / 255, green: Double((hex >> 8) & 0xff) / 255,
                  blue: Double(hex & 0xff) / 255, opacity: opacity)
    }
    static let bg = Color(hex: 0x0a0a0b)
    static let side = Color(hex: 0x0e0e10)
    static let raised = Color(hex: 0x19191c)
    static let line = Color(hex: 0x242428)
    static let line2 = Color(hex: 0x303036)
    static let ink = Color(hex: 0xf4f4f5)
    static let soft = Color(hex: 0xc4c4cc)
    static let mut = Color(hex: 0x8b8b95)
    static let accent = Color(hex: 0x34d399)
    static let moving = Color(hex: 0x60a5fa)
    static let open = Color(hex: 0xfbbf24)
    static let dropped = Color(hex: 0xfb7185)
    static let purple = Color(hex: 0xc084fc)
}

let STATE_COLOR: [String: Color] = [
    "moving": .moving, "built": .accent, "parked": .mut, "dropped": .dropped, "open": .open,
]
let STATE_NAME: [String: String] = ["moving": "In development", "built": "Built", "parked": "Parked", "dropped": "Dropped", "open": "Open idea"]
/// Who confirmed a decision.
let HOW_COLOR: [String: Color] = ["owner-stated": .accent, "owner-accepted": .moving, "from a note": .mut]
let HOW_NAME: [String: String] = ["owner-stated": "you said it", "owner-accepted": "you accepted it", "from a note": "from a note"]

/// The rounded dark panel used for every card.
struct Panel: ViewModifier {
    var radius: CGFloat = 20
    var border: Color = .line
    func body(content: Content) -> some View {
        content
            .background(
                LinearGradient(colors: [Color(hex: 0x151517), Color(hex: 0x111113)], startPoint: .top, endPoint: .bottom),
                in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(border, lineWidth: 1))
            .overlay(alignment: .top) {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .strokeBorder(LinearGradient(colors: [.white.opacity(0.05), .clear], startPoint: .top, endPoint: .center), lineWidth: 1)
            }
    }
}

extension View {
    func panel(radius: CGFloat = 20, border: Color = .line) -> some View { modifier(Panel(radius: radius, border: border)) }
}

/// A button style with hover and press states.
struct BtnStyle: ButtonStyle {
    enum Kind { case primary, normal, ghost }
    var kind: Kind = .normal
    var big = false
    @State private var hover = false

    func makeBody(configuration: Configuration) -> some View {
        let fg: Color = kind == .primary ? Color(hex: 0x04130c) : (kind == .ghost && !hover ? .soft : .ink)
        let bg: Color = switch kind {
        case .primary: .accent
        case .normal: hover ? Color(hex: 0x222226) : .raised
        case .ghost: hover ? Color(hex: 0x1b1b1f) : .clear
        }
        return configuration.label
            .font(.system(size: big ? 14 : 13, weight: kind == .primary ? .semibold : .medium))
            .foregroundStyle(fg)
            .padding(.horizontal, big ? 18 : 12)
            .frame(height: big ? 40 : 30)
            .background(bg, in: RoundedRectangle(cornerRadius: big ? 12 : 9, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: big ? 12 : 9, style: .continuous)
                .strokeBorder(kind == .normal ? Color.line2 : .clear, lineWidth: 1))
            .shadow(color: kind == .primary ? Color.accent.opacity(0.25) : .clear, radius: 8, y: 3)
            .brightness(kind == .primary && hover ? 0.06 : 0)
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
            .animation(.easeOut(duration: 0.1), value: configuration.isPressed)
            .onHover { hover = $0 }
            .contentShape(Rectangle())
    }
}

/// Lifts a card slightly on hover.
struct HoverLift: ViewModifier {
    @State private var hover = false
    func body(content: Content) -> some View {
        content
            .offset(y: hover ? -1 : 0)
            .brightness(hover ? 0.02 : 0)
            .animation(.easeOut(duration: 0.15), value: hover)
            .onHover { hover = $0 }
    }
}

struct Ring: View {
    var value: Double
    var color: Color = .accent
    var size: CGFloat = 46
    var stroke: CGFloat = 5
    var body: some View {
        ZStack {
            Circle().stroke(Color(hex: 0x26262b), lineWidth: stroke)
            Circle().trim(from: 0, to: max(0, min(1, value)))
                .stroke(color, style: StrokeStyle(lineWidth: stroke, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: size, height: size)
    }
}

/// A ring split into coloured segments with small gaps.
struct SegRing: View {
    var segs: [(Double, Color)]
    var size: CGFloat = 84
    var stroke: CGFloat = 9
    var body: some View {
        let tot = max(segs.reduce(0) { $0 + $1.0 }, 1)
        let gap = 0.012
        ZStack {
            Circle().stroke(Color(hex: 0x1d1d21), lineWidth: stroke)
            ForEach(Array(segs.enumerated()), id: \.offset) { n, s in
                let start = segs.prefix(n).reduce(0) { $0 + $1.0 } / tot
                let end = start + s.0 / tot
                if s.0 > 0 {
                    Circle().trim(from: start, to: max(start, end - gap))
                        .stroke(s.1, style: StrokeStyle(lineWidth: stroke, lineCap: .round))
                        .rotationEffect(.degrees(-90))
                }
            }
        }
        .frame(width: size, height: size)
    }
}

/// The square letter badge for a project.
struct Glyph: View {
    var name: String
    var color: Color
    var size: CGFloat = 36
    var body: some View {
        Text(String(name.prefix(1)))
            .font(.system(size: size * 0.42, weight: .semibold))
            .foregroundStyle(color)
            .frame(width: size, height: size)
            .background(color.opacity(0.12), in: RoundedRectangle(cornerRadius: size / 3, style: .continuous))
    }
}

struct Chip: View {
    var text: String
    var color: Color
    var body: some View {
        Text(text)
            .font(.system(size: 11.5, weight: .semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 9)
            .frame(height: 22)
            .background(color.opacity(0.13), in: Capsule())
    }
}

/// The soft green glow at the top of each page.
struct PageGlow: View {
    var body: some View {
        RadialGradient(colors: [Color.accent.opacity(0.09), .clear], center: UnitPoint(x: 0.3, y: -0.15), startRadius: 0, endRadius: 650)
            .frame(height: 420)
            .frame(maxHeight: .infinity, alignment: .top)
            .allowsHitTesting(false)
    }
}

/// The Backtrail mark: two points joined by a winding trail, on a 24-unit grid. The web app draws the same path.
struct TrailMark: Shape {
    func path(in rect: CGRect) -> Path {
        let s = min(rect.width, rect.height) / 24
        var p = Path()
        p.addEllipse(in: CGRect(x: 2.5, y: 2.5, width: 5, height: 5))
        p.move(to: CGPoint(x: 7.5, y: 5))
        p.addLine(to: CGPoint(x: 13, y: 5))
        p.addArc(center: CGPoint(x: 13, y: 8.5), radius: 3.5, startAngle: .degrees(-90), endAngle: .degrees(90), clockwise: false)
        p.addLine(to: CGPoint(x: 11, y: 12))
        p.addArc(center: CGPoint(x: 11, y: 15.5), radius: 3.5, startAngle: .degrees(-90), endAngle: .degrees(90), clockwise: true)
        p.addLine(to: CGPoint(x: 16.5, y: 19))
        p.addEllipse(in: CGRect(x: 16.5, y: 16.5, width: 5, height: 5))
        return p.applying(CGAffineTransform(scaleX: s, y: s).translatedBy(x: rect.minX / s, y: rect.minY / s))
    }
}

/// The app icon artwork: the mark in green on a dark rounded square.
struct AppIconArt: View {
    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 230, style: .continuous)
                .fill(LinearGradient(colors: [Color(hex: 0x1a1a1e), Color(hex: 0x0b0b0d)], startPoint: .top, endPoint: .bottom))
                .overlay(RoundedRectangle(cornerRadius: 230, style: .continuous).strokeBorder(Color.white.opacity(0.08), lineWidth: 6))
            RadialGradient(colors: [Color.accent.opacity(0.28), .clear], center: .center, startRadius: 0, endRadius: 420)
            TrailMark()
                .stroke(Color.accent, style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                .frame(width: 24, height: 24)
                .scaleEffect(22)
                .shadow(color: Color.accent.opacity(0.6), radius: 1.5)
        }
        .frame(width: 824, height: 824)
        .padding(100)
    }
}
