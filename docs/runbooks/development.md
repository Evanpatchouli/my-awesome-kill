# 开发运行手册

## 环境与安装

需要 Node.js 20.19+ 或 22.12+（Vite 8 要求）、pnpm 10、Rust 稳定版及 Tauri 2 对应平台的系统依赖。规则引擎以 WebAssembly 随应用构建：还需安装 Rust 的 `wasm32-unknown-unknown` target，以及与 `packages/duel-web/Cargo.toml` 中版本一致的 `wasm-bindgen-cli`（当前为 0.2.130），并确保 `cargo` 和 `wasm-bindgen` 在 PATH 中。Android 开发还需安装 Android SDK、NDK 与 Java，并配置 Tauri 所要求的环境变量。无需额外的应用环境变量或服务。

首次配置 WebAssembly 工具链后，在仓库根目录执行：

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
pnpm install
pnpm dev          # 浏览器，http://localhost:1420
pnpm test         # 规则测试
pnpm typecheck
pnpm build
pnpm tauri dev    # 桌面应用
```

`pnpm dev`、`pnpm typecheck` 和 `pnpm build` 会先编译 Rust 引擎并生成 WASM 绑定；修改规则代码后重新运行相应命令。

Android 工程已初始化，可在 `apps/game` 运行 `pnpm tauri android dev`。Android manifest 使用横屏；重新初始化生成目录时须保留 `android:screenOrientation="landscape"`。打包和设备运行还需要对应平台 SDK 和签名配置。

桌面和 Android 使用应用数据目录中的 `matches.sqlite3` 保存完局战绩；以对局 id 去重。浏览器开发模式使用 localStorage，因此与原生应用的数据互不共享。

## 本机工具故障

若 Windows 的 pnpm 或 rustup shim 因装入点策略无法启动，应修复系统的执行/联接策略，或使用可信位置安装的 Node、pnpm 和 Rust 工具链。仅绕过 pnpm 启动器不能解决 pnpm workspace 联接同样被系统拒绝的问题。若只需检查 Rust 版本，可直接调用 rustup toolchain 的 `bin/cargo.exe` 和 `bin/rustc.exe`。不要提交本机生成的依赖目录和构建产物。

## 本机验证边界

Windows 上已验证前端生产构建、Rust `cargo check` 和 SQLite 重复对局 ID 的单元测试。前端构建后，可使用 `cargo build --release --features tauri/custom-protocol --manifest-path apps/game/src-tauri/Cargo.toml` 生成内置前端资源的程序，直接运行 `apps/game/src-tauri/target/release/my-awesome-kill.exe`。这不是安装包；首次构建仍需 Rust 和 Windows 构建工具。已在开发端口关闭时验证生产程序进入经典模式、25 将画廊并选择曹操进入牌桌。

本机的 pnpm shim 遇到系统不受信任装入点错误，临时验证时使用官方 pnpm 10.20.0 包内的 `bin/pnpm.cjs` 由 Node 启动，并通过 PATH 中的临时 `pnpm.cmd` 调用它；常规开发应修复或重装 pnpm。若 rustup shim 无法启动，可将已安装 toolchain 的 `bin` 加入 PATH，直接调用其中的 `cargo.exe`。这些方法只绕过本机损坏的启动器，不修复被系统拒绝的 workspace 目录联接。

Android 的 ARM64 调试包位于 `apps/game/src-tauri/gen/android/app/build/outputs/apk/arm64/debug/app-arm64-debug.apk`。已在隔离 WSL 工程中使用 Linux JDK、Gradle 和 Android SDK 打包，并核对 v2 签名、入口、横屏配置、原生库哈希和嵌入的 WASM。本机 Windows JDK 17、21、25 的 `Selector.open()` 均返回 `Invalid argument`，导致 Gradle 无法连接 daemon。Windows 上修改 Java 临时目录或使用 `--no-daemon` 均未解决该问题。

### 在 Windows 验证 ARM64 包

2026-11-18 用 API 36 Google Play x86_64 镜像的 ARM64 翻译实际运行了 SHA-256 为 `2657E4D9E067F34C3800A12FB31C9D9946F94E4F5F8BA13877356A975966BCD7` 的调试包。从首页进入经典选将，选择张飞，启动八人对局并观察到 AI 出牌。`dumpsys package` 确认为 `arm64-v8a`，Activity 确认为横屏。此次实跑使用交接时的包，供后续新制品回归时对照。

1. 为临时 AVD 设置独立的 `ANDROID_AVD_HOME`。选择两个空闲的模拟器控制与传输端口，另设独立的 `ANDROID_ADB_SERVER_PORT`。通过 Python `subprocess` 的 `env` 显式传递进程环境，将 SDK 下的 `emulator`、`emulator/lib64`、`emulator/lib64/qt/lib` 和 `emulator/qemu/windows-x86_64` 前置到 PATH。先执行 `emulator.exe -version` 核实返回 0。本机默认 PATH 曾导致 QEMU 返回 `0xC0000135`。
2. 本次使用 `emulator.exe -avd mak_sandbox -ports 5566,5567 -read-only -no-snapshot -no-window -no-audio -gpu swiftshader_indirect`。`-read-only` 模式仍会在临时 AVD 目录生成缓存。等 `adb -P 15037 -s emulator-5566 shell getprop sys.boot_completed` 返回 `1` 后，将 APK 安装到该 serial。初次引导和首次 WebView 显示均需要时间。
3. 从实际应用首页选择玩法、选将并入桌。用 `adb -P 15037 -s emulator-5566 exec-out screencap -p` 保存截图，并以 `dumpsys activity` 核对横屏。结束时向这个 serial 执行 `emu kill`，然后向对应的 ADB 端口执行 `kill-server`。
