// Render a page in WebKit (the engine Tauri uses on macOS) and save a PNG. Used by scripts/check_overlay_frame.sh and
// scripts/check_pin_frame.sh.
// usage: webkit_snap <url|html file> <out.png> <width> <height> [inject.js] [delay s]
// env: DPR=1|2 forces the device scale factor; LIVE=<x> shows the window on screen at x and captures it as
// composited (screencapture -l) instead of a software snapshot; TRANSPARENT=1 mimics the overlay window; FIT=1
// resizes the window to the size the page asked for before the snapshot (pins lay out their own window).
import Cocoa
import WebKit
let a = CommandLine.arguments
let (w, h) = (CGFloat(Double(a[3])!), CGFloat(Double(a[4])!))
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
class D: NSObject, WKNavigationDelegate {
  func webView(_ v: WKWebView, didFinish n: WKNavigation!) {
    DispatchQueue.main.asyncAfter(deadline: .now() + (a.count > 6 ? Double(a[6])! : 0.4)) {
      if ProcessInfo.processInfo.environment["FIT"] != nil { self.fit(v); return }
      self.snap(v)
    }
  }
  /// FIT=1: resize the window to the size the page asked for (window.__fit() → "w,h", see scripts/dev/pin_mock.js),
  /// print window.__report(), then snapshot — so whatever the real window would clip is clipped here too.
  func fit(_ v: WKWebView) {
    v.evaluateJavaScript("window.__fit ? window.__fit() : ''") { r, _ in
      let p = ((r as? String) ?? "").split(separator: ",").compactMap { Double($0) }
      guard p.count == 2 else { print("err: the page did not size its window"); exit(1) }
      win.setContentSize(NSSize(width: ceil(p[0]), height: ceil(p[1])))
      v.frame = NSRect(x: 0, y: 0, width: ceil(p[0]), height: ceil(p[1]))
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) {
        v.evaluateJavaScript("window.__report ? window.__report() : ''") { r, _ in
          print("report \((r as? String) ?? "")")
          self.snap(v)
        }
      }
    }
  }
  func snap(_ v: WKWebView) {
    if ProcessInfo.processInfo.environment["LIVE"] != nil {
      // capture the window as composited on screen (not a software snapshot)
      let p = Process(); p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
      p.arguments = ["-x", "-o", "-l\(win.windowNumber)", a[2]]; try! p.run(); p.waitUntilExit()
      print("live captured"); exit(0)
    }
    let c = WKSnapshotConfiguration()
    v.takeSnapshot(with: c) { img, err in
      guard let img = img, let t = img.tiffRepresentation, let r = NSBitmapImageRep(data: t) else { print("err \(String(describing: err))"); exit(1) }
      try! r.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: a[2]))
      print("ok \(r.pixelsWide)x\(r.pixelsHigh) scale \(NSScreen.main?.backingScaleFactor ?? 0)")
      exit(0)
    }
  }
}
let live = ProcessInfo.processInfo.environment["LIVE"] != nil
let win = NSWindow(contentRect: NSRect(x: live ? Double(ProcessInfo.processInfo.environment["LIVE"]!)! : -10000, y: live ? 40 : -10000, width: w, height: h), styleMask: [.borderless], backing: .buffered, defer: false)
if live { win.orderFrontRegardless() }
if ProcessInfo.processInfo.environment["TRANSPARENT"] != nil { win.isOpaque = false; win.backgroundColor = .clear }
let cfg = WKWebViewConfiguration()
if a.count > 5, let js = try? String(contentsOfFile: a[5], encoding: .utf8) {
  cfg.userContentController.addUserScript(WKUserScript(source: js, injectionTime: .atDocumentStart, forMainFrameOnly: true))
}
let v = WKWebView(frame: NSRect(x: 0, y: 0, width: w, height: h), configuration: cfg)
if let d = ProcessInfo.processInfo.environment["DPR"], let f = Double(d) {
  // private WebKit SPI, test-only: render as if on a 1x / 2x display
  let sel = NSSelectorFromString("_setOverrideDeviceScaleFactor:")
  guard v.responds(to: sel) else { print("DPR override unavailable"); exit(2) }
  typealias F = @convention(c) (AnyObject, Selector, CGFloat) -> Void
  unsafeBitCast(v.method(for: sel), to: F.self)(v, sel, CGFloat(f))
}
win.contentView = v
if ProcessInfo.processInfo.environment["TRANSPARENT"] != nil { v.setValue(false, forKey: "drawsBackground") }
let d = D()
v.navigationDelegate = d
if a[1].hasPrefix("http") { v.load(URLRequest(url: URL(string: a[1])!)) } else { v.loadFileURL(URL(fileURLWithPath: a[1]), allowingReadAccessTo: URL(fileURLWithPath: "/")) }
app.run()
