// Draws Rec's app icon (1024x1024 PNG): a dark tile with selection corner
// brackets around a red record dot. Run: swift assets/make-icon.swift assets/icon.png
import AppKit

let size: CGFloat = 1024
let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icon.png"
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size), pixelsHigh: Int(size),
                           bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                           colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
let ctx = NSGraphicsContext.current!.cgContext

// Tile: macOS icon grid, 824pt body with a continuous-ish corner.
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
let tilePath = CGPath(roundedRect: tile, cornerWidth: 186, cornerHeight: 186, transform: nil)
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: NSColor.black.withAlphaComponent(0.35).cgColor)
ctx.addPath(tilePath)
ctx.setFillColor(NSColor(srgbRed: 0.10, green: 0.10, blue: 0.12, alpha: 1).cgColor)
ctx.fillPath()
ctx.restoreGState()

ctx.saveGState()
ctx.addPath(tilePath)
ctx.clip()
let gradient = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB),
                          colors: [NSColor(srgbRed: 0.20, green: 0.20, blue: 0.24, alpha: 1).cgColor,
                                   NSColor(srgbRed: 0.08, green: 0.08, blue: 0.10, alpha: 1).cgColor] as CFArray,
                          locations: [0, 1])!
ctx.drawLinearGradient(gradient, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])
ctx.restoreGState()

// Selection corner brackets.
let inset: CGFloat = 250, arm: CGFloat = 120, w: CGFloat = 40
ctx.setStrokeColor(NSColor.white.withAlphaComponent(0.92).cgColor)
ctx.setLineWidth(w)
ctx.setLineCap(.round)
ctx.setLineJoin(.round)
let lo = inset, hi = size - inset
for (cx, cy, dx, dy) in [(lo, lo, 1.0, 1.0), (hi, lo, -1.0, 1.0), (lo, hi, 1.0, -1.0), (hi, hi, -1.0, -1.0)] as [(CGFloat, CGFloat, CGFloat, CGFloat)] {
    ctx.move(to: CGPoint(x: cx + dx * arm, y: cy))
    ctx.addLine(to: CGPoint(x: cx, y: cy))
    ctx.addLine(to: CGPoint(x: cx, y: cy + dy * arm))
    ctx.strokePath()
}

// Record dot.
let r: CGFloat = 132
ctx.setFillColor(NSColor(srgbRed: 1.0, green: 0.27, blue: 0.23, alpha: 1).cgColor)
ctx.fillEllipse(in: CGRect(x: 512 - r, y: 512 - r, width: r * 2, height: r * 2))

NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("wrote \(out)")
