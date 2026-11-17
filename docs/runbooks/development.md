# 开发运行手册

## 环境与安装

需要 Node.js 20.19+ 或 22.12+（Vite 8 要求）、pnpm 10、Rust 稳定版及 Tauri 2 对应平台的系统依赖。Android 开发还需安装 Android SDK、NDK 与 Java，并配置 Tauri 所要求的环境变量。无需额外的应用环境变量或服务。

在仓库根目录执行：

```sh
pnpm install
pnpm dev          # 浏览器，http://localhost:1420
pnpm test         # 规则测试
pnpm typecheck
pnpm build
pnpm tauri dev    # 桌面应用
```

Android 工程已初始化，可在 `apps/game` 运行 `pnpm tauri android dev`。Android manifest 使用横屏；重新初始化生成目录时须保留 `android:screenOrientation="landscape"`。打包和设备运行还需要对应平台 SDK 和签名配置。

桌面和 Android 使用应用数据目录中的 `matches.sqlite3` 保存完局战绩；以对局 id 去重。浏览器开发模式使用 localStorage，因此与原生应用的数据互不共享。

## 本机工具故障

若 Windows 的 pnpm 或 rustup shim 因装入点策略无法启动，应修复系统的执行/联接策略，或使用可信位置安装的 Node、pnpm 和 Rust 工具链。仅绕过 pnpm 启动器不能解决 pnpm workspace 联接同样被系统拒绝的问题。若只需检查 Rust 版本，可直接调用 rustup toolchain 的 `bin/cargo.exe` 和 `bin/rustc.exe`。不要提交本机生成的依赖目录和构建产物。

## 本机验证边界

Windows 上已验证前端生产构建、Rust `cargo check` 和 SQLite 重复对局 ID 的单元测试。前端构建后，可使用 `cargo build --release --features tauri/custom-protocol --manifest-path apps/game/src-tauri/Cargo.toml` 生成内置前端资源的程序，直接运行 `apps/game/src-tauri/target/release/my-awesome-kill.exe`。这不是安装包；首次构建仍需 Rust 和 Windows 构建工具。

本机的 pnpm shim 遇到系统不受信任装入点错误，临时验证时使用官方 pnpm 10.20.0 包内的 `bin/pnpm.cjs` 由 Node 启动，并通过 PATH 中的临时 `pnpm.cmd` 调用它；常规开发应修复或重装 pnpm。若 rustup shim 无法启动，可将已安装 toolchain 的 `bin` 加入 PATH，直接调用其中的 `cargo.exe`。这些方法只绕过本机损坏的启动器，不修复被系统拒绝的 workspace 目录联接。

Android 已验证 Tauri 生成工程、ARM64 Rust 库编译，并在隔离 WSL 工程中使用 Linux JDK、Gradle 和 Android SDK 将该库打入 debug APK；签名、入口、横屏配置及库哈希均已核对。尚未在设备或模拟器运行 APK。本机 JDK 17、21、25 的 `Selector.open()` 均在 Unix 域套接字连接时报 `Invalid argument`；Gradle 因此无法连接 daemon。普通 IPv4 loopback 可用，修改 Java 临时目录及 `--no-daemon` 均未解决。请在 Java NIO 选择器和 Gradle 正常的 Android 构建环境继续验证 APK 与设备横屏行为。
