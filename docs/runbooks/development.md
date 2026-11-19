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

### 本机构建空间预算

本机可在当前 PowerShell 进程复用固定 Cargo target，并限制增量缓存和并行度。依赖已经缓存时，使用离线、锁定的 release 构建；缺少依赖时离线构建会失败，需先准备缓存。

```powershell
$env:CARGO_TARGET_DIR = 'C:\Users\evanpatchouli\AppData\Local\my-awesome-kill\p1-cargo-target'
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_BUILD_JOBS = '2'
$env:CARGO_NET_OFFLINE = 'true'
cargo build --offline --locked --release --manifest-path packages/duel-web/Cargo.toml --target wasm32-unknown-unknown
```

引擎构建及 feature-table QA 脚本也会使用 `CARGO_TARGET_DIR`；未设置时沿用 `packages/duel-web/target`。构建前后同时监控 C 盘剩余空间、固定 target 目录大小和承载开发盘的 VHDX 文件大小。删除 E 盘文件不能保证 C 盘空间立即归还，VHDX 的实际回收取决于宿主和虚拟磁盘管理。

在本机 PowerShell 7.2+ 中，`./scripts/test-core-budget.ps1` 运行核心库及集成测试，`-TestFilter 名称` 可只运行指定测试。它在独立进程中设置上述缓存和并发选项，每秒采样一次。缓存达到 2 GiB、C 盘余量低于 120 GiB、单次 C 盘余量减少 3 GiB 或工作盘镜像增长 256 MiB 时，停止此次测试进程树并报告。阈值基于采样，短时写入可能越过阈值。其他直接调用 Cargo 的命令需要另行监控。

Android 工程已初始化，可在 `apps/game` 运行 `pnpm tauri android dev`。Android manifest 使用横屏；重新初始化生成目录时须保留 `android:screenOrientation="landscape"`。打包和设备运行还需要对应平台 SDK 和签名配置。

桌面和 Android 使用应用数据目录中的 `matches.sqlite3` 保存完局战绩；以对局 id 去重。浏览器开发模式使用 localStorage，因此与原生应用的数据互不共享。

## 本机工具故障

若 Windows 的 pnpm 或 rustup shim 因装入点策略无法启动，应修复系统的执行/联接策略，或使用可信位置安装的 Node、pnpm 和 Rust 工具链。仅绕过 pnpm 启动器不能解决 pnpm workspace 联接同样被系统拒绝的问题。若只需检查 Rust 版本，可直接调用 rustup toolchain 的 `bin/cargo.exe` 和 `bin/rustc.exe`。不要提交本机生成的依赖目录和构建产物。

本机已验证的 pnpm 10.20.0 来自核过 npm 签名与 SHA-512 的发行包，wasm-bindgen 0.2.130 来自核过 SHA-256 的官方 Windows 构建。已有这两份工具缓存时，可在当前 PowerShell 进程使用以下路径运行项目命令。

```powershell
$stableBin = Join-Path $env:USERPROFILE '.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin'
$toolBin = Join-Path $env:LOCALAPPDATA 'my-awesome-kill/tools'
$env:PATH = "$toolBin/pnpm-10.20.0;$toolBin/wasm-bindgen-0.2.130;$stableBin;$env:PATH"
pnpm --version
wasm-bindgen --version
```

## 本机验证边界

Windows 上已验证前端生产构建、Rust `cargo check` 和 SQLite 重复对局 ID 的单元测试。前端构建后，可使用 `cargo build --release --features tauri/custom-protocol --manifest-path apps/game/src-tauri/Cargo.toml` 生成内置前端资源的程序，直接运行 `apps/game/src-tauri/target/release/my-awesome-kill.exe`。这不是安装包；首次构建仍需 Rust 和 Windows 构建工具。已在开发端口关闭时验证生产程序进入经典模式、25 将画廊并选择曹操进入牌桌。

本机的 pnpm shim 遇到系统不受信任装入点错误，临时验证时使用官方 pnpm 10.20.0 包内的 `bin/pnpm.cjs` 由 Node 启动，并通过 PATH 中的临时 `pnpm.cmd` 调用它；常规开发应修复或重装 pnpm。若 rustup shim 无法启动，可将已安装 toolchain 的 `bin` 加入 PATH，直接调用其中的 `cargo.exe`。这些方法只绕过本机损坏的启动器，不修复被系统拒绝的 workspace 目录联接。

Android 的 ARM64 调试包位于 `apps/game/src-tauri/gen/android/app/build/outputs/apk/arm64/debug/app-arm64-debug.apk`。已在隔离 WSL 工程中使用 Linux JDK、Gradle 和 Android SDK 打包，并核对 v2 签名、入口、横屏配置、原生库哈希和嵌入的 WASM。本机 Windows JDK 17、21、25 的 `Selector.open()` 均返回 `Invalid argument`，导致 Gradle 无法连接 daemon。Windows 上修改 Java 临时目录或使用 `--no-daemon` 均未解决该问题。

