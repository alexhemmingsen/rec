// End-to-end test of Rec on macOS: drives the real app with synthetic
// keyboard and mouse events and checks what a user would see.
//
//   swiftc -O -o rec-e2e e2e/macos/RecE2E.swift
//   ./rec-e2e path/to/Rec.app
//
// Needs Accessibility (to post events) for this process and Screen Recording
// for Rec.app. CI grants both with e2e/macos/grant-permissions.sh.
//
// Checks, in order:
//  1. The shortcut shows the selection overlay over the whole display.
//  2. Esc cancels it.
//  3. Dragging a region starts recording at once; the Stop pill appears
//     outside the region; clicking it puts the MP4's absolute path on the
//     clipboard; the file has the region's size and the expected length.
//  4. A click without dragging cancels.
//  5. Recording the whole display: the pill sits inside the region but is
//     not in the video (Rec excludes its own windows).
//  6. History lists both recordings.

import AppKit
import AVFoundation
import CoreGraphics

setvbuf(stdout, nil, _IOLBF, 0)

func log(_ s: String) { print("[e2e] \(s)") }
func fail(_ s: String) -> Never {
    print("[e2e] FAIL: \(s)")
    exit(1)
}
func check(_ cond: Bool, _ s: String) { if !cond { fail(s) } else { log("ok: \(s)") } }
func pause(_ s: Double) { Thread.sleep(forTimeInterval: s) }

struct Win {
    let layer: Int
    let bounds: CGRect
    let onscreen: Bool
}

func recWindows() -> [Win] {
    guard let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] else { return [] }
    return list.compactMap { w in
        guard (w[kCGWindowOwnerName as String] as? String) == "Rec",
              let b = w[kCGWindowBounds as String] as? [String: Any],
              let rect = CGRect(dictionaryRepresentation: b as CFDictionary) else { return nil }
        return Win(
            layer: w[kCGWindowLayer as String] as? Int ?? 0,
            bounds: rect,
            onscreen: (w[kCGWindowIsOnscreen as String] as? Bool) ?? false
        )
    }
}

func waitFor(_ what: String, timeout: Double, _ cond: () -> Bool) {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
        if cond() { log("ok: \(what)"); return }
        pause(0.1)
    }
    log("windows now: \(recWindows().filter { $0.onscreen }.map { "layer \($0.layer) \($0.bounds)" })")
    fail("timed out waiting for: \(what)")
}

let source = CGEventSource(stateID: .hidSystemState)

func key(_ code: CGKeyCode, _ flags: CGEventFlags = []) {
    for down in [true, false] {
        let e = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down)!
        e.flags = flags
        e.post(tap: .cghidEventTap)
        pause(0.05)
    }
}

var eventTap: CGEventTapLocation = .cghidEventTap

/// REC_E2E_DIRECT=1 (CI): deliver mouse events straight to Rec's process.
/// GitHub's macOS VMs drop system-routed drags, so CI addresses the window
/// under the point itself; Rec's handlers still get ordinary NSEvents.
let direct = ProcessInfo.processInfo.environment["REC_E2E_DIRECT"] == "1"

func recPid() -> pid_t? {
    NSRunningApplication.runningApplications(withBundleIdentifier: "com.alexhem.rec").first?.processIdentifier
}

/// Rec's topmost on-screen window under a point.
func recWindowNumber(at p: CGPoint) -> Int64? {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else { return nil }
    for w in list where (w[kCGWindowOwnerName as String] as? String) == "Rec" {
        guard let b = w[kCGWindowBounds as String] as? [String: Any],
              let rect = CGRect(dictionaryRepresentation: b as CFDictionary), rect.contains(p),
              let number = w[kCGWindowNumber as String] as? Int else { continue }
        return Int64(number)
    }
    return nil
}

func mouse(_ type: CGEventType, _ p: CGPoint) {
    let e = CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: p, mouseButton: .left)!
    if type == .leftMouseDown || type == .leftMouseUp || type == .leftMouseDragged {
        e.setIntegerValueField(.mouseEventClickState, value: 1)
    }
    if direct, let pid = recPid() {
        CGWarpMouseCursorPosition(p)
        if let window = recWindowNumber(at: p) {
            e.setIntegerValueField(.mouseEventWindowUnderMousePointer, value: window)
            e.setIntegerValueField(.mouseEventWindowUnderMousePointerThatCanHandleThisEvent, value: window)
        }
        e.postToPid(pid)
    } else {
        e.post(tap: eventTap)
    }
}

