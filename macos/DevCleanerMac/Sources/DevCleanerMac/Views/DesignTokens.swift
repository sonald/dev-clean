import SwiftUI

enum DCColor {
    static let window = Color(red: 0.043, green: 0.055, blue: 0.090)
    static let sidebar = Color(red: 0.008, green: 0.024, blue: 0.090)
    static let content = Color(red: 0.063, green: 0.067, blue: 0.082)
    static let panel = Color(red: 0.071, green: 0.086, blue: 0.133)
    static let panel2 = Color(red: 0.110, green: 0.110, blue: 0.118)
    static let elevated = Color(red: 0.102, green: 0.106, blue: 0.122)
    static let border = Color.white.opacity(0.10)
    static let text = Color.white
    static let secondary = Color(red: 0.63, green: 0.64, blue: 0.70)
    static let tertiary = Color(red: 0.39, green: 0.45, blue: 0.55)
    static let blue = Color(red: 0.0, green: 0.48, blue: 1.0)
    static let green = Color(red: 0.20, green: 0.84, blue: 0.29)
    static let yellow = Color(red: 1.0, green: 0.84, blue: 0.04)
    static let red = Color(red: 1.0, green: 0.27, blue: 0.23)
    static let purple = Color(red: 0.42, green: 0.37, blue: 0.92)
    static let indigo = Color(red: 0.51, green: 0.55, blue: 0.97)
}

struct DCCard<Content: View>: View {
    var padding: CGFloat = 18
    @ViewBuilder var content: Content

    var body: some View {
        content
            .padding(padding)
            .background(DCColor.panel)
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(DCColor.border)
            )
    }
}

struct PrimaryButtonStyle: ButtonStyle {
    var color: Color = DCColor.blue
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .background(color.opacity(configuration.isPressed ? 0.75 : 1))
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .opacity(isEnabled ? 1 : 0.4)
    }
}

struct SecondaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(DCColor.text)
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .background(DCColor.panel2.opacity(configuration.isPressed ? 0.7 : 1))
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(DCColor.border)
            )
            .opacity(isEnabled ? 1 : 0.4)
    }
}

struct GhostButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(configuration.isPressed ? DCColor.text.opacity(0.7) : DCColor.secondary)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .opacity(isEnabled ? 1 : 0.4)
    }
}

struct DCPageHeader<Trailing: View>: View {
    var title: String
    var subtitle: String?
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 8) {
                Text(title)
                    .font(.system(size: 28, weight: .bold))
                    .foregroundStyle(DCColor.text)
                if let subtitle {
                    Text(subtitle)
                        .font(.system(size: 13))
                        .foregroundStyle(DCColor.secondary)
                }
            }
            Spacer(minLength: 24)
            trailing
        }
    }
}

struct DCTopBar<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: 12) {
            content
        }
        .padding(.horizontal, 20)
        .frame(height: 48)
        .background(DCColor.content)
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(DCColor.border)
                .frame(height: 1)
        }
    }
}

struct DCPill: View {
    var title: String
    var color: Color = DCColor.secondary
    var filled = false

    var body: some View {
        Text(title)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .background(filled ? color.opacity(0.16) : DCColor.elevated)
            .clipShape(Capsule())
    }
}
