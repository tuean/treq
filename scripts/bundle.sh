#!/usr/bin/env bash
# 打包成最小 macOS .app（Dock 图标的来源）：
#   - 裸二进制直接跑不会显示自定义图标，必须包成 .app 由 LaunchServices 启动
#   - 顺手把进程名 / 菜单栏名变成 treq（Info.plist 的 CFBundleName）
#
# 用法：scripts/bundle.sh [debug|release]     默认 release
#       打开：open target/treq.app
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE="${1:-release}"
case "$PROFILE" in
  debug)   cargo build -p treq-app ;;
  release) cargo build --release -p treq-app ;;
  *) echo "usage: $0 [debug|release]" >&2; exit 2 ;;
esac

# 图标缺了就现场生成
if [ ! -f assets/AppIcon.icns ]; then
  python3 scripts/make_icon.py
fi

APP="target/treq.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/$PROFILE/treq-app" "$APP/Contents/MacOS/treq"
cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>treq</string>
  <key>CFBundleDisplayName</key><string>treq</string>
  <key>CFBundleIdentifier</key><string>com.treq.app</string>
  <key>CFBundleExecutable</key><string>treq</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

# ad-hoc 签名：本机自用足够，避免 “已损坏” 提示
codesign --force --sign - "$APP" >/dev/null 2>&1 || true

echo "built $APP"
echo "open:  open $APP"
