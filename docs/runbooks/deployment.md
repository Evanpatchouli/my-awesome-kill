# 本地构建制品

先按[开发手册](development.md)准备工具链并在仓库根运行 `pnpm install`。

## 两个平台一起发布

在 Windows x64 主机的仓库根运行 `pnpm build:release`。需要 MSVC、Rust 的 `x86_64-pc-windows-msvc`、`aarch64-linux-android` 和 `wasm32-unknown-unknown` target、`wasm-bindgen`、JDK、Android SDK/NDK 和 Build Tools。`ANDROID_HOME`（或 `ANDROID_SDK_ROOT`）应指向 Android SDK。

版本只从根 `package.json` 的 `version` 读取，格式为 `MAJOR.MINOR.PATCH`，无需逐个修改 Tauri、Cargo 或 Gradle 的版本字段。Android 的版本号由同一值计算为 `MAJOR × 1000000 + MINOR × 1000 + PATCH`（minor、patch 均小于 1000）。

在 `apps/game/src-tauri/gen/android/keystore.properties`（UTF-8）配置 `storeFile`、`storePassword`、`keyAlias`、`keyPassword`。密钥保存在仓库外，不提交密码文件；Windows 路径可写成 `C:/路径/文件.jks`。没有发布签名时，可以运行 `pnpm build:release --local`：仍以 release 模式构建，但 APK 使用 Android 调试密钥并带 `-local` 后缀，仅供本机安装测试。Windows 产物是可直接运行的 EXE，不生成安装包，也不做 Authenticode 签名。

完成后两份文件一起进入 `release/<version>/`：

- `my-awesome-kill-<version>-windows-x64.exe`
- `my-awesome-kill-<version>-android-arm64.apk`（本机签名为 `my-awesome-kill-<version>-android-arm64-local.apk`）

脚本验证 EXE 的产品版本，以及 APK 的包名、版本、ARM64 架构、非调试模式和签名。构建失败不会留下仅包含一个平台的发布目录；不会覆盖已存在的版本目录。重新发布同一版本前，先移走原目录。若构建被强制终止，确认没有构建仍在运行后才可移除 `release/.build.lock`。Android 不允许普通安装覆盖签名不同或 versionCode 更高的旧包，可用干净的测试设备或用户配置验证新包。

## 单平台开发验证

- Windows：`pnpm tauri build --no-bundle`。关掉开发服务后运行原生 EXE，确认可以离线进入经典对局。
- Android：`pnpm tauri android build --debug --apk --target aarch64`。这是独立的调试流程，不会输出到发布目录。
