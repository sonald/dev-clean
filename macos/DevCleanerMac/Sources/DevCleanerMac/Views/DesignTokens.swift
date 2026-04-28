import SwiftUI

enum DCColor {
    static let window = Color(red: 0.043, green: 0.055, blue: 0.090)
    static let panel = Color(red: 0.071, green: 0.086, blue: 0.133)
    static let panel2 = Color(red: 0.110, green: 0.110, blue: 0.118)
    static let border = Color.white.opacity(0.10)
    static let text = Color.white
    static let secondary = Color(red: 0.63, green: 0.64, blue: 0.70)
    static let blue = Color(red: 0.0, green: 0.48, blue: 1.0)
    static let green = Color(red: 0.20, green: 0.84, blue: 0.29)
    static let yellow = Color(red: 1.0, green: 0.84, blue: 0.04)
    static let red = Color(red: 1.0, green: 0.27, blue: 0.23)
    static let purple = Color(red: 0.42, green: 0.37, blue: 0.92)
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

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .background(color.opacity(configuration.isPressed ? 0.75 : 1))
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
}

struct SecondaryButtonStyle: ButtonStyle {
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
    }
}
