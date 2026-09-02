import SwiftUI
import UIKit

enum LabAppearance: String {
    static let storageKey = "frankenmarkdown.appearance"
    case dark
    case light
    var colorScheme: ColorScheme { self == .dark ? .dark : .light }
}

enum Lab {
    static let background = adaptive(
        dark: UIColor(red: 0.006, green: 0.027, blue: 0.019, alpha: 1),
        light: UIColor(red: 0.965, green: 0.982, blue: 0.972, alpha: 1)
    )
    static let panel = adaptive(
        dark: UIColor(white: 0, alpha: 0.54),
        light: UIColor(red: 1, green: 1, blue: 0.998, alpha: 0.97)
    )
    static let panelStrong = adaptive(
        dark: UIColor(white: 0, alpha: 0.88),
        light: UIColor(red: 0.970, green: 0.987, blue: 0.977, alpha: 0.99)
    )
    static let panelSoft = adaptive(
        dark: UIColor(white: 1, alpha: 0.035),
        light: UIColor(red: 0.04, green: 0.20, blue: 0.11, alpha: 0.055)
    )
    static let stroke = adaptive(
        dark: UIColor(white: 1, alpha: 0.075),
        light: UIColor(red: 0.03, green: 0.22, blue: 0.12, alpha: 0.20)
    )
    static let emerald = adaptive(
        dark: UIColor(red: 0.204, green: 0.827, blue: 0.6, alpha: 1),
        light: UIColor(red: 0.015, green: 0.405, blue: 0.235, alpha: 1)
    )
    static let cyan = adaptive(
        dark: UIColor(red: 0.25, green: 0.82, blue: 0.96, alpha: 1),
        light: UIColor(red: 0.015, green: 0.405, blue: 0.535, alpha: 1)
    )
    static let amber = adaptive(
        dark: UIColor(red: 0.98, green: 0.75, blue: 0.14, alpha: 1),
        light: UIColor(red: 0.65, green: 0.37, blue: 0.005, alpha: 1)
    )
    static let danger = adaptive(
        dark: UIColor(red: 0.97, green: 0.44, blue: 0.44, alpha: 1),
        light: UIColor(red: 0.70, green: 0.12, blue: 0.16, alpha: 1)
    )
    static let text = adaptive(
        dark: UIColor(red: 0.89, green: 0.91, blue: 0.94, alpha: 1),
        light: UIColor(red: 0.045, green: 0.115, blue: 0.075, alpha: 1)
    )
    static let secondary = adaptive(
        dark: UIColor(red: 0.58, green: 0.64, blue: 0.72, alpha: 1),
        light: UIColor(red: 0.285, green: 0.365, blue: 0.315, alpha: 1)
    )
    static let shadow = adaptive(
        dark: UIColor(white: 0, alpha: 0.36),
        light: UIColor(red: 0.02, green: 0.16, blue: 0.10, alpha: 0.12)
    )

    private static func adaptive(dark: UIColor, light: UIColor) -> Color {
        Color(uiColor: UIColor { traits in traits.userInterfaceStyle == .dark ? dark : light })
    }

    static func size(_ base: CGFloat) -> CGFloat {
#if targetEnvironment(macCatalyst)
        base * 1.38
#else
        UIFontMetrics(forTextStyle: .body).scaledValue(for: base)
#endif
    }
}

struct LabAppearanceButton: View {
    @Binding var selection: String
    private var appearance: LabAppearance { LabAppearance(rawValue: selection) ?? .dark }
    private var targetAppearance: LabAppearance { appearance == .dark ? .light : .dark }
    private var accent: Color { targetAppearance == .light ? Lab.amber : Lab.cyan }

    var body: some View {
        Button {
            selection = targetAppearance.rawValue
        } label: {
            HStack(spacing: 7) {
                Image(systemName: targetAppearance == .light ? "sun.max.fill" : "moon.stars.fill")
                    .font(.system(size: Lab.size(12), weight: .bold))
                    .frame(width: 26, height: 26)
                    .background(accent.opacity(0.14), in: Circle())
                Text(targetAppearance == .light ? "LIGHT" : "DARK")
                    .font(.system(size: Lab.size(8.5), weight: .black, design: .monospaced))
                    .kerning(0.8)
            }
            .foregroundStyle(accent)
            .padding(.leading, 5)
            .padding(.trailing, 11)
            .frame(minHeight: 44)
            .background(Lab.panel, in: Capsule())
            .overlay {
                Capsule().strokeBorder(
                    LinearGradient(
                        colors: [accent.opacity(0.48), Lab.stroke, accent.opacity(0.20)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ),
                    lineWidth: 1
                )
            }
            .shadow(color: .black.opacity(appearance == .light ? 0.11 : 0.28), radius: 8, y: 4)
        }
        .buttonStyle(AppearanceTogglePressStyle())
        .animation(.snappy(duration: 0.24), value: selection)
        .accessibilityIdentifier("appearance-toggle")
        .accessibilityLabel(appearance == .dark ? "Switch to light mode" : "Switch to dark mode")
        .accessibilityValue(appearance == .dark ? "Dark mode" : "Light mode")
        .accessibilityHint("Remembers this choice for future launches")
    }
}

private struct AppearanceTogglePressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
            .opacity(configuration.isPressed ? 0.78 : 1)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
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
                .foregroundColor(Lab.text.opacity(0.88))
            + Text("RANKEN")
                .font(.system(size: Lab.size(size * 0.66), weight: .black, design: .monospaced))
                .foregroundColor(Lab.text.opacity(0.88))
            + Text(productInitial)
                .font(.system(size: Lab.size(size), weight: .black, design: .monospaced))
                .foregroundColor(accent)
            + Text(productRemainder)
                .font(.system(size: Lab.size(size * 0.66), weight: .black, design: .monospaced))
                .foregroundColor(accent)
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
                stride(from: CGFloat.zero, through: size.width, by: step).forEach { positionX in
                    path.move(to: CGPoint(x: positionX, y: 0))
                    path.addLine(to: CGPoint(x: positionX, y: size.height))
                }
                stride(from: CGFloat.zero, through: size.height, by: step).forEach { positionY in
                    path.move(to: CGPoint(x: 0, y: positionY))
                    path.addLine(to: CGPoint(x: size.width, y: positionY))
                }
                context.stroke(path, with: .color(Lab.stroke.opacity(0.32)), lineWidth: 0.6)
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
            .shadow(color: Lab.shadow, radius: 14, y: 7)
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
