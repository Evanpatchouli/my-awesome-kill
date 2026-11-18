# 随心杀 P1 直接接棒现场

## 接棒后先做

先用自己的完整 `CODEX_THREAD_ID` 在 Codex 桌面记录核对 `hostId=local`、`projectId=56a3f74c-1f60-47c5-8e25-30ee6f6375cb`、chat cwd 和实际 shell 目录 `E:\my-awesome-kill`，并调用 `list_projects` 复核 ID 对应路径。再读 `.agents/current-task.md`，核对 `git status --short --branch` 与当前代码。用户最近指定的 AGENTS 入口是 `C:\Users\evanpatchouli\.agents\AGENTS.md`。本地 `master`，交接前收口所有写入，无需翻旧聊天。

**第一项开发工作是 Q2b 的临时规则到期。** 从 `tests/reference/compare.json` 的平/负局读取 `phaseEnd`、`phaseAfter` 的请求与回调以及各时刻的锦囊合法性。复跑上游 `--scenario compare --check tests/reference/compare.json` 后，给正常经典 Turn/Phase 与可信登记内容写最小失败用例。使临时技能在 `phaseAfter` 请求时仍可观测，在其首次实际技能回调前已经到期。正常出牌选择和外部 `Game::submit` 都必须服从这个状态。到期只移除该次临时登记与修饰，已有永久技能及在途清理分别处理。

随后补齐同一黄金局的 `cardEnabled` 与 `useCard2`。下一张杀原本无法指定 D，但在巧说追加目标时通过 `targetEnabled2`，B、D 各扣一点；后续决斗只指定 B。平/负在结束前限制锦囊，回合末实际回调已解除。支付一次、保留父事件与实体、活席/固有限制重验、玩家选择和 AI 复用同一候选。对涉及持刀者和受杀者的借刀保留不同角色约束。实施 brief 已写入 `docs/exec-plans/active/p1-engine-migration.md` 的 Q2b 小节。完成一个可验证部分就本地提交。

## 可信边界和仍需的工作

目标是固定上游 `c73270948d72949aa80009536b9fd5400767d091` 的**整套对局机制与可观察行为**。独立 Rust/新内容接口，无需照搬旧 JS 脚本执行方式。正式首页/经典画廊、两侧入口、标准 25 将、40 技、104 张实体、2–10 席、桌面与 Android 横屏均为持续回归项。额外卡包仅作可复现的底座样本，应用无运行时 `.reference` 依赖。

- 通用技能 dispatcher、动态注册和实付费用已接经典技能。普通杀的可取消伤害帧与上层用牌清理、实际受伤回调看到的 HP 均有真实轨迹。最近一次修复前的“伤害请求前为 4”与实际 `damage` 回调已扣到 3 的观察不可互换。
- 单张重铸对照 377 条记录，真实铁索与法恩对照 106 行，体力子事件取消对照 76 行。已覆盖火/雷、全额/部分吸收、传播期间重验、翻面、死源和普通伤害记录。原多卡重铸、其他性质及更多角色生命周期仍待核实。
- 两席拼点及纵适已作为真实 Rust 内容能力落地。可用 `TriggerContext::compare`、`compare_result`、`Game::comparison`、`set_compare_number`、`gain_compare_card`。原版正常轨迹在自然第 2/3/4 回合，翻牌前私密；根取消只清理已进处理区的牌，取消后手失去时 B 的牌留手但仍计分。51/104 条是这次两席拼点段，后续巧说与临时修饰尚待 Q2b。网络断线代答、多人/牌堆拼点另列。
- 忠义验证见 `tests/reference/zhongyi-expansion.json`：受控双队中材料 h→x、加标签、限定技消耗，友方杀 1→2。下轮 `roundStart` 请求仍见 x，技能回调时已清理到 d，限定状态保留。X2 需接在临时清理语义之上。
- 其余 128 个 content 入口与文件以外的能力见 `docs/exec-plans/active/noname-coverage.md`。仍有额外阶段/回合、复杂选择、批量移动/区域/标记、更多修饰、复生/休整/明暗置、通用历史及命令重放等。P1 整体验收尚未通过。

## 可复现工具和验收

当前主机工具路径见 `docs/runbooks/development.md`。pnpm 10.20.0、wasm-bindgen 0.2.130 均以校验过的发行件缓存于 `%LOCALAPPDATA%/my-awesome-kill/tools`，Rust 使用 `%USERPROFILE%/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin`。用同一个 Cargo target；本次设置 `CARGO_INCREMENTAL=0` 与 `CARGO_BUILD_JOBS=2`。不需要再复制整个 node_modules、target 或 SDK。C 盘承载 `C:\Work\Work.vhdx`；本任处理过宿主空间不足导致开发盘下线，启动大型构建或模拟器应实际检查宿主可分配空间并错开高负载。