func drag(_ a: CGPoint, _ b: CGPoint) {
    mouse(.mouseMoved, a)
    pause(0.3)
    mouse(.leftMouseDown, a)
    pause(0.1)
    log("after mousedown: pressed buttons \(NSEvent.pressedMouseButtons), cursor \(NSEvent.mouseLocation)")
    for i in 1...30 {
        let t = CGFloat(i) / 30
        mouse(.leftMouseDragged, CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t))
        pause(0.015)
    }
    mouse(.leftMouseUp, b)
}

func click(_ p: CGPoint) {
    mouse(.mouseMoved, p)
    pause(0.2)
    mouse(.leftMouseDown, p)
    pause(0.08)
    mouse(.leftMouseUp, p)
}

/// The shortcut from Rec's settings (default ⌃⇧5).
let (shortcutKey, shortcutFlags): (CGKeyCode, CGEventFlags) = {
    let keyCodes: [String: CGKeyCode] = [
        "A": 0, "S": 1, "D": 2, "F": 3, "H": 4, "G": 5, "Z": 6, "X": 7, "C": 8, "V": 9, "B": 11, "Q": 12,
        "W": 13, "E": 14, "R": 15, "Y": 16, "T": 17, "1": 18, "2": 19, "3": 20, "4": 21, "6": 22, "5": 23,
        "9": 25, "7": 26, "8": 28, "0": 29, "O": 31, "U": 32, "I": 34, "P": 35, "L": 37, "J": 38, "K": 40,
        "N": 45, "M": 46,
    ]
    let settings = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Application Support/com.alexhem.rec/settings.json")
    var accelerator = "Control+Shift+5"
    if let data = try? Data(contentsOf: settings),
       let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
       let s = json["shortcut"] as? String { accelerator = s }
    var flags: CGEventFlags = []
    var code: CGKeyCode = 23
    for token in accelerator.split(separator: "+").map(String.init) {
        switch token.lowercased() {
        case "control", "ctrl": flags.insert(.maskControl)
        case "alt", "option": flags.insert(.maskAlternate)
        case "shift": flags.insert(.maskShift)
        case "super", "cmd", "command", "meta": flags.insert(.maskCommand)
        default:
            let k = token.replacingOccurrences(of: "Digit", with: "").replacingOccurrences(of: "Key", with: "").uppercased()
            if let c = keyCodes[k] { code = c } else { fail("can't press shortcut key \(token)") }
        }
    }
    log("shortcut: \(accelerator)")
    return (code, flags)
}()
func pressShortcut() { key(shortcutKey, shortcutFlags) }

let display = CGDisplayBounds(CGMainDisplayID())
let mode = CGDisplayCopyDisplayMode(CGMainDisplayID())!
let scale = Double(mode.pixelWidth) / Double(display.width)

/// Rec's selection overlay sits at NSPopUpMenuWindowLevel.
let overlayLayer = 101
func overlayVisible() -> Bool { recWindows().contains { $0.layer == overlayLayer && $0.onscreen } }
func pill() -> Win? {
    recWindows().first { $0.onscreen && abs($0.bounds.width - 168) < 1 && abs($0.bounds.height - 40) < 1 }
}
func clipboard() -> String? { NSPasteboard.general.string(forType: .string) }

func waitForPath() -> String {
    var path = ""
    waitFor("an .mp4 path on the clipboard", timeout: 15) {
        guard let s = clipboard()?.trimmingCharacters(in: CharacterSet(charactersIn: "\"")), s.hasSuffix(".mp4") else { return false }
        path = s
        return true
    }
    return path
}

func videoInfo(_ path: String) -> (duration: Double, size: CGSize, asset: AVURLAsset) {
    let asset = AVURLAsset(url: URL(fileURLWithPath: path))
    let sem = DispatchSemaphore(value: 0)
    var result: (Double, CGSize) = (0, .zero)
    // Detached: the main thread is blocked on the semaphore.
    Task.detached {
        let duration = try await asset.load(.duration)
        let track = try await asset.loadTracks(withMediaType: .video).first!
        let size = try await track.load(.naturalSize)
        result = (duration.seconds, size)
        sem.signal()
    }
    sem.wait()
    return (result.0, result.1, asset)
}