### 在 Windows 验证 ARM64 包

2026-11-18 用 API 36 Google Play x86_64 镜像的 ARM64 翻译实际运行了 SHA-256 为 `2657E4D9E067F34C3800A12FB31C9D9946F94E4F5F8BA13877356A975966BCD7` 的调试包。从首页进入经典选将，选择张飞，启动八人对局并观察到 AI 出牌。`dumpsys package` 确认为 `arm64-v8a`，Activity 确认为横屏。此次实跑使用交接时的包，供后续新制品回归时对照。

1. 为临时 AVD 设置独立的 `ANDROID_AVD_HOME`。选择两个空闲的模拟器控制与传输端口，另设独立的 `ANDROID_ADB_SERVER_PORT`。通过 Python `subprocess` 的 `env` 显式传递进程环境，将 SDK 下的 `emulator`、`emulator/lib64`、`emulator/lib64/qt/lib` 和 `emulator/qemu/windows-x86_64` 前置到 PATH。先执行 `emulator.exe -version` 核实返回 0。本机默认 PATH 曾导致 QEMU 返回 `0xC0000135`。
2. 本次使用 `emulator.exe -avd mak_sandbox -ports 5566,5567 -read-only -no-snapshot -no-window -no-audio -gpu swiftshader_indirect`。`-read-only` 模式仍会在临时 AVD 目录生成缓存。等 `adb -P 15037 -s emulator-5566 shell getprop sys.boot_completed` 返回 `1` 后，将 APK 安装到该 serial。初次引导和首次 WebView 显示均需要时间。
3. 从实际应用首页选择玩法、选将并入桌。用 `adb -P 15037 -s emulator-5566 exec-out screencap -p` 保存截图，并以 `dumpsys activity` 核对横屏。结束时向这个 serial 执行 `emu kill`，然后向对应的 ADB 端口执行 `kill-server`。

系统栏修复后的 ARM64 调试包 SHA-256 为 `7EB2B285608FC40CE6C26DFEC7FF616672363E44AB3DD03A61A16361B6BFE68A`，已通过 Android v2 验签。用 API 36、160 dpi、640×320 的隔离模拟器实跑，状态栏与导航栏各占 24 px，Activity 将中间 640×272 交给 WebView。飞行模式下从首页选张飞进入八人局，实际点选过河拆桥、目标并确认，版本从 0 变为 1。WebView 根滚动量为 0，手牌和确认按钮下缘分别位于其内容区的 268px 和 264px。此包使用 `a6cd3bf` 的规则和 `d7b45b8` 的短横屏布局。

### 包含两席拼点的本地构建

`bbe1f51` 的 Windows release 程序位于 `apps/game/src-tauri/target/release/my-awesome-kill.exe`，SHA-256 为 `202947EEB2BACD373EB2D1BFC4F31740479A76275F2FF7A1375F88A73865DB98`。关闭本地开发与预览服务后，使用隔离 WebView2 资料从 `http://tauri.localhost` 实际选张飞、进入八人局并指定目标出牌。WebView 读取的内嵌 WASM SHA-256 为 `AFE033E354986D28ADF944DD1F86795AB0F1A0E699C10F1A3911FEA0DA67EC74`。

同一生产构建的 Android 调试包位于 `apps/game/src-tauri/gen/android/app/build/outputs/apk/arm64/debug/app-arm64-debug-bbe1f51.apk`，SHA-256 为 `7FFB92DC72134B238943AA0EDEA1A1615D9B0C3CC333C88B533A16CEF8F072FA`。完整包为 144,530,551 字节。通过 v2 验签和 16 KiB zip 对齐检查，工件内实际 ARM64 库的 WASM 和 CSS 与最终 dist 的哈希分别一致。使用已有 Gradle 工程时，重新生成 APK 封装输出可避免旧原生库在增量 ZIP 中形成大块空洞。

新包也在独立 API 36 模拟器上实际安装过。设备内 `base.apk` 和 WebView 实际加载的 WASM 都与以上 SHA-256 一致。飞行模式下从首页滚动选将页，选曹操并进入八人局。在万箭齐发响应中点击闪并确认，手牌 4→3，弃牌堆 7→8。640×320 屏幕下 WebView 占据 `[0,24]–[640,296]`，手牌和按钮在 640×272 可用范围内；七个对手席位以横向列表呈现，右缘可见部分末席。