上游独立 WSL Ubuntu 检出 `/tmp/mak-oracle-noname-c7327094` 已确认版本，服务器在交接前已关闭。该目录仍在时，将 `/home/evanpatchouli/.vite-plus/js_runtime/node/24.16.0/bin` 前置 PATH，在 `packages/fs` 运行 `../../node_modules/.bin/tsx src/entry.ts --debug --dirname=../../apps/core`（8089），在 `apps/core` 运行 `node node_modules/vite/bin/vite.js dev --strictPort`（8081）。本机可用 `E:\Temp\mak-oracle-tools-1.65.1\node_modules\playwright` 配合 `C:\Program Files\Google\Chrome\Application\chrome.exe`。若临时目录不存在，按 `tests/reference/README.md` 从锁定检出恢复，勿在主仓参考目录安装依赖。

- `pnpm engine:test`、`pnpm test`（33 个 TS 基线）、`pnpm typecheck`、`pnpm build`、`node scripts/check-standard-content.mjs` 均通过。147 项常规 Rust release 测试通过，单独八席 100 局均终局、55,247 次决定，决策 p95 7.9 µs、投影+JSON p95 13.0 µs。SQLite 重复完局的原生单测通过。
- 从生产首页验证八席选将、出牌/响应、装备、判定、多选弃牌、动效、终局与重开。带非默认 Cargo features 的独立 WASM 工厂仅用于浏览器 QA。重铸、连环和拼点三条烟测均从真实牌桌执行，普通发行绑定没有测试工厂。脚本入口和三个实际种子见 `docs/runbooks/testing.md`。
- 最后功能提交 `bbe1f51`，普通发行 WASM SHA-256 `AFE033E354986D28ADF944DD1F86795AB0F1A0E699C10F1A3911FEA0DA67EC74`。Windows `apps/game/src-tauri/target/release/my-awesome-kill.exe` 为 `202947EEB2BACD373EB2D1BFC4F31740479A76275F2FF7A1375F88A73865DB98`，真实 WebView 加载同一哈希，从内置首页选张飞、八席、出牌推进版本 0→1。Android `apps/game/src-tauri/gen/android/app/build/outputs/apk/arm64/debug/app-arm64-debug-bbe1f51.apk` 为 `7FFB92DC72134B238943AA0EDEA1A1615D9B0C3CC333C88B533A16CEF8F072FA`，144,530,551 字节，v2 验签、16 KiB zip 对齐及包内原生库/压缩资源已核对。全新独立 API 36 ARM64 翻译模拟器在飞行模式安装此最终包，设备内 base.apk 及从 WebView 实际读取的 WASM 均核对了哈希。从首页选曹操入八人局，点闪并确认响应万箭齐发，手牌 4→3、弃牌堆 7→8。640×320 下可用内容为 640×272，七名对手用横向列表浏览。截图位于下述 android-final/。本机没有持有该模拟器或 ADB 的待处理任务。
- 本任界面与设备证据均在 `C:/Users/evanpatchouli/.codex/visualizations/2026/11/17/01a1eb7c-8b2d-7a50-9d09-3f57dcd9bc0e/`。本任设备实测使用隔离的模拟器与专属 ADB 端口。

功能、验证和实施 brief 已分别本地提交，交接前的文档提交为 `a15b189`。当前没有独立用户未提交改动。C 盘保留恢复时的代码/补丁快照；从 `d7b45b8` 起的本轮已提交增量另存于 `C:\Users\evanpatchouli\.codex\backups\my-awesome-kill\p1-20261119-01a1eb7c-source.bundle`，需配合同一基线还原。

## 交接原因

本任最新有效采样为 2026-11-18T17:23:38.128Z，last_token_usage.total_tokens = 645,904，model_context_window = 828,400，占 77.97%。距 80% 还有 16,816 个 token。 当前功能单元、复核和在途写入均已收口。Q2b 将同时跨越 Turn/Phase 生命周期、触发注册、临时修饰、物理/转化卡合法性、真实多目标用牌，以及对应的 WASM 交互和独立复核。按本任前两组跨层单元的实际消耗，启动它可能挤占指定交接空间，因此在两席拼点能力可独立验证的边界提前交班。后继继续同一授权，并传递 `.agents/current-task.md` 的完整项目绑定条款。
