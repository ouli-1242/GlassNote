#!/usr/bin/env bash
# Git Bash 环境下的 Tauri 启动包装。
#
# ── 为什么需要它 ──────────────────────────────────────────────────────────
#
# 在 Git Bash 里直接 `npm run tauri dev` 会遇到两个叠加的坑：
#
# 1) Git for Windows 在 <Git>\usr\bin 里带了一个 coreutils 的 link.exe（做硬链接的小工具），
#    而 Git Bash 把 /usr/bin 放在 PATH 很靠前的位置。rustc 调用 `link.exe` 时撞上它，
#    报 `link: extra operand`，并且附上一句误导性的「你可能需要安装 C++ 生成工具」。
#
# 2) 就算把 MSVC 的 bin 手动提到 PATH 最前面，也只会换一个错：
#    `LNK1181: 无法打开输入文件 "kernel32.lib"`。
#    因为链接器不只依赖 PATH，还要靠 LIB / INCLUDE 找到 Windows SDK 的库与头文件，
#    这些只有 vcvars64.bat 会设置。
#
# 所以正确做法是：先跑 vcvars64.bat 建立完整的 MSVC 开发环境，再执行命令。
# 用 PowerShell / CMD 的「Developer Command Prompt」时环境已配好，不需要本脚本。
#
# 用法：
#   ./scripts/dev.sh            # 等价于 npm run tauri dev
#   ./scripts/dev.sh build      # 等价于 npm run tauri build
#   ./scripts/dev.sh check      # 只跑 cargo check

set -euo pipefail

# ── 定位 vcvars64.bat ───────────────────────────────────────────────────
find_vcvars() {
  local roots=(
    "/d/Program Files/Microsoft Visual Studio/2022/Community"
    "/c/Program Files/Microsoft Visual Studio/2022/Community"
    "/c/Program Files/Microsoft Visual Studio/2022/Professional"
    "/c/Program Files/Microsoft Visual Studio/2022/Enterprise"
    "/d/Program Files/Microsoft Visual Studio/2022/Professional"
    "/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools"
    "/d/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools"
  )
  local root
  for root in "${roots[@]}"; do
    if [ -f "$root/VC/Auxiliary/Build/vcvars64.bat" ]; then
      printf '%s' "$root/VC/Auxiliary/Build/vcvars64.bat"
      return 0
    fi
  done
  return 1
}

if ! VCVARS="$(find_vcvars)"; then
  echo "错误：找不到 vcvars64.bat。" >&2
  echo "请安装 Visual Studio 2022 生成工具，并勾选「使用 C++ 的桌面开发」工作负载：" >&2
  echo "  https://visualstudio.microsoft.com/visual-cpp-build-tools/" >&2
  exit 1
fi

# ── 把 vcvars64 设置的环境变量导入当前 bash ──────────────────────────────
#
# 不去手工拼 LIB / INCLUDE：Windows SDK 的版本号与路径随 VS 更新变化，
# 硬编码必然过时。直接让 vcvars64 跑一遍、把它的环境读回来，是唯一不会失效的做法。
import_vcvars_env() {
  local win_path
  win_path="$(cygpath -w "$VCVARS" 2>/dev/null || printf '%s' "$VCVARS")"

  # `set` 会列出全部环境变量；只挑链接与编译真正需要的几项导入，
  # 全量导入会把 cmd 的临时变量也带进来，污染 bash 环境。
  cmd //c "\"$win_path\" >nul 2>&1 && set" 2>/dev/null | tr -d '\r' | while IFS='=' read -r key value; do
    case "$key" in
      PATH|LIB|INCLUDE|LIBPATH|WindowsSdkDir|WindowsSDKVersion|VCToolsInstallDir|VCINSTALLDIR|UCRTVersion|UniversalCRTSdkDir)
        printf 'export %s=%q\n' "$key" "$value"
        ;;
    esac
  done
}

echo "正在载入 MSVC 环境：$VCVARS"
eval "$(import_vcvars_env)"

# ── 补齐 Rust 工具链 ─────────────────────────────────────────────────────
# 优先用 rustup 的代理目录（标准布局），退回直接使用 toolchain 目录。
if [ -d "$HOME/.cargo/bin" ]; then
  export PATH="$HOME/.cargo/bin:$PATH"
else
  export PATH="$HOME/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin:$PATH"
  echo "提示：~/.cargo/bin 不存在，直接使用工具链目录。"
  echo "      如需 rustup 命令（rustup update 等），请另行安装 rustup。"
fi

# ── 自检 ─────────────────────────────────────────────────────────────────
resolved="$(command -v link.exe || true)"
case "$resolved" in
  */Git/usr/bin/*|/usr/bin/*)
    echo "错误：link.exe 仍解析到 Git 自带的版本：$resolved" >&2
    echo "vcvars64 载入后 PATH 顺序仍不对，请检查环境。" >&2
    exit 1
    ;;
  "") echo "错误：找不到 link.exe。" >&2; exit 1 ;;
esac

if [ -z "${LIB:-}" ]; then
  echo "错误：LIB 未设置，链接器将找不到 Windows SDK 的库。" >&2
  echo "vcvars64 环境导入失败，请改用方案 C（见 README 常见问题）。" >&2
  exit 1
fi

echo "链接器：$resolved"
echo "cargo：$(command -v cargo)"

cd "$(dirname "$0")/.."

case "${1:-dev}" in
  dev)   exec npm run tauri dev ;;
  build) exec npm run tauri build ;;
  check) exec cargo check --manifest-path src-tauri/Cargo.toml ;;
  *)     echo "未知参数：$1（可用：dev | build | check）" >&2; exit 2 ;;
esac
