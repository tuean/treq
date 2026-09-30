#!/usr/bin/env bash
# treq 打包 + 安装到 Applications，并在更新后自动重装。
#
# 用法：
#   scripts/install_app.sh                    # release 打包 → /Applications/treq.app
#   scripts/install_app.sh --debug            # 用 debug 构建（迭代快）
#   scripts/install_app.sh --app-dir DIR      # 装到其它目录（如 ~/Applications）
#   scripts/install_app.sh --force            # 应用正在运行也覆盖安装
#   scripts/install_app.sh --watch            # 监听源码改动，更新后自动重装（应用运行时跳过，关闭后自动补装）
#   scripts/install_app.sh --hooks            # 安装 git post-commit/post-merge/post-rewrite 钩子
#   scripts/install_app.sh --agent            # 安装 LaunchAgent（登录后后台 watch 自动更新）
#   scripts/install_app.sh --no-build         # 跳过构建，直接装 target/treq.app
#   scripts/install_app.sh --quiet            # 安静模式（供 hook/agent 调用）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PROFILE=release
APP_DIR=/Applications
APP_NAME=treq.app
FORCE=0
WATCH=0
INTERVAL=2
INSTALL_HOOKS=0
INSTALL_AGENT=0
NO_BUILD=0
QUIET=0

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
  case "$1" in
    --debug) PROFILE=debug ;;
    --release) PROFILE=release ;;
    --app-dir) APP_DIR="${2:?--app-dir needs a value}"; shift ;;
    --app-dir=*) APP_DIR="${1#*=}" ;;
    --force) FORCE=1 ;;
    --watch) WATCH=1 ;;
    --interval) INTERVAL="${2:?--interval needs a value}"; shift ;;
    --interval=*) INTERVAL="${1#*=}" ;;
    --hooks) INSTALL_HOOKS=1 ;;
    --agent) INSTALL_AGENT=1 ;;
    --no-build) NO_BUILD=1 ;;
    --quiet) QUIET=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

# git hook / launchd 的 PATH 往往没有 ~/.cargo/bin
if ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

log() { [ "$QUIET" = 1 ] || echo "$@"; }
die() { echo "treq install: $*" >&2; exit 1; }

app_running() {
  pgrep -x treq >/dev/null 2>&1 \
    || pgrep -x treq-app >/dev/null 2>&1 \
    || pgrep -f "$APP_DIR/Contents/MacOS/treq" >/dev/null 2>&1
}

build_bundle() {
  if [ "$NO_BUILD" = 1 ]; then
    [ -d "target/treq.app" ] || die "target/treq.app 不存在，去掉 --no-build 重新构建"
    return 0
  fi
  log "==> 构建 ($PROFILE) 并打包 .app"
  ./scripts/bundle.sh "$PROFILE"
}

# 构建 + 安装必须串行：bundle.sh 会先 rm -rf target/treq.app，
# 并发的 watch / hook / 手动运行会读到半成品，之前就因此装坏过。
LOCK_DIR="$ROOT/target/.install_app.lock"
release_lock() { rm -rf "$LOCK_DIR"; }
with_lock() {
  local tries=0
  while ! mkdir "$LOCK_DIR" 2>/dev/null; do
    tries=$((tries + 1))
    if [ "$tries" -ge 600 ]; then
      die "等待另一个安装进程超时（$LOCK_DIR）"
    fi
    sleep 0.2
  done
  echo "$$" > "$LOCK_DIR/pid"
  trap release_lock EXIT INT TERM
  "$@"
  release_lock
  trap - EXIT INT TERM
}

do_build_install() {
  build_bundle
  install_app
}

install_once() {
  with_lock do_build_install
}

