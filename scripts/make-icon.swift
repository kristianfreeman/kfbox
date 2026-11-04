// Rasterises kfbox's flat app icon (the fallback for macOS before 26) with
// CoreGraphics, from the same Lissajous formula as scripts/make-icon.mjs.
// Called by that script; by hand:
//   swift scripts/make-icon.swift <out.png> <size> <radius> <stroke>
// <radius> and <stroke> are fractions of the squircle, which takes the
// standard macOS grid: 824/1024 of the canvas, centred, transparent margin.

import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let args = CommandLine.arguments
guard args.count == 5, let size = Int(args[2]), let radius = Double(args[3]),
  let strokeFrac = Double(args[4])
else {
  FileHandle.standardError.write("usage: make-icon.swift <out.png> <size> <radius> <stroke>\n".data(using: .utf8)!)
  exit(2)
}

let s = Double(size)
let space = CGColorSpace(name: CGColorSpace.sRGB)!
let ctx = CGContext(
  data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0, space: space,
  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
// Top-left origin, like the SVG.
ctx.translateBy(x: 0, y: s)
ctx.scaleBy(x: 1, y: -1)

func rgb(_ hex: UInt32) -> CGColor {
  CGColor(
    colorSpace: space,
    components: [
      CGFloat((hex >> 16) & 0xff) / 255, CGFloat((hex >> 8) & 0xff) / 255, CGFloat(hex & 0xff) / 255, 1,
    ])!
}

// Squircle: a superellipse (n = 5), close to Apple's continuous-corner tile.
let tile = s * 824 / 1024
let c = s / 2
let squircle = CGMutablePath()
let n = 5.0
for i in 0..<720 {
  let a = Double(i) / 720 * 2 * Double.pi
  let ca = cos(a), sa = sin(a)
  let x = c + tile / 2 * (ca < 0 ? -1 : 1) * pow(abs(ca), 2 / n)
  let y = c + tile / 2 * (sa < 0 ? -1 : 1) * pow(abs(sa), 2 / n)
  if i == 0 { squircle.move(to: CGPoint(x: x, y: y)) } else { squircle.addLine(to: CGPoint(x: x, y: y)) }
}
squircle.closeSubpath()

let top = c - tile / 2, bottom = c + tile / 2

// Soft drop shadow under the tile, as macOS icons have.
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: s * 10 / 1024), blur: s * 20 / 1024, color: rgb(0x000000).copy(alpha: 0.28))
ctx.addPath(squircle)
ctx.setFillColor(rgb(0xffffff))
ctx.fillPath()
ctx.restoreGState()

// Background: #ffffff (top) to #e4e4ea (bottom).
ctx.saveGState()
ctx.addPath(squircle)
ctx.clip()
let bg = CGGradient(colorsSpace: space, colors: [rgb(0xffffff), rgb(0xe4e4ea)] as CFArray, locations: [0, 1])!
ctx.drawLinearGradient(bg, start: CGPoint(x: c, y: top), end: CGPoint(x: c, y: bottom), options: [])
ctx.restoreGState()

// The figure: x = sin(3t + pi/2), y = sin(2t), one closed stroke.
let r = tile * radius
let figure = CGMutablePath()
let samples = 2000
for i in 0..<samples {
  let t = Double(i) / Double(samples) * 2 * Double.pi
  let p = CGPoint(x: c + r * sin(3 * t + Double.pi / 2), y: c + r * sin(2 * t))
  if i == 0 { figure.move(to: p) } else { figure.addLine(to: p) }
}
figure.closeSubpath()

let w = tile * strokeFrac
ctx.saveGState()
ctx.addPath(figure)
ctx.setLineWidth(w)
ctx.setLineCap(.round)
ctx.setLineJoin(.round)
ctx.replacePathWithStrokedPath()
ctx.clip()
// Stroke gradient over the figure's height: #2a2a33 (top) to #0c0c10 (bottom).
let ink = CGGradient(colorsSpace: space, colors: [rgb(0x2a2a33), rgb(0x0c0c10)] as CFArray, locations: [0, 1])!
ctx.drawLinearGradient(ink, start: CGPoint(x: c, y: c - r - w / 2), end: CGPoint(x: c, y: c + r + w / 2), options: [])
ctx.restoreGState()

let url = URL(fileURLWithPath: args[1])
let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(dest, ctx.makeImage()!, nil)
guard CGImageDestinationFinalize(dest) else { exit(1) }