func frame(_ asset: AVURLAsset, at seconds: Double) -> CGImage {
    let gen = AVAssetImageGenerator(asset: asset)
    gen.requestedTimeToleranceBefore = .zero
    gen.requestedTimeToleranceAfter = .zero
    return try! gen.copyCGImage(at: CMTime(seconds: seconds, preferredTimescale: 600), actualTime: nil)
}

/// RGBA pixels of a CGImage.
func pixels(_ img: CGImage) -> (data: [UInt8], width: Int, height: Int) {
    let w = img.width, h = img.height
    var data = [UInt8](repeating: 0, count: w * h * 4)
    let ctx = CGContext(data: &data, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                        space: CGColorSpace(name: CGColorSpace.sRGB)!,
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))
    return (data, w, h)
}

// ---------------------------------------------------------------------------

guard CommandLine.arguments.count > 1 else { fail("usage: rec-e2e path/to/Rec.app") }
let appPath = CommandLine.arguments[1]
log("display \(display), scale \(scale)\(direct ? ", mouse events posted to Rec directly" : "")")

NSPasteboard.general.clearContents()
let open = Process()
open.executableURL = URL(fileURLWithPath: "/usr/bin/open")
open.arguments = ["-n", "--env", "REC_DEBUG=1", appPath]
try! open.run()
open.waitUntilExit()

waitFor("Rec's main window", timeout: 30) { recWindows().contains { $0.onscreen && $0.bounds.width >= 700 } }
pause(2)

// 1 + 2: shortcut shows the overlay, Esc cancels.
pressShortcut()
waitFor("the selection overlay after the shortcut", timeout: 5, overlayVisible)
let overlay = recWindows().first { $0.layer == overlayLayer && $0.onscreen && $0.bounds.origin == display.origin }
check(overlay != nil && overlay!.bounds.size == display.size, "the overlay covers the whole main display")
waitFor("the main window to get out of the way", timeout: 3) {
    !recWindows().contains { $0.onscreen && $0.layer == 0 && $0.bounds.width >= 700 }
}
pause(0.5)
key(53) // Esc
waitFor("Esc to cancel the selection", timeout: 5) { !overlayVisible() }

