// compose SOURCE.png OUT.jpg "Headline" "Subline" [panel]
//
// Lays one of the app's off-screen pictures on a 2880 x 1800 canvas with a caption: the size
// and kind of picture App Store Connect takes for a Mac screenshot. See screenshots.sh.
import AppKit
import ImageIO
import UniformTypeIdentifiers

let arguments = CommandLine.arguments
let source = NSImage(contentsOfFile: arguments[1])!.cgImage(forProposedRect: nil, context: nil, hints: nil)!
let isPanel = arguments.count > 5 && arguments[5] == "panel"
let (width, height) = (2880, 1800)
let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!

// Background: a dark slope of colour, with a soft glow where the picture goes.
let colours = [CGColor(red: 0.13, green: 0.16, blue: 0.27, alpha: 1), CGColor(red: 0.04, green: 0.05, blue: 0.08, alpha: 1)] as CFArray
context.drawLinearGradient(CGGradient(colorsSpace: nil, colors: colours, locations: [0, 1])!, start: CGPoint(x: 0, y: height), end: CGPoint(x: width, y: 0), options: [])
let glow = [CGColor(red: 0.45, green: 0.55, blue: 0.95, alpha: 0.28), CGColor(red: 0.45, green: 0.55, blue: 0.95, alpha: 0)] as CFArray
context.drawRadialGradient(CGGradient(colorsSpace: nil, colors: glow, locations: [0, 1])!, startCenter: CGPoint(x: 2130, y: 900), startRadius: 0, endCenter: CGPoint(x: 2130, y: 900), endRadius: 1100, options: [])

// The picture, on the right, as large as leaves it room to breathe.
let fit = min(1, 1500 / CGFloat(source.height), 1280 / CGFloat(source.width))
let size = CGSize(width: CGFloat(source.width) * fit, height: CGFloat(source.height) * fit)
let place = CGRect(x: 2130 - size.width / 2, y: 900 - size.height / 2, width: size.width, height: size.height)
let corner = 22 * 4 * fit
context.saveGState()
context.setShadow(offset: CGSize(width: 0, height: -36), blur: 90, color: CGColor(gray: 0, alpha: 0.55))
if isPanel {
    // A panel's picture is a plain rectangle; on screen it has the window's round corners.
    context.addPath(CGPath(roundedRect: place, cornerWidth: corner, cornerHeight: corner, transform: nil))
    context.setFillColor(CGColor(gray: 0.12, alpha: 1))
    context.fillPath()
    context.restoreGState()
    context.saveGState()
    context.addPath(CGPath(roundedRect: place, cornerWidth: corner, cornerHeight: corner, transform: nil))
    context.clip()
}
context.interpolationQuality = .high
context.draw(source, in: place)
context.restoreGState()
if isPanel {
    context.addPath(CGPath(roundedRect: place.insetBy(dx: 1, dy: 1), cornerWidth: corner, cornerHeight: corner, transform: nil))
    context.setStrokeColor(CGColor(gray: 1, alpha: 0.14))
    context.setLineWidth(2)
    context.strokePath()
}

// The caption, on the left.
NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
let headline = NSAttributedString(string: arguments[3], attributes: [
    .font: NSFont.systemFont(ofSize: 118, weight: .bold), .foregroundColor: NSColor.white, .kern: -1.5,
])
let subline = NSAttributedString(string: arguments[4], attributes: [
    .font: NSFont.systemFont(ofSize: 54, weight: .regular), .foregroundColor: NSColor.white.withAlphaComponent(0.72),
])
let column = CGSize(width: 1180, height: 1400)
let top = headline.boundingRect(with: column, options: .usesLineFragmentOrigin)
let bottom = subline.boundingRect(with: column, options: .usesLineFragmentOrigin)
let gap: CGFloat = 44
let total = top.height + gap + bottom.height
let y = 900 + total / 2
headline.draw(with: CGRect(x: 210, y: y - top.height, width: column.width, height: top.height), options: .usesLineFragmentOrigin)
subline.draw(with: CGRect(x: 214, y: y - top.height - gap - bottom.height, width: column.width, height: bottom.height), options: .usesLineFragmentOrigin)

let destination = CGImageDestinationCreateWithURL(URL(fileURLWithPath: arguments[2]) as CFURL, UTType.jpeg.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(destination, context.makeImage()!, [kCGImageDestinationLossyCompressionQuality: 0.92] as CFDictionary)
precondition(CGImageDestinationFinalize(destination))
