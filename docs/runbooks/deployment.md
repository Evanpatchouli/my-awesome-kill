# 本地构建制品

先按[开发手册](development.md)准备工具链并在仓库根运行 `pnpm install`。

## Windows

在仓库根运行 `pnpm tauri build --no-bundle`。可执行文件为 `apps/game/src-tauri/target/release/my-awesome-kill.exe`，内置 Vue 静态资源。验收时关闭 1420 上的开发服务，打开 exe，检查「随心杀」首页的「开始游玩」与画廊的「经典三国杀」入口。原生窗口使用系统 WebView2。

## Android ARM64

在 `apps/game` 运行 `pnpm tauri android build --debug --apk --target aarch64`。该命令需要可用的 Java、SDK、NDK 和 ARM64 Rust target。P1 的本地调试签名制品位于 `src-tauri/gen/android/app/build/outputs/apk/arm64/debug/app-arm64-debug.apk`（相对游戏包）。

可用 SDK Build Tools 的 `apksigner verify --verbose` 检查签名，使用 `aapt dump badging` 检查包名 `com.mak.game`、应用名「随心杀」与入口 Activity。检查 manifest 的横屏声明、游戏分类和归档中的 `lib/arm64-v8a/libmy_awesome_kill_lib.so`。本次打包环境及设备验证边界见开发手册。
