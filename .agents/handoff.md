# 随心杀 P1 直接接棒

## 第一项开发工作

用本任完整 `CODEX_THREAD_ID` 在桌面查自己的 chat，核对 hostId、projectId、cwd，再用 `list_projects` 核实 `local / 56a3f74c-1f60-47c5-8e25-30ee6f6375cb / E:\my-awesome-kill` 和实际 shell 目录。若归属错误，停止写入并报告。主分支为 `master`。交班前收口所有写入和测试。无需翻旧聊天；项目规范入口、用户授权和原样传递的项目绑定规则在 `.agents/current-task.md`。

**下一项是 R3b 的批量重铸根及其移动。** 先在独立固定上游复跑以下命令，再给 Rust 八人桌的可信主动技能写最小失败用例。使用真实 `Game::submit(Action::ActivateSkill)` 同时选择两张手牌和一件装备。

```powershell
node scripts/reference-trace.mjs --scenario recast-batch --check tests/reference/recast-batch.json
node scripts/reference-trace.mjs --scenario recast-batch-boundaries --check tests/reference/recast-batch-boundaries.json
```

- 在失牌前把原数组第二张暗手牌交给 B。两个工厂各只调用一次，均收到原来的三张。失牌包装保留原选择，实际内层 `loseAfter` 只有第一张手牌与原装备，保持输入顺序和实际 h/e 来源。随后建一个 `draw(3)` 及一个 `gain`。物理实体、活动座选择、他席知情和终态一起核对。
- 整个失牌包装取消时没有内层失牌历史。若三张全在内层失牌前被转走，原内层以空的实际 cards 正常执行 End/After，stockcards 仍是原三张，且有失牌历史。两者都能完成重铸并按原数量摸牌。取消 `drawBefore` 不改变顶牌顺序。显式 `gain2` 与普通暗摸应分别验证。两个时机请求夹着工厂调用，实际技能回调与子移动依原先的队列顺序执行。
- 实现从 `packages/duel-core/src/classic/recast.rs` 的 `Recasting`、`RecastRule`、`RecastMovement`、`Movement`，以及 `classic/expansion.rs` 的批量弃置入手。单张 `can_recast` / `plan_recast` 与注册工厂继续可用，原 377 条单张轨迹是回归线。可信批量入口可接受获授权的一张或多张 h/e 实体，原有严格玩家入口继续按其规则筛选。批量公告可公开所选牌面，同时保持保密的手牌知情记录；一般暗给的移牌后清除已由 `509a9a8` 实现。
- 通过非默认 feature 的正常首页、选将、牌桌多选和 AI 验证；真实名典样例已证明两张普通基本牌通过实际技能提交。名典其他技能效果和不同规则族沿覆盖盘点分别验收。不要把逐张调用或只写协议类型算作批量重铸完成。详细行为证据在 `docs/exec-plans/active/p1-engine-migration.md` 的 R3 小节。

## 本任交付

- **Q2b 巧说。** 在上游 commit `c73270948d72949aa80009536b9fd5400767d091` 实际复跑 `compare`。平、负在 `phaseAfter` 请求时仍见限制，首个实际回调已恢复决斗。获胜时只为下一次合格使用改目标。真实用牌先支付，原本超范围的 D 进入额外候选，B/D 分别受伤，下一次决斗只作用于 B。借刀的原版按位配对、减目标、取消和主动/被动响应另有 105 条观测。核心 `24f5334`、`c34fcb5`、`8a46ebb`；牌桌 `6263020`。
- **X2 忠义。** 真实主动技能、限定状态、实体与标签、模式队伍和可修改伤害进入底座，普通关羽保留武圣。自然跨度两轮的 42 行逐列对齐；另有 47 行取消/移交/双持有者，以及 17 行持有者在失牌回调死亡的实测。此时已公开装备保留在无主特殊区，使用和扩展的后续时机照常完成。核心 `8779367`、`3d9a69e`、`bb5006c`；牌桌 `b06c8fb`。
- **P3 阶段、回合与判定。** 当先/放权、普通跳过与取消、额外回合全局号/自然游标、前插和嵌套队尾均经上游验证。普通阶段场景比较 494 行中的 154 条共同请求和回调，队列场景比较 112 行的共同字段。根、各层目标、标记、每段用量及索引激活均在实际牌桌结算。另有判定区六例 158/158 行，包含乐不思蜀、闪电、无懈、天妒和双实体独立清理。67 行补充说明无懈在选中后转手与已进入失牌步骤再被截走时的差别。核心 `cab5823`、`12916b3`、`042c6df`；牌桌 `e65557e`。
- **Q3 多席拼点。** `f67b98b` 复用两席和多席的密封收牌、批付与按根清理。逐对和共同两种根、可信查询与真实鼓舌/胆烈在 `9a493ca`；客户端 `b215b9d`。五局 323 条中逐行比对 173 条请求和 118 条首回调，其余 32 条展示、内容、探针和后续伤害类时点未计入 291。另验证点数变化、胜者、逐对暂停、对手私选、单席/批量支付取消和真正根取消，独立复核已通过。
- **R3 取证与起点。** `cd9ffe4` 的 190 条及 `5edeadb` 的 62 条都已独立 write/check。普通暗给隐去出资席剩余手牌的旧知情有 5 项定向回归（`509a9a8`）。批量重铸根、整批工厂及正式提交验收尚未实施。

