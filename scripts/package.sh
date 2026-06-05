#!/bin/bash
# MiaoCR macOS Release Packaging Script
# Usage: ./scripts/package.sh

set -e

# Project root directory
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

echo -e "\033[36m========================================\033[0m"
echo -e "\033[36m  MiaoCR macOS Release Packager\033[0m"
echo -e "\033[36m========================================\033[0m"
echo ""

# 1. Build release
echo -e "\033[33m[1/3] Building release binary...\033[0m"
cargo build --release
echo -e "\033[32m  Build OK\033[0m"

# 2. Prepare .app structure
APP_DIR="dist/MiaoCR.app"
echo -e "\033[33m[2/3] Creating .app bundle structure: $APP_DIR\033[0m"
rm -rf dist/
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

# Copy binary
cp target/release/miaocr "$APP_DIR/Contents/MacOS/miaocr"
chmod +x "$APP_DIR/Contents/MacOS/miaocr"

# Copy icon
if [ -f "assets/icon.icns" ]; then
    cp assets/icon.icns "$APP_DIR/Contents/Resources/icon.icns"
    echo "  Icon copied to resources"
else
    echo -e "\033[33m  Warning: assets/icon.icns not found, skipping icon integration\033[0m"
fi

# Create Info.plist
cat <<EOF > "$APP_DIR/Contents/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>zh_CN</string>
    <key>CFBundleDisplayName</key>
    <string>喵OCR</string>
    <key>CFBundleExecutable</key>
    <string>miaocr</string>
    <key>CFBundleIconFile</key>
    <string>icon.icns</string>
    <key>CFBundleIdentifier</key>
    <string>com.tsingliu.miaocr</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>MiaoCR</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSScreenCaptureUsageDescription</key>
    <string>喵OCR需要屏幕录制权限以获取屏幕截图进行文字识别。</string>
</dict>
</plist>
EOF

# Codesign the app bundle (critical for macOS permissions to work correctly)
echo -e "\033[33mCodesigning the app bundle...\033[0m"
if command -v codesign &> /dev/null; then
    codesign --force --deep --sign - --requirements '=designated => identifier "com.tsingliu.miaocr"' "$APP_DIR"
    echo -e "\033[32m  Codesign completed successfully\033[0m"
else
    echo -e "\033[33m  Warning: codesign command not found, skipping codesigning\033[0m"
fi

echo -e "\033[32m  App bundle prepared successfully\033[0m"

# 3. Create DMG (Optional but very standard for macOS)
# We check if create-dmg or similar is available. If not, we just offer a ZIP package.
echo -e "\033[33m[3/3] Packaging to ZIP...\033[0m"
cd dist
zip -q -r MiaoCR-macOS.zip MiaoCR.app
cd ..

echo -e "\033[32m  ZIP file created: dist/MiaoCR-macOS.zip\033[0m"
echo ""
echo -e "\033[36m========================================\033[0m"
echo -e "\033[32m  Done! Output: $PROJECT_ROOT/dist\033[0m"
echo -e "\033[36m========================================\033[0m"
