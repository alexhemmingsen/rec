// Builds Rec's app icon (1024x1024 PNG) from the square logo artwork: the
// artwork goes on the macOS icon grid (an 824pt rounded tile with a soft
// shadow and transparent margins).
// Run: swift assets/make-icon.swift assets/logo.jpg assets/icon.png
// then: pnpm tauri icon assets/icon.png
import AppKit

let args = CommandLine.arguments
guard args.count > 2, let logo = NSImage(contentsOfFile: args[1]),
      let art = logo.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    print("usage: swift assets/make-icon.swift <logo> <out.png>")
    exit(1)
}

let size = 1024
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
                           bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                           colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
let ctx = NSGraphicsContext.current!.cgContext
ctx.interpolationQuality = .high

let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
let tilePath = CGPath(roundedRect: tile, cornerWidth: 186, cornerHeight: 186, transform: nil)

// Shadow under the tile.
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: NSColor.black.withAlphaComponent(0.35).cgColor)
ctx.addPath(tilePath)
ctx.setFillColor(NSColor.black.cgColor)
ctx.fillPath()
ctx.restoreGState()

// The artwork, clipped to the tile.
ctx.saveGState()
ctx.addPath(tilePath)
ctx.clip()
ctx.draw(art, in: tile)
ctx.restoreGState()

NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: args[2]))
print("wrote \(args[2])")
