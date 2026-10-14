#!/usr/bin/env swift
import AppKit
import Foundation

// Draw the Grimoire disk image background. Finder places the app icon and
// the Applications shortcut over the lower half, so the wide logo stays above them.
// Usage: dmg-background.swift <logo.png> <1x.png> <2x.png> <width> <height> <app-x> <applications-x>

let args = CommandLine.arguments
guard args.count == 8 else {
    fputs("usage: dmg-background.swift <logo.png> <1x.png> <2x.png> <width> <height> <app-x> <applications-x>\n", stderr)
    exit(1)
}

let logoURL = URL(fileURLWithPath: args[1])
let out1x = args[2]
let out2x = args[3]
guard let width = Int(args[4]), let height = Int(args[5]),
      let appX = Double(args[6]), let applicationsX = Double(args[7]),
      width > 0, height > 0 else {
    fputs("width, height, and icon positions must be numbers\n", stderr)
    exit(1)
}
guard let source = NSImage(contentsOf: logoURL), source.size.width > 0, source.size.height > 0 else {
    fputs("could not read logo \(logoURL.path)\n", stderr)
    exit(1)
}

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> NSColor {
    let red = CGFloat((hex >> 16) & 0xFF) / 255
    let green = CGFloat((hex >> 8) & 0xFF) / 255
    let blue = CGFloat(hex & 0xFF) / 255
    return NSColor(srgbRed: red, green: green, blue: blue, alpha: alpha)
}

// Quick Look pads a wide SVG into a square with white. Drop that padding so the
// logo keeps its own aspect ratio.
func trimmedLogo(_ image: NSImage) -> NSImage {
    guard let rep = NSBitmapImageRep(data: image.tiffRepresentation ?? Data()) else {
        return image
    }
    let pixelsWide = rep.pixelsWide
    let pixelsHigh = rep.pixelsHigh

    func isPadding(_ x: Int, _ y: Int) -> Bool {
        guard let sample = rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else {
            return true
        }
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 0
        sample.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return alpha < 0.05 || (red > 0.97 && green > 0.97 && blue > 0.97)
    }

    var minX = pixelsWide
    var minY = pixelsHigh
    var maxX = 0
    var maxY = 0
    for y in 0..<pixelsHigh {
        for x in 0..<pixelsWide where !isPadding(x, y) {
            minX = min(minX, x)
            minY = min(minY, y)
            maxX = max(maxX, x)
            maxY = max(maxY, y)
        }
    }
    guard maxX > minX + 8, maxY > minY + 8 else {
        return image
    }

    // Drop the anti-aliased edge left where the logo's background met the
    // white padding Quick Look adds around a wide SVG.
    let crop = NSRect(x: minX + 3, y: minY + 3, width: maxX - minX - 5, height: maxY - minY - 5)
    guard let cropped = NSBitmapImageRep(
        bitmapDataPlanes: nil,
        pixelsWide: Int(crop.width),
        pixelsHigh: Int(crop.height),
        bitsPerSample: 8,
        samplesPerPixel: 4,
        hasAlpha: true,
        isPlanar: false,
        colorSpaceName: .deviceRGB,
        bytesPerRow: 0,
        bitsPerPixel: 0
    ) else {
        return image
    }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: cropped)
    image.draw(
        in: NSRect(x: 0, y: 0, width: crop.width, height: crop.height),
        from: crop,
        operation: .copy,
        fraction: 1
    )
    NSGraphicsContext.restoreGraphicsState()

    let result = NSImage(size: NSSize(width: crop.width, height: crop.height))
    result.addRepresentation(cropped)
    return result
}

let logo = trimmedLogo(source)

func render(scale: Int, destination: String) {
    let pixelsWide = width * scale
    let pixelsHigh = height * scale
    guard let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil,
        pixelsWide: pixelsWide,
        pixelsHigh: pixelsHigh,
        bitsPerSample: 8,
        samplesPerPixel: 4,
        hasAlpha: true,
        isPlanar: false,
        colorSpaceName: .deviceRGB,
        bytesPerRow: 0,
        bitsPerPixel: 0
    ) else {
        fputs("could not create background bitmap\n", stderr)
        exit(1)
    }

    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let scale = CGFloat(scale)
    let canvas = NSRect(x: 0, y: 0, width: CGFloat(width) * scale, height: CGFloat(height) * scale)

    color(0x1A1714).setFill()
    NSBezierPath(rect: canvas).fill()

    let logoWidth = canvas.width * 0.86
    let logoHeight = logoWidth * (logo.size.height / logo.size.width)
    let logoRect = NSRect(
        x: (canvas.width - logoWidth) / 2,
        y: canvas.height - logoHeight - 28 * scale,
        width: logoWidth,
        height: logoHeight
    )
    logo.draw(in: logoRect, from: .zero, operation: .sourceOver, fraction: 1)

    // Finder draws icon names in black on a picture background, and it will not
    // keep a custom label color. A light chip under each name keeps that text readable.
    func labelChip(centerX: CGFloat, chipWidth: CGFloat) {
        let chipHeight = 32 * scale
        let rect = NSRect(
            x: centerX * scale - chipWidth * scale / 2,
            y: 48 * scale,
            width: chipWidth * scale,
            height: chipHeight
        )
        color(0xEFE6D8).setFill()
        NSBezierPath(roundedRect: rect, xRadius: 8 * scale, yRadius: 8 * scale).fill()
    }
    labelChip(centerX: CGFloat(appX), chipWidth: 112)
    labelChip(centerX: CGFloat(applicationsX), chipWidth: 132)

    NSGraphicsContext.restoreGraphicsState()

    guard let png = rep.representation(using: .png, properties: [:]) else {
        fputs("could not encode background\n", stderr)
        exit(1)
    }
    do {
        try png.write(to: URL(fileURLWithPath: destination))
    } catch {
        fputs("could not write \(destination): \(error)\n", stderr)
        exit(1)
    }
}

render(scale: 1, destination: out1x)
render(scale: 2, destination: out2x)
