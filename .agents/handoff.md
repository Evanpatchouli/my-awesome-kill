# 随心杀 P1 直接接棒

## 先核实现场

本地固定项目为 local / `56a3f74c-1f60-47c5-8e25-30ee6f6375cb` / `E:\my-awesome-kill`。先用自己的完整 `CODEX_THREAD_ID` 在 Codex 桌面精确匹配当前聊天，核对 hostId、projectId、cwd，再调用 `list_projects` 核实注册记录及实际 shell 工作目录。读 `.agents/current-task.md`、适用项目规范及用户最新入口 `C:\Users\evanpatchouli\.agents\AGENTS.md`，核对 `git status --short --branch` 和真实代码。无需翻旧聊天。若绑定不符，停止写入和传棒。

本任 `01a1f1c5-2033-74c1-a5ac-86be09da3a00` 从本地 `master` 的 `3208f89` 接棒，初始工作区干净。最后产品提交是 `8dad11d`，之后的验收工具、文档和交接为单独本地提交。没有用户独立改动需要挪动。目标仍是将固定 noname 的对局机制与可观察行为独立迁入通用 Rust 底座，以同一引擎交付经典玩法。现有 25 将、40 技、104 张实体、普通 2–10 席和原图须保留，普通入口保持经典。

## 第一项具体工作：装备栏

固定上游 `c73270948d72949aa80009536b9fd5400767d091` 已实跑普通/合并坐骑下的废除、恢复与扩展。接手后先运行：

```powershell
node scripts/reference-trace.mjs --scenario equip-slots --check tests/reference/equip-slots.json
```

1. 在普通八席注册内容中，通过真实 `Game::submit(Action::ActivateSkill)` 建立第一个装备栏用例。B 有一把武器、防具和一匹防御马、手中还有替换用武器。普通分马模式下初始五类启用数均为 1，已装三件后空栏为 `[0,0,0,1,1]`，可装备数仍为五个 1；武器不允许无替换再装，但允许替换。不得把四类已存在的标准实体装备当作全部五类槽。
2. A 的真实技能废除 B 的一个武器栏。先发生计数和公开占位的更新，再通过 `loseToDiscardpile` 和内层 `lose` 结算实际材料。分别取消外层/内层，核对已废除的栏、仍留在装备区的实物与有效登记。重复废除不再凭空增加禁用数或重复失牌，恢复只恢复栏位。上游源码入口是 `content.ts` 的 `disableEquip`、`enableEquip`、`expandEquip` 与 `player.js` 的四类计数和 `canEquip`。
3. 扩展第二个武器栏后两件可并存。废除一栏由来源 A 选择实际失去哪件；满栏装新武器则由持有者 B 选择替换项。重复槽参数与合马选项各有实测：合马时普通 `[3,4]` 请求扣一个合并栏，宝物栏即使为空也能单独禁用/恢复。覆盖真实玩家与 AI 的选择、失牌中断、当前有效装备、各席投影和全局守恒。 `tests/reference/equip-slots.json` 有 260 条观测、18 个状态和两组取消样例；现有样本只是基准。
4. `Game::equipment_registrations(seat,slot_of)` 给可信内容返回仍登记的材料引用，`SeatView.equipment` 只含仍在装备区的实体，`detachedEquipment` 仅公开牌种。普通单槽 `replaceEquip` 的 Before/Begin/End/After/Cancelled 已在真正装备帧上执行，父事件按真实可失去的旧实体和当时槽余量结束安装或弃牌。未来的容量、替换候选与选择接口应复用这个事件。不要用一份实体列表代替有效登记，也不要把已公开的空占位伪造成有实体 ID 的牌。

详细行为边界见 `docs/exec-plans/active/p1-engine-migration.md`，其余未覆盖单元见 `docs/exec-plans/active/noname-coverage.md`。装备栏之后继续其他牌区、修饰、联合选择、通用历史和模式契约。弥笃的判定区分支有原版观测，完整装备选择和活墨效果仍待验；酒杀恃勇尚需酒规则族。普通模式可玩和单个 feature 通过都不能宣布全底座完成。

## 本任交付及证据

