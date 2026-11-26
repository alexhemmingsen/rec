// Draws the DMG window's background: an arrow from Rec to Applications and
// a line of text under it. Writes background.png (640×400) and
// background@2x.png next to this file; scripts/release-macos.sh joins them
// into one TIFF so Finder picks the sharp one on Retina screens.
//
//   swift scripts/dmg/background.swift
//
// Coordinates match scripts/dmg/settings.py (icons centred at y = 180).

import AppKit

let width = 640.0, height = 400.0
let dir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()

func draw(scale: CGFloat, to name: String) {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: Int(width * scale), pixelsHigh: Int(height * scale),
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = NSSize(width: width, height: height)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    // AppKit's y grows upwards; the layout is in top-left coordinates.
    func y(_ top: Double) -> Double { height - top }

    NSColor(white: 0.97, alpha: 1).setFill()
    NSRect(x: 0, y: 0, width: width, height: height).fill()

    // The arrow between the icons (Rec at x 160, Applications at x 480).
    let arrowY = y(180)
    let gray = NSColor(white: 0.72, alpha: 1)
    gray.setStroke()
    let shaft = NSBezierPath()
    shaft.move(to: NSPoint(x: 262, y: arrowY))
    shaft.line(to: NSPoint(x: 364, y: arrowY))
    shaft.lineWidth = 5
    shaft.lineCapStyle = .round
    shaft.stroke()
    gray.setFill()
    let head = NSBezierPath()
    head.move(to: NSPoint(x: 382, y: arrowY))
    head.line(to: NSPoint(x: 360, y: arrowY + 14))
    head.line(to: NSPoint(x: 360, y: arrowY - 14))
    head.close()
    head.lineJoinStyle = .round
    head.lineWidth = 3
    head.fill()
    head.stroke()

    let text = "Drag Rec into Applications" as NSString
    let style = NSMutableParagraphStyle()
    style.alignment = .center
    let attributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: 13, weight: .medium),
        .foregroundColor: NSColor(white: 0.45, alpha: 1),
        .paragraphStyle: style,
    ]
    text.draw(in: NSRect(x: 0, y: y(330) - 9, width: width, height: 20), withAttributes: attributes)

    NSGraphicsContext.restoreGraphicsState()
    let png = rep.representation(using: .png, properties: [:])!
    try! png.write(to: dir.appendingPathComponent(name))
}

draw(scale: 1, to: "background.png")
draw(scale: 2, to: "background@2x.png")
print("wrote background.png and background@2x.png in \(dir.path)")
