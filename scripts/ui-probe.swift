// Drives the running app with real input events, and screenshots it safely.
//
// Why this exists: rustafari is built without accesskit, so it exposes no
// accessibility tree. AppleScript's `click at` goes through those APIs and is
// silently swallowed — which is how two interaction bugs (an unusable scale
// slider, and tool names that ignored clicks) shipped past every screenshot,
// test and lint we have. Posting CGEvents is the only way to exercise the UI
// the way a person does.
//
// Driven by scripts/ui-probe.sh; see there for usage.

import CoreGraphics
import Foundation

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("ui-probe: \(message)\n".utf8))
    exit(1)
}

func number(_ index: Int) -> Double {
    let args = CommandLine.arguments
    guard index < args.count, let value = Double(args[index]) else {
        fail("expected a number at argument \(index)")
    }
    return value
}

func post(_ type: CGEventType, _ at: CGPoint) {
    CGEvent(
        mouseEventSource: nil, mouseType: type, mouseCursorPosition: at, mouseButton: .left
    )?.post(tap: .cghidEventTap)
}

/// Screen coordinates of the frontmost sizeable window owned by `app`.
func window(of app: String) -> (id: Int, x: Int, y: Int, width: Int, height: Int) {
    let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
    guard let windows = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]]
    else {
        fail("could not read the window list (grant Screen Recording permission)")
    }
    for entry in windows {
        guard let owner = entry[kCGWindowOwnerName as String] as? String, owner == app,
            let id = entry[kCGWindowNumber as String] as? Int,
            let bounds = entry[kCGWindowBounds as String] as? [String: Any],
            let x = bounds["X"] as? Double, let y = bounds["Y"] as? Double,
            let width = bounds["Width"] as? Double, let height = bounds["Height"] as? Double,
            // Skip shadows and other tiny helper windows.
            width > 200, height > 200
        else { continue }
        return (id, Int(x), Int(y), Int(width), Int(height))
    }
    fail("no window found for \"\(app)\" — is it running?")
}

switch CommandLine.arguments.dropFirst().first {
case "window":
    guard let app = CommandLine.arguments.dropFirst(2).first else { fail("usage: window <app>") }
    let w = window(of: app)
    print("\(w.id) \(w.x) \(w.y) \(w.width) \(w.height)")

case "click":
    let at = CGPoint(x: number(2), y: number(3))
    // Move first: egui decides what is hovered from the pointer position it
    // last saw, so a press with no preceding move can land on nothing.
    post(.mouseMoved, at)
    usleep(120_000)
    post(.leftMouseDown, at)
    usleep(90_000)
    post(.leftMouseUp, at)

case "drag":
    let from = CGPoint(x: number(2), y: number(3))
    let to = CGPoint(x: number(4), y: number(5))
    post(.mouseMoved, from)
    usleep(150_000)
    post(.leftMouseDown, from)
    usleep(150_000)
    // Many small steps, so widgets that track incremental drag deltas (the
    // pane splitter, the sliders) see a realistic gesture rather than a jump.
    let steps = 24
    for step in 1...steps {
        let t = Double(step) / Double(steps)
        post(
            .leftMouseDragged,
            CGPoint(x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t))
        usleep(40_000)
    }
    usleep(200_000)
    post(.leftMouseUp, to)

default:
    fail("usage: ui-probe (window <app> | click <x> <y> | drag <x1> <y1> <x2> <y2>)")
}