- 复活在同一回调立即生效，事件版有真实的四阶段与各处取消。存活名单追加、物理环复接、负上限/负体力、已明身份、鼓舌的计数和显示、清理前复活、死者技能和来源奖励均有差分。 `5e262bd`、`e6d1c19`、`c87488f`。原版 78 条与后续 35 条共同观测，以及死亡主回调中的 11 个阶段。 `46d9544` 核对三张奖励按真实顺序放在旧手牌前。
- 休整分开保存生死、出局、时钟与 `undist`。自然/额外回合、多个候选、有限/无限时钟、保留死亡与已出局者的显式死亡、计时清理、伤害/拼点/响应途中离场、无行动者时暂停同一轮转均有回归。 `fb070b7`、`6707a71`。原版休整有 134 条请求或实际回调、56 个回合边界和 22 个状态；伤害中断另对照 110 条。真实殁亡用最后两名常侍，未把整套党锢及衍生技能计入完成。
- 判定区废除先改变逻辑标志，实际弃置子事件结束后再公开占位。取消可以留下原牌。真实支付、四层目标、单实体 `addJudge`、闪电绕回、失牌及清理中断、过河/顺手和死亡等均从实际动作验证。原版主样本 310 条；四个废区场景完整比较其中 200 条的共同字段。828 条补充观测中，十二局支付与中断比较 408 条共同观测；748 条支付样本中十局比较 324 条，另验清理、四种外部原区与同席转区。 `fc9c302`、`36b1a4e`、`a51df60`。
- 同席原青釭材料被获得到手、重新装备后，外层按原判定对象失去原材料，可能留下没有物理装备的有效武器登记。两种顺序、八卦黑杀、防具及借刀均实测；原版仁王只作额外取证。规则和界面区分可选择的实体与仍生效的装备。 `67da4b1`、`526f3cc`。虚拟判定使用独立 key；原材料移出后原判定仍可见/会阻止同名牌，选中逻辑判定会追踪同一实体。 `ed04361`、`8dad11d`。支持范围为单材料；不把多材料虚拟牌、任意历史接口或扩展装备槽宣称为完成。
- `2959e69` 改为物理环上一次计算距离。相对 `fb070b7` 的对照中，100 局 54,675 次决定的命令与 0 号席投影共 269,241,747 字节，SHA-256 完全相同。固定同一 P 核交替十对，规则 p95 中位 10.7 → 9.8 µs。此前 `f160124` 的 55,216 次决定与本次差异全部来自已在固定上游确认的反贼奖励手牌顺序。
- 冻结版 308 项库测试、20 项集成测试均通过；另跑忽略的 100 局八席，全部终局且守恒。33 项 TS、WASM conformance、10 条人机命令回放、独立浏览器重放、typecheck、生产构建、默认首页/画廊/牌桌、25 将与 104 张实体及 55 张原图均通过。`84e00fc` 的 13 个独立 feature 全部编译并在正常 Vue 入口实跑，普通模块在运行前后的哈希未改变。`cargo fmt --check` 和 diff 检查通过；`clippy --all-targets --all-features -- -D warnings` 未通过，对 `3208f89` 与冻结版本分别实际运行，两边均为相同的 27 项既有诊断。

## 本机与复验

冻结产品为 `8dad11d3bfd92ee416a2bf15aa9c0527b9c9936a`。发行 dist `E:\Temp\mak-p1-dist-8dad11d-01a1f1c5`，默认 WASM 为 1,431,427 字节，SHA-256 `D9889FE1C3E64DE16BD44014B4B950038BD5C85DCB432D96CE544A2B28E3F1F0`。证据总目录 `C:\Users\evanpatchouli\.codex\visualizations\2026\11\19\01a1f1c5-2033-74c1-a5ac-86be09da3a00\8dad11d`。每个非默认绑定及原始构建/浏览器日志在 `E:\Temp\mak-p1-matrix-8dad11d-01a1f1c5`，总目录内的 `features-summary.json` 已核实 13 项均通过。性能原始对照保留在 `E:\Temp\mak-perf-01a1f1c5` 和 `E:\Temp\mak-classic-diff`。

