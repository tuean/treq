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
  <!-- 隐私用途说明：工作区默认在 ~/Documents/treq，缺这条时 macOS 会在
       运行时拦截访问（open() 阻塞）并弹「想访问文稿」——启动看起来就是卡死。
       同一个原因，桌面上放着工作区也要 NSDesktopFolderUsageDescription。 -->
  <key>NSDocumentsFolderUsageDescription</key>
  <string>treq 需要读取「文稿」里的工作区（默认 ~/Documents/treq：集合、请求、环境变量都是普通文件）。</string>
  <key>NSDesktopFolderUsageDescription</key>
  <string>工作区放在桌面时需要读取桌面文件。</string>
  <key>NSDownloadsFolderUsageDescription</key>
  <string>导入接口文件（Postman / OpenAPI / HAR）或保存响应到「下载」时需要访问下载文件夹。</string>
</dict>
</plist>
PLIST

# ad-hoc 签名：本机自用足够，避免 “已损坏” 提示
codesign --force --sign - "$APP" >/dev/null 2>&1 || true

echo "built $APP"
echo "open:  open $APP"