// 3: record a region.
NSPasteboard.general.clearContents()
pause(0.5)
pressShortcut()
waitFor("the overlay again", timeout: 5, overlayVisible)
pause(0.5)
let region = CGRect(x: display.minX + 160, y: display.minY + 140, width: 400, height: 300)
drag(region.origin, CGPoint(x: region.maxX, y: region.maxY))
log("after mouseup: pressed buttons \(NSEvent.pressedMouseButtons), cursor \(NSEvent.mouseLocation)")
let pillDeadline = Date().addingTimeInterval(8)
while pill() == nil && Date() < pillDeadline { pause(0.1) }
if pill() == nil {
    // Is Rec still responsive? Esc should close the overlay.
    key(53)
    pause(1.5)
    log("after Esc: overlay visible = \(overlayVisible())")
    // Retry through the session event tap to tell a delivery problem from an
    // app problem.
    eventTap = .cgSessionEventTap
    mouse(.leftMouseUp, CGPoint(x: region.maxX, y: region.maxY))
    pause(0.5)
    pressShortcut()
    waitFor("the overlay (session-tap retry)", timeout: 5, overlayVisible)
    pause(0.5)
    drag(region.origin, CGPoint(x: region.maxX, y: region.maxY))
    let retryDeadline = Date().addingTimeInterval(8)
    while pill() == nil && Date() < retryDeadline { pause(0.1) }
    log("session-tap retry: pill \(pill() == nil ? "missing" : "shown")")
}
waitFor("the Stop pill", timeout: 1) { pill() != nil }
check(!overlayVisible(), "the overlay is gone while recording")
let p1 = pill()!
log("pill at \(p1.bounds), region \(region)")
check(!p1.bounds.intersects(region), "the pill sits outside the recorded region")
check(display.contains(p1.bounds), "the pill is on the display")
// Something to record.
NSWorkspace.shared.open(URL(fileURLWithPath: "/System/Applications/Calculator.app"))
pause(2.5)
click(CGPoint(x: p1.bounds.midX, y: p1.bounds.midY))
let path1 = waitForPath()
check(path1.hasPrefix("/"), "the clipboard holds an absolute path: \(path1)")
check(URL(fileURLWithPath: path1).lastPathComponent.range(of: #"^rec-\d{4}-\d{2}-\d{2}-\d{2}-\d{2}-\d{2}(-\d+)?\.mp4$"#, options: .regularExpression) != nil,
      "the file is named rec-YYYY-MM-DD-HH-MM-SS.mp4")
check(FileManager.default.fileExists(atPath: path1), "the file exists")
let v1 = videoInfo(path1)
log("video 1: \(v1.duration)s, \(v1.size)")
check(v1.duration > 1.8 && v1.duration < 8, "the video lasts about as long as the recording (\(v1.duration)s)")
let expectW = Double(Int(400 * scale) & ~1), expectH = Double(Int(300 * scale) & ~1)
let widthOK: Bool = abs(Double(v1.size.width) - expectW) <= 2
let heightOK: Bool = abs(Double(v1.size.height) - expectH) <= 2
check(widthOK && heightOK, "the video has the region's size at full sharpness (\(v1.size) vs \(expectW)x\(expectH))")
waitFor("the pill to go away after 'Path copied'", timeout: 6) { pill() == nil }

// 4: a click without dragging cancels.
pause(1)
NSPasteboard.general.clearContents()
pressShortcut()
waitFor("the overlay for the click test", timeout: 5, overlayVisible)
pause(0.5)
click(CGPoint(x: display.midX, y: display.midY))
waitFor("a click to cancel", timeout: 5) { !overlayVisible() }
pause(1)
check(pill() == nil && clipboard() == nil, "nothing was recorded")

// 5: the whole display; the pill is inside the region but not in the video.
pause(1)
pressShortcut()
waitFor("the overlay for the full-screen test", timeout: 5, overlayVisible)
pause(0.5)
drag(CGPoint(x: display.minX + 1, y: display.minY + 1), CGPoint(x: display.maxX - 1, y: display.maxY - 1))
waitFor("the Stop pill (full screen)", timeout: 8) { pill() != nil }
let p2 = pill()!
check(display.contains(p2.bounds), "the pill is inside the full-screen region")
pause(2.5)
// Grab the pill area just before stopping, from the last frames.
click(CGPoint(x: p2.bounds.midX, y: p2.bounds.midY))
let path2 = waitForPath()
let v2 = videoInfo(path2)
log("video 2: \(v2.duration)s, \(v2.size)")
let img = pixels(frame(v2.asset, at: max(0, v2.duration - 0.6)))
let sx = Double(img.width) / Double(display.width - 2), sy = Double(img.height) / Double(display.height - 2)
let px0 = Int((p2.bounds.minX - display.minX - 1) * sx), py0 = Int((p2.bounds.minY - display.minY - 1) * sy)
let px1 = Int((p2.bounds.maxX - display.minX - 1) * sx), py1 = Int((p2.bounds.maxY - display.minY - 1) * sy)
var red = 0, total = 0
for y in max(0, py0)..<min(img.height, py1) {
    for x in max(0, px0)..<min(img.width, px1) {
        let i = (y * img.width + x) * 4
        let r: Int = Int(img.data[i])
        let g: Int = Int(img.data[i + 1])
        let b: Int = Int(img.data[i + 2])
        total += 1
        if r > 200 && g < 110 && b < 100 { red += 1 }
    }
}
var nonBlack = 0
for i in stride(from: 0, to: img.data.count, by: 4 * 97) {
    let brightness: Int = Int(img.data[i]) + Int(img.data[i + 1]) + Int(img.data[i + 2])
    if brightness > 30 { nonBlack += 1 }
}
log("pill area in the video: \(total) px, \(red) stop-button red; frame has \(nonBlack) non-black samples")
check(nonBlack > 50, "the video shows the screen (not a black frame)")
check(total > 0 && red < max(4, total / 200), "the Stop pill is not in the video")
waitFor("the pill to go away", timeout: 6) { pill() == nil }

// 6: History has both recordings.
let historyFile = FileManager.default.homeDirectoryForCurrentUser
    .appendingPathComponent("Library/Application Support/com.alexhem.rec/history.json")
let history = (try? String(contentsOf: historyFile, encoding: .utf8)) ?? ""
check(history.contains(path1) && history.contains(path2), "History lists both recordings")

let quit = Process()
quit.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
quit.arguments = ["-e", "tell application id \"com.alexhem.rec\" to quit"]
try? quit.run()
quit.waitUntilExit()
log("PASS")