install_app() {
  local src="target/treq.app"
  local dest="$APP_DIR/$APP_NAME"
  [ -d "$src" ] || die "找不到 $src"
  [ -x "$src/Contents/MacOS/treq" ] || die "$src 不完整（构建可能被中断）"
  if app_running && [ "$FORCE" != 1 ]; then
    log "treq 正在运行，跳过安装（关闭应用后重跑，或加 --force）"
    return 0
  fi
  [ -d "$APP_DIR" ] || mkdir -p "$APP_DIR" || die "无法创建 $APP_DIR"
  [ -w "$APP_DIR" ] || die "$APP_DIR 不可写；试试 sudo，或 --app-dir ~/Applications"
  local tmp="$APP_DIR/.$APP_NAME.new.$$"
  log "==> 安装到 $dest"
  rm -rf "$tmp"
  ditto "$src" "$tmp"
  [ -x "$tmp/Contents/MacOS/treq" ] || { rm -rf "$tmp"; die "复制结果不完整"; }
  rm -rf "$dest"
  mv "$tmp" "$dest"
  xattr -dr com.apple.quarantine "$dest" 2>/dev/null || true
  log "已安装：$dest  （打开：open \"$dest\"）"
}

snapshot() {
  find "$ROOT/Cargo.toml" "$ROOT/Cargo.lock" "$ROOT/crates" "$ROOT/scripts" "$ROOT/assets" \
    -type f \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' -o -name '*.sh' \
    -o -name '*.py' -o -name '*.png' -o -name '*.icns' -o -name '*.svg' \) \
    -exec stat -f '%m:%z:%N' {} + 2>/dev/null | sort
}

install_hooks() {
  local hooks_dir="$ROOT/.git/hooks"
  [ -d "$hooks_dir" ] || die "不是 git 仓库（找不到 .git/hooks）"
  local marker="# treq auto-install hook"
  for hook in post-commit post-merge post-rewrite; do
    local f="$hooks_dir/$hook"
    if [ -f "$f" ] && ! grep -qF "$marker" "$f"; then
      cp "$f" "$f.bak.$(date +%Y%m%d%H%M%S)"
      log "已备份原有 $hook"
    fi
    cat > "$f" <<EOF
#!/usr/bin/env bash
$marker
# 每次提交/合并/变基后，后台把最新代码打包安装到 Applications。
# 应用正在运行时脚本会跳过，等下次没开时再装。
nohup "$ROOT/scripts/install_app.sh" --quiet >/dev/null 2>&1 &
EOF
    chmod +x "$f"
    log "已安装 hook: $hook"
  done
}

install_agent() {
  local plist="$HOME/Library/LaunchAgents/com.treq.autoupdate.plist"
  mkdir -p "$HOME/Library/LaunchAgents"
  cat > "$plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.treq.autoupdate</string>
  <key>ProgramArguments</key>
  <array>
    <string>$ROOT/scripts/install_app.sh</string>
    <string>--watch</string>
    <string>--quiet</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>/tmp/treq-autoupdate.log</string>
  <key>StandardErrorPath</key><string>/tmp/treq-autoupdate.err</string>
</dict>
</plist>
EOF
  launchctl unload "$plist" 2>/dev/null || true
  launchctl load "$plist"
  log "已安装 LaunchAgent: $plist（日志 /tmp/treq-autoupdate.log）"
}

if [ "$INSTALL_HOOKS" = 1 ]; then
  install_hooks
fi
if [ "$INSTALL_AGENT" = 1 ]; then
  install_agent
fi

if [ "$WATCH" = 1 ]; then
  log "==> 监听源码改动（每 ${INTERVAL}s 检查，Ctrl-C 退出）"
  install_once
  last="$(snapshot)"
  pending=0
  while true; do
    sleep "$INTERVAL"
    now="$(snapshot)"
    if [ "$now" != "$last" ]; then
      last="$now"
      pending=1
      log "==> 检测到源码改动"
    fi
    if [ "$pending" = 1 ]; then
      if app_running && [ "$FORCE" != 1 ]; then
        log "treq 正在运行，等它关闭后自动更新"
      else
        install_once
        pending=0
      fi
    fi
  done
fi

install_once
