import Foundation
import Vision
import AppKit

let args = CommandLine.arguments
guard args.count >= 3,
      let width = Int(args[1]),
      let height = Int(args[2]) else {
    fputs("Usage: macocr <width> <height>\n", stderr)
    exit(1)
}

// 从 standardInput 读取所有输入数据
var stdinData = Data()
let bufferSize = 4096
let stdin = FileHandle.standardInput
while true {
    let data = stdin.readData(ofLength: bufferSize)
    if data.isEmpty {
        break
    }
    stdinData.append(data)
}

guard stdinData.count == width * height * 4 else {
    fputs("Error: Input data size (\(stdinData.count)) does not match dimensions (\(width)x\(height)x4 = \(width * height * 4))\n", stderr)
    exit(1)
}

let colorSpace = CGColorSpaceCreateDeviceRGB()
// BGRA 在小端字节序（macOS 均为小端）下，对应 AlphaInfo.premultipliedFirst 与 byteOrder32Little 的组合
let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)

guard let provider = CGDataProvider(data: stdinData as CFData),
      let cgImage = CGImage(
          width: width,
          height: height,
          bitsPerComponent: 8,
          bitsPerPixel: 32,
          bytesPerRow: width * 4,
          space: colorSpace,
          bitmapInfo: bitmapInfo,
          provider: provider,
          decode: nil,
          shouldInterpolate: false,
          intent: .defaultIntent
      ) else {
    fputs("Error: Failed to create CGImage\n", stderr)
    exit(1)
}

let requestHandler = VNImageRequestHandler(cgImage: cgImage, options: [:])
let request = VNRecognizeTextRequest { (request, error) in
    if let error = error {
        fputs("Error: \(error.localizedDescription)\n", stderr)
        exit(1)
    }
    guard let observations = request.results as? [VNRecognizedTextObservation] else {
        return
    }
    let recognizedStrings = observations.compactMap { $0.topCandidates(1).first?.string }
    print(recognizedStrings.joined(separator: "\n"))
}

request.recognitionLanguages = ["zh-Hans", "zh-Hant", "en-US"]
request.recognitionLevel = .accurate

do {
    try requestHandler.perform([request])
} catch {
    fputs("Error: \(error.localizedDescription)\n", stderr)
    exit(1)
}
