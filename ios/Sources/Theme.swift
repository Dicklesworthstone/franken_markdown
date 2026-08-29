import SwiftUI
import UIKit

enum Lab {
    static let background = Color(red: 0.006, green: 0.027, blue: 0.019)
    static let panel = Color.black.opacity(0.54)
    static let stroke = Color.white.opacity(0.075)
    static let emerald = Color(red: 0.204, green: 0.827, blue: 0.6)
    static let cyan = Color(red: 0.25, green: 0.82, blue: 0.96)
    static let amber = Color(red: 0.98, green: 0.75, blue: 0.14)
    static let danger = Color(red: 0.97, green: 0.44, blue: 0.44)
    static let text = Color(red: 0.89, green: 0.91, blue: 0.94)
    static let secondary = Color(red: 0.58, green: 0.64, blue: 0.72)

    static func size(_ base: CGFloat) -> CGFloat {
#if targetEnvironment(macCatalyst)
        base * 1.38
#else
        UIFontMetrics(forTextStyle: .body).scaledValue(for: base)
#endif
    }
}

/// One family wordmark across the FrankenSuite: the name stays uppercase, but
/// the F and product initial carry the visual rhythm of the camel-case name.
struct FrankenWordmark: View {
    let productInitial: String
    let productRemainder: String
    let fullName: String
    var size: CGFloat = 20
    var accent: Color = Lab.emerald

    var body: some View {
        (
            Text("F")
                .font(.system(size: Lab.size(size), weight: .black, design: .monospaced))
                .foregroundColor(accent)
            + Text("RANKEN")
                .font(.system(size: Lab.size(size * 0.66), weight: .black, design: .monospaced))
                .foregroundColor(Lab.text.opacity(0.88))
            + Text(productInitial)
                .font(.system(size: Lab.size(size), weight: .black, design: .monospaced))
                .foregroundColor(accent)
            + Text(productRemainder)
                .font(.system(size: Lab.size(size * 0.66), weight: .black, design: .monospaced))
                .foregroundColor(Lab.text.opacity(0.88))
        )
        .kerning(0.8)
        .lineLimit(1)
        .minimumScaleFactor(0.72)
        .allowsTightening(true)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(fullName)
    }
}

struct LaboratoryBackground: View {
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    var body: some View {
        ZStack {
            Lab.background
            RadialGradient(
                colors: [Lab.emerald.opacity(reduceTransparency ? 0.06 : 0.17), .clear],
                center: .topLeading,
                startRadius: 0,
                endRadius: 720
            )
            Canvas { context, size in
                var path = Path()
                let step: CGFloat = 48
                stride(from: CGFloat.zero, through: size.width, by: step).forEach { x in
                    path.move(to: CGPoint(x: x, y: 0))
                    path.addLine(to: CGPoint(x: x, y: size.height))
                }
                stride(from: CGFloat.zero, through: size.height, by: step).forEach { y in
                    path.move(to: CGPoint(x: 0, y: y))
                    path.addLine(to: CGPoint(x: size.width, y: y))
                }
                context.stroke(path, with: .color(Lab.emerald.opacity(0.035)), lineWidth: 0.6)
            }
            .accessibilityHidden(true)
        }
        .ignoresSafeArea()
    }
}

struct LabPanel<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        content
            .padding(16)
            .background(Lab.panel, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 18, style: .continuous)
                    .stroke(Lab.stroke, lineWidth: 1)
            }
    }
}

struct LabLabel: View {
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.system(size: Lab.size(10), weight: .black, design: .monospaced))
            .kerning(2.1)
            .foregroundStyle(Lab.emerald)
    }
}

struct PrimaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: Lab.size(12), weight: .black, design: .monospaced))
            .textCase(.uppercase)
            .foregroundStyle(Color(red: 0.01, green: 0.08, blue: 0.05))
            .padding(.horizontal, 18)
            .padding(.vertical, 11)
            .background(
                LinearGradient(colors: [Lab.emerald, Lab.emerald.opacity(0.72)],
                               startPoint: .topLeading, endPoint: .bottomTrailing),
                in: Capsule()
            )
            .opacity(isEnabled ? (configuration.isPressed ? 0.72 : 1) : 0.34)
    }
}
