#!/bin/bash

# RunBot Desktop 图标生成脚本
# 需要安装: brew install librsvg imagemagick

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SVG_FILE="$SCRIPT_DIR/icon-design.svg"
OUTPUT_DIR="$SCRIPT_DIR"

echo "🎨 RunBot Desktop 图标生成器"
echo "================================"

# 检查依赖
if ! command -v rsvg-convert &> /dev/null; then
    echo "❌ 需要安装 librsvg: brew install librsvg"
    exit 1
fi

if ! command -v convert &> /dev/null; then
    echo "❌ 需要安装 imagemagick: brew install imagemagick"
    exit 1
fi

# 生成 PNG 图标
echo "📦 生成 PNG 图标..."

# 标准尺寸
rsvg-convert -w 32 -h 32 "$SVG_FILE" -o "$OUTPUT_DIR/32x32.png"
rsvg-convert -w 128 -h 128 "$SVG_FILE" -o "$OUTPUT_DIR/128x128.png"
rsvg-convert -w 256 -h 256 "$SVG_FILE" -o "$OUTPUT_DIR/128x128@2x.png"
rsvg-convert -w 1024 -h 1024 "$SVG_FILE" -o "$OUTPUT_DIR/icon.png"

# Windows Logo 尺寸
rsvg-convert -w 30 -h 30 "$SVG_FILE" -o "$OUTPUT_DIR/Square30x30Logo.png"
rsvg-convert -w 44 -h 44 "$SVG_FILE" -o "$OUTPUT_DIR/Square44x44Logo.png"
rsvg-convert -w 71 -h 71 "$SVG_FILE" -o "$OUTPUT_DIR/Square71x71Logo.png"
rsvg-convert -w 89 -h 89 "$SVG_FILE" -o "$OUTPUT_DIR/Square89x89Logo.png"
rsvg-convert -w 107 -h 107 "$SVG_FILE" -o "$OUTPUT_DIR/Square107x107Logo.png"
rsvg-convert -w 142 -h 142 "$SVG_FILE" -o "$OUTPUT_DIR/Square142x142Logo.png"
rsvg-convert -w 150 -h 150 "$SVG_FILE" -o "$OUTPUT_DIR/Square150x150Logo.png"
rsvg-convert -w 284 -h 284 "$SVG_FILE" -o "$OUTPUT_DIR/Square284x284Logo.png"
rsvg-convert -w 310 -h 310 "$SVG_FILE" -o "$OUTPUT_DIR/Square310x310Logo.png"
rsvg-convert -w 50 -h 50 "$SVG_FILE" -o "$OUTPUT_DIR/StoreLogo.png"

echo "✅ PNG 图标生成完成"

# 生成 macOS icns
echo "🍎 生成 macOS icns..."
ICONSET_DIR="$OUTPUT_DIR/icon.iconset"
mkdir -p "$ICONSET_DIR"

rsvg-convert -w 16 -h 16 "$SVG_FILE" -o "$ICONSET_DIR/icon_16x16.png"
rsvg-convert -w 32 -h 32 "$SVG_FILE" -o "$ICONSET_DIR/icon_16x16@2x.png"
rsvg-convert -w 32 -h 32 "$SVG_FILE" -o "$ICONSET_DIR/icon_32x32.png"
rsvg-convert -w 64 -h 64 "$SVG_FILE" -o "$ICONSET_DIR/icon_32x32@2x.png"
rsvg-convert -w 128 -h 128 "$SVG_FILE" -o "$ICONSET_DIR/icon_128x128.png"
rsvg-convert -w 256 -h 256 "$SVG_FILE" -o "$ICONSET_DIR/icon_128x128@2x.png"
rsvg-convert -w 256 -h 256 "$SVG_FILE" -o "$ICONSET_DIR/icon_256x256.png"
rsvg-convert -w 512 -h 512 "$SVG_FILE" -o "$ICONSET_DIR/icon_256x256@2x.png"
rsvg-convert -w 512 -h 512 "$SVG_FILE" -o "$ICONSET_DIR/icon_512x512.png"
rsvg-convert -w 1024 -h 1024 "$SVG_FILE" -o "$ICONSET_DIR/icon_512x512@2x.png"

iconutil -c icns "$ICONSET_DIR" -o "$OUTPUT_DIR/icon.icns"
rm -rf "$ICONSET_DIR"
echo "✅ macOS icns 生成完成"

# 生成 Windows ico
echo "🪟 生成 Windows ico..."
convert "$OUTPUT_DIR/icon.png" -define icon:auto-resize=256,128,64,48,32,16 "$OUTPUT_DIR/icon.ico"
echo "✅ Windows ico 生成完成"

echo ""
echo "🎉 所有图标生成完成！"
echo "================================"
ls -la "$OUTPUT_DIR"/*.png "$OUTPUT_DIR"/*.icns "$OUTPUT_DIR"/*.ico 2>/dev/null