目标仍是固定上游整套对局机制与可观察行为。普通标准模式、单张重铸和两席拼点等既有结果不能代替整体验收。128 个 content 入口之外还需覆盖 player、gameEvent、game、模式和其他调度职责。额外联合选择、批量移动、禁用/扩展区域、角色上限/明暗/休整/复生、关系和修饰、通用历史及确定性回放都要逐族检验，见 `docs/exec-plans/active/noname-coverage.md`。

## 验收与本机环境

最新功能提交 `509a9a8` 的普通发布 WASM SHA-256 为 `41400E8D9638D99ADDEC94F8B9EE47C061C0ADD21EAA747A65FFAA9E12FEDEC2`。257 项常规 Rust release 测试（238 单元、19 集成）、单独 100 局八席（55,283 次决定、全终局守恒）、33 项 TS 测试、`pnpm engine:test`、`pnpm typecheck`、`pnpm build`、标准内容与美术校验全部通过。末轮三次 100 局重测，规则 p95 为 9.5 / 9.8 / 9.7 µs，投影与 JSON p95 为 12.5 / 12.7 / 12.7 µs。生产首页、选将、出牌/响应、动效、终局和重开，以及重铸、属性、两席拼点、巧说、忠义、阶段、多人拼点七个各自编译的非默认 WASM 工厂均经浏览器实跑。新版 Q3 行为以 Rust/WASM 和这些真实浏览器场景验收。

Windows 和 Android 的独立原生验收对应 `042c6df`，当时的默认发布 WASM SHA-256 为 `E04FF567BABC6D2CAC7E015FDF2451B3285EDD185436AF2EDC3F00C41A8223B3`。在两端实际 WebView 内重新读取的字节与该文件一致，均离线从首页选张飞进入八人局并提交过河拆桥。Windows 版本 0→1，Android 版本 0→5，均无 JS 异常。

- Windows 程序 `E:\Temp\my-awesome-kill-042c6df.exe`，SHA-256 `3470DC8E3AE33B4DB2A2ABC9348D2188CBA05A03F1B4424EAC249A5FDF9EFF1A`。
- ARM64 debug APK `E:\Temp\my-awesome-kill-042c6df-arm64-debug.apk`，135,172,351 字节，SHA-256 `A6AF768146548523FE7ABBD9212D2AE3AE5CEA0ECA383F2E0626FF385E2EE2E0`。v2 验签、16 KiB 对齐、原生库及设备 base.apk 均已核对。设备为隔离 API 36 x86_64 AVD 的 ARM64 兼容环境，飞行模式、640×320 横屏。曾有一次将盘符路径解释成 URL 的失败包，最终包已改用相对 frontendDist 并完成设备实测。
- 本任浏览器截图及两个 native verification.json 位于 `C:\Users\evanpatchouli\.codex\visualizations\2026\11\18\01a1efb0-eb04-7982-be52-e6674b8e18e7`。原生证据在 `native-042c6df\windows`、`native-042c6df\android`。

本机已有 Node/pnpm、Rust toolchain 和 wasm-bindgen，路径见 `docs/runbooks/development.md`。pnpm 10.20.0 / wasm-bindgen 0.2.130 在 `%LOCALAPPDATA%\my-awesome-kill\tools`，Rust 在 `%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin`。复用现有 target、SDK 和 Gradle 缓存；本任使用 `CARGO_INCREMENTAL=0`、`CARGO_BUILD_JOBS=2`。C 盘承载 `C:\Work\Work.vhdx`，大型构建/模拟器启动前检查宿主盘，错开高负载。

WSL Ubuntu 的独立检出为 `/tmp/mak-oracle-noname-c7327094`，Node 为 `/home/evanpatchouli/.vite-plus/js_runtime/node/24.16.0/bin`。从 packages/fs 启动 `../../node_modules/.bin/tsx src/entry.ts --debug --dirname=../../apps/core`（8089），从 apps/core 启动 `node node_modules/vite/bin/vite.js dev --strictPort`（8081）。Windows Playwright 位于 `E:\Temp\mak-oracle-tools-1.65.1\node_modules\playwright`，Chrome 为 `C:\Program Files\Google\Chrome\Application\chrome.exe`。脚本环境变量和所有场景见 `tests/reference/README.md`、`docs/runbooks/testing.md`。交接时已收掉本任浏览器服务、上游服务、测试进程和隔离模拟器/专属 ADB。

本地 `master`，最后一项功能提交 `509a9a8`，R3 补充取证 `5edeadb`。交接前用户没有独立未提交改动。源码增量 bundle 位于 `C:\Users\evanpatchouli\.codex\backups\my-awesome-kill\p1-20261119-01a1efb0-source.bundle`，要求已有 `be0c9c6`。先核对代码、测试、Git 状态和本任 chat 归属再继续。

## 交接原因

本任最新有效采样为 2026-11-18T20:44:26.444Z，last_token_usage.total_tokens = 636,054，model_context_window = 828,400，占 76.78%。距 80% 留有 26,666 个 token。 两份上游批量轨迹、兼容基线、独立暗给修复及整轮验收已分别完成。下一项可实际验收的 R3b 用例需要同一根内的有序多实体失牌、按原数组的一次批量获得、取消后的结果、知情与旧单张工厂兼容。本任最近的多席收牌到完整算分和复核已跨越十万余 token 的上下文增长；再启动这项跨层工作可能挤占交接空间。本次在工作区干净、无在途写入的边界交班。