在文件支持的 Windows 开发盘上运行构建和模拟器时，应同时查看承载 VHDX 的宿主卷。模拟器冷启动、Windows 页面文件和编译目录都可能占用同一个物理盘。复用 Cargo target，并在一次隔离 AVD 验证后关闭该实例及其专属 ADB 服务器，可减少重复写入和并发磁盘占用。

2026-11-19 另从提交 `042c6df` 的默认发行资源重建并实测两端。Windows 程序 SHA-256 为 `3470DC8E3AE33B4DB2A2ABC9348D2188CBA05A03F1B4424EAC249A5FDF9EFF1A`。Android ARM64 调试包 SHA-256 为 `A6AF768146548523FE7ABBD9212D2AE3AE5CEA0ECA383F2E0626FF385E2EE2E0`，大小 135,172,351 字节，v2 验签、16 KiB 对齐和设备内 APK 哈希均已核对。两端离线选张飞进入八人局并提交过河拆桥，Windows 版本由 0 变为 1，Android 由 0 变为 5。运行中 WebView 读到的 WASM 均为 `E04FF567BABC6D2CAC7E015FDF2451B3285EDD185436AF2EDC3F00C41A8223B3`。Android 的 `frontendDist` 覆盖应使用相对配置文件的路径；本次直接填入 Windows 盘符绝对路径时被 Tauri 解析为 URL。核对实际页面中的 WASM 和可交互状态能发现这种打包错误。

### 包含有序分区和体力上限的离线制品

`f160124` 的 Windows release 程序为 17,352,704 字节，SHA-256 为 `62FF61FDD9333A013E04BFEAA07552682AF4ADE14CA5AEE25F1CB7AB90E1A39B`。ARM64 debug APK 为 144,642,295 字节，SHA-256 为 `7D21BC45FFD8FABFABB3BCC3CFBBA7933D5BC441B2EA6111B9AF26D8B0D9CE93`。已核对 v2 签名、16 KiB 对齐和设备的 base.apk。包内、设备内与新编译的 ARM64 库哈希均为 `FDC0BFE36E4B55B81DED451E33833D0DE58489DAB32BF91B7F07EDE2BEA1401D`。

分别在 1280×720 的 Windows WebView2 和 API 36 x86_64 模拟器的 ARM64 兼容环境中离线运行。设备处于横屏，640×320 显示面板中的 WebView 边界为 (0,24)–(640,296)。两个宿主从正在运行的页面读取到同一份 WASM，SHA-256 为 `23C64D2FC44DF8109E1C9A16E9C632089AD1470E5125FD5735B7076A0DE1B9AD`。从首页选择张飞进入八席后，过河拆桥的提交令版本由 0 变为 1。返回选将后用诸葛亮发动观星，实体 28 移至底行，顶行的 86、85 随后实际进入手牌。两组牌和确认按钮均在各端可用视口内，两次验证均无 JS 异常。

复用独立 Gradle 工程时，对照目标 Git 版本核验参与打包的 manifest、Kotlin Activity、资源和 Gradle 配置，再放入新编译的原生库。本次首个候选沿用了旧版 `MainActivity.kt`，虽然 WASM 和原生库哈希正确，可用视口却为 640×320。同步系统栏内边距代码后重新打包，在设备中实测为 640×272。

### 休整、判定区与独立登记的离线制品

`8dad11d` 的默认 WASM 为 1,431,427 字节，SHA-256 为 `D9889FE1C3E64DE16BD44014B4B950038BD5C85DCB432D96CE544A2B28E3F1F0`。Windows release EXE 为 17,375,744 字节，SHA-256 为 `27FD09784EA1576471923ED9BCB7CA16F25AF71CC6BCFBFBCBE8DCD986A17AEA`。ARM64 debug APK 为 135,263,231 字节，SHA-256 为 `CB06367339611B17B0911AD252E59624ECAC53505EC7D2391AAD9FBF9AAA7BD2`。包内 JNI 与新编译的 ARM64 库均为 `69E7778418FE368B1BAD19B32A2874AC3F375984CC4EC3AA1BDF745E726459DC`，安装包通过 v2 签名和 16 KiB 对齐验证。

在开发及预览服务关闭后，Windows WebView2 与 API 36 x86_64 模拟器中的 ARM64 应用均已从运行页面离线读取到上述 WASM。Windows 视口为 1280×720，Android 横屏显示面板为 640×320，WebView 为 640×272。两端均从首页选张飞进八席，提交过河拆桥后版本由 0 变为 1。重新选诸葛亮，观星将 28 置底，86、85 随后进入手牌，两份验收均无 JavaScript 异常。Windows 首页有入场动效，坐标点击应等待按钮位置稳定，并用 `elementFromPoint` 确认命中。Android 采用模拟器的 ARM64 兼容层，本次未覆盖物理 ARM 设备。