Windows release EXE `E:\Temp\my-awesome-kill-8dad11d.exe`，17,375,744 字节，SHA-256 `27FD09784EA1576471923ED9BCB7CA16F25AF71CC6BCFBFBCBE8DCD986A17AEA`。Windows 1280×720 的离线 WebView 已读取冻结 WASM。张飞进八席提交过河拆桥，版本 0→1；诸葛亮观星把 28 放底，86、85 入手，无 JS 异常。首个旧验证器未等首页位置稳定，实际点击错过入口；增强版 `E:\Temp\mak-p1-native-01a1f1c5\windows-verify.cjs` 等待坐标稳定且实际命中按钮，再重跑以上流程成功。

Android 最终 ARM64 debug 包的便捷路径为 `E:\Temp\my-awesome-kill-8dad11d-arm64-debug.apk`，与 `E:\Temp\mak-p1-native-01a1f1c5\artifacts\20261119-134227-c7bfeb53\app-arm64-debug.apk` 是同一制品。大小 135,263,231 字节；SHA-256 为 `CB06367339611B17B0911AD252E59624ECAC53505EC7D2391AAD9FBF9AAA7BD2`。已验证 v2、16 KiB 对齐，设备 base.apk 与发行包同哈希，包内 JNI 与新编译 ARM64 库均为 `69E7778418FE368B1BAD19B32A2874AC3F375984CC4EC3AA1BDF745E726459DC`。API 36 x86_64 隔离模拟器通过 ARM64 兼容层运行，设备 640×320 横屏，WebView 为 640×272。设备离线读取到同一默认 WASM，张飞八席过河拆桥 0→1，诸葛亮 28 放底、86/85 实际入手，无 JS 异常。见总证据目录下 `native/android/device-20261119-134313-e00a0e10` 与 `native/windows`。这次未验证物理 ARM 设备。

沿用 Windows 已装 Rust、pnpm 和 wasm-bindgen 版本，PATH 在 `docs/runbooks/development.md`。本任使用 `CARGO_BUILD_JOBS=2`、`CARGO_INCREMENTAL=0` 和既有 target。本机 Android SDK 为 `E:\dvp\Android\Sdk`、NDK `28.2.13676358`，离线 Gradle 工程在 WSL `/tmp/mak-apk-fast/project`。本任外置 `E:\Temp\mak-p1-native-01a1f1c5\android-native.ps1` 支持传入冻结 revision/freeze.json 后分 Build、Device 两步验证。重新核对受控 Android 源码、原生库和冻结 dist；Gradle 验签工具调用需将 Linux JDK 的 bin 放入 PATH。本次构建/设备核验的临时服务、隔离 WebView、专属 ADB、模拟器、Gradle 均已关闭。`C:\Work\Work.vhdx` 承载 E 盘；本任收口时 C/E 各约 58.7 / 185.9 GiB 可用，大型构建/模拟器前重新核对并错峰。

固定上游独立检出仍为 WSL Ubuntu `/tmp/mak-oracle-noname-c7327094`。将 `/home/evanpatchouli/.vite-plus/js_runtime/node/24.16.0/bin` 放入 WSL PATH，从 `packages/fs` 启动 `../../node_modules/.bin/tsx src/entry.ts --debug --dirname=../../apps/core`（8089），从 `apps/core` 启动 `node node_modules/vite/bin/vite.js dev --strictPort`（8081）。Windows 取证设 `REFERENCE_PLAYWRIGHT_PATH=E:\Temp\mak-oracle-tools-1.65.1\node_modules\playwright` 及 `REFERENCE_CHROMIUM_EXECUTABLE_PATH=C:\Program Files\Google\Chrome\Application\chrome.exe`。脚本使用真实上游操作和独立浏览器上下文，用完仅关闭自启服务。所有命令与已有样本见 `tests/reference/README.md`。

## 交接原因

本任最新有效采样为 2026-11-19T05:47:06.381Z，last_token_usage.total_tokens = 605,102，model_context_window = 828,400，占 73.04%。距 80% 余 57,618 个 token。在第一次冻结休整单元后的样本（331,383）与本次收口之间，活跃上下文实际增加 273,719，包括判定区、真实支付、独立登记和发行验收。下一单元需要同时处理装备容量、可替换性、多人选择、失牌中断以及独立牌桌回归。现在启动可能挤占交接空间，选择在有完整制品及可复跑样本的本地提交边界交接。

按 `.agents/current-task.md` 末尾的逐字传递规则，仅创建本项目的一个直接后继，明确受理后结束本会话。下一任从自己的最新有效样本开始计数。P1 整体验收完成后停止接力。
