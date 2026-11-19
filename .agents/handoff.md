# 随心杀 P1 直接接棒

## 接棒后先做

用本任完整 `CODEX_THREAD_ID` 在 Codex 桌面端匹配自己的 chat，核实 hostId、projectId 和 cwd，并用 `list_projects` 对照登记项目，再检查 shell 实际目录。固定绑定为 local / `56a3f74c-1f60-47c5-8e25-30ee6f6375cb` / `E:\my-awesome-kill`。核实后读 `.agents/current-task.md`、适用的用户指令，检查 `git status --short --branch`。用户最新的 AGENTS 指令入口为 `C:\Users\evanpatchouli\.agents\AGENTS.md`。接力协议与逐字传递的项目绑定规则在当前任务文件末尾。

本任 `01a1f06a-3e4d-75e1-8413-ce32e97cc306` 从 `9351a7d` 接手本地 `master`。交接前所有产品、测试和说明写入均已本地提交，工作区没有用户独立修改。本任创建的应用、测试页面服务、固定上游服务、专属 ADB、模拟器和 Gradle daemon 均已关闭。项目事实以代码、固定样本和可重复运行结果为准。

**下一项是同步复活与事件版复活。** 首先在固定上游 `c73270948d72949aa80009536b9fd5400767d091` 的独立检出重跑：

```powershell
node scripts/reference-trace.mjs --scenario revival --check tests/reference/revival.json
```

1. 用普通八席注册技能和 `Game::submit(Action::ActivateSkill)` 建立第一个 Rust 失败用例。让非主公 B 带手牌、装备、已显示的七个鼓舌标记，横置并翻面，由另一席的真实操作令其死亡。完整死亡后应能分别读取存储计数 7 和空的公开标记。现有 `clear_marks_on_death` 删除计数，`compare_content/tests.rs` 中也有一个依据旧实现写成 0 的断言。按实测修正存储及投影，并保留临时技能明确清理的语义。
2. 在同一测试中加入同步复活。复活后 B 实际为 2/4，已公开身份和武将延续，弃掉的实体仍在弃牌堆。B 追加到存活名单末尾，按座位索引的存活环则再次接通原邻座。重复调用传入 9、0、-2，在四点上限下分别得到 4、1、-2 体力，最后仍存活。原上限为负时先把上限恢复到至少 1。同步调用在当前回调立即生效，事件入口另走 Before/Begin/End/After 和可取消流程。
3. 检验 `reviveBefore` 取消后仍死亡，`reviveEvent(0)` 正常恢复到 1/1。另有真正的 `die` 主时机内同步复活样例。该反贼保留手牌、装备、七个标记及临时技能，身份已明，来源的击杀数增加 1，且在 `dieEnd` 前摸三张。现有死亡清理仍按“已进入死亡”的旧边界执行，需要在真实后继步骤重新读取生死状态。注意正常死亡奖惩、终局及各实体守恒。
4. 源证据为 `d357024` 及 `17c64d6`。后者保留原 78 条，另加真实鼓舌和死亡主回调复活的独立结果，连续三次上游复跑匹配。实现后按行为复核并用独立非默认内容在正常牌桌验证。跨轮休整另起单元，核对 `rest`、`restEnd`、出局状态、自然回合倒计时及额外回合。详见 `docs/exec-plans/active/p1-engine-migration.md` 和 `docs/exec-plans/active/noname-coverage.md`。

## 本任已交付

- **批量重铸与明典。** `af59798` 共用重铸根并支持整批工厂、真实 h/e 实体、一个失牌和一个摸牌/获得、可取消包装、已知手牌和局部/整批截获。七个非明典场景逐行核对 211 条共同请求、执行回调与工厂观测。`5ec0446` 补充 23 条给牌中转死亡观测。`d654a09` 验证给牌者与旁观者的独立知情。`e82c45b` 在明典的出牌和受伤入口使用真实多选。种子 317 在实际 UI 选普通杀 28、11，暗摸 92、33，随后 1 号 AI 亦发动。原有单张重铸 377 条已复跑。
- **有序分区与私有牌堆。** `d519f4c` 固定 82 条上游请求。`b713bf1`、`145824b` 实现逐步校验、AI、取消、私有处理区、牌堆还原与实际清理。`4cc42a2` 在牌桌支持位置插入、行内重排、撤销与短横屏。`0f082cc` 实现恂恂，核对该局 25 条中的 21 条共同请求及五张实体的位置，在真人操作和 AI 路径验收。Rust 对严重边界另验局终不再开选择、无法求解的强制 AI 选择、子选择与清理均取消时释放原实体和所有权。
- **确定性命令回放。** `89b50b5` 的 `RecordedGame` / `RecordedDuel` 从指定初始局面或经典种子录制接受的命令并恢复任意前缀。自定义内容按同一版本标识重新安装独立工厂，索引触发器按注册顺序采样。完整八席 AI 对局、发布 WASM 的 10 条人机指令和两个独立浏览器上下文的六个检查点均通过。完整命令属于离线可信宿主数据。全局通用事件历史、录像呈现等仍列在覆盖表。
- **体力上限与死亡。** `587b5b5` 固定 20 局 226 条。`9c56549` 的真实八席对照全部 220 条请求与回调，另外 6 条上游技能内容首尾单列。包含正负数、零、直接夹断、两处取消、带来源或无来源死亡、威重和红色实体杀触发的恃勇分支。实际死亡可取消，取消后隐藏身份受保护；非主公救援者或濒死者在桃的嵌套回调中阵亡都有回归。`f160124` 的真实牌桌验证 4/4 → 4/5 → 4/4 → 3/3，威重分别暗摸 2、1、1 张，较高上限用数字表示。酒杀触发条件仍待酒的规则族验收。

既有能力包含普通经典身份局、25 将/40 技/104 张、动态触发与费用、可取消伤害、属性与连环、巧说、忠义、当先/放权、判定与多席拼点。目标仍是参考版本整套对局机制及可观察行为。继续逐族核对区域权限、剩余联合选择、角色生命周期和修饰、完整历史与模式接口。普通模式已可玩只是其中一项验收。

## 发行、原生与本机环境

- 冻结功能提交 `f160124` 通过 278 项常规 Rust release 测试（259 单元、19 集成）、100 局八席终局与守恒、33 项 TS、WASM conformance、typecheck、生产构建、标准数据与 55 张原图校验。100 局 55,216 次决定。三次测量的规则 p95 为 10.2 / 10.0 / 10.1 µs，投影与 JSON p95 为 13.0 / 12.9 / 13.0 µs。普通生产入口和十个单独编译的规则 feature 均通过浏览器实跑。忠义和明典的独立动态导入初始化已在 `74b384a` 修正并重跑。
- 默认发行 WASM SHA-256 为 `23C64D2FC44DF8109E1C9A16E9C632089AD1470E5125FD5735B7076A0DE1B9AD`，1,340,410 字节。冻结资源为 `E:\Temp\mak-p1-dist-f160124-01a1f06a`。Windows release 程序 `E:\Temp\my-awesome-kill-f160124.exe`，SHA-256 `62FF61FDD9333A013E04BFEAA07552682AF4ADE14CA5AEE25F1CB7AB90E1A39B`。
- 最终 ARM64 debug APK 为 `E:\Temp\my-awesome-kill-f160124-arm64-debug.apk`，大小 144,642,295 字节，SHA-256 `7D21BC45FFD8FABFABB3BCC3CFBBA7933D5BC441B2EA6111B9AF26D8B0D9CE93`。v2、16 KiB 对齐、设备 base.apk 和原生库均核对。复用的 WSL 工程起初留有旧 Activity，第一候选的系统栏占位异常。最终包使用与目标版本核对过的原生源码，API 36 x86_64 隔离模拟器经 ARM64 兼容运行，横屏设备 640×320，WebView 可用区 640×272。
- 两端均已离线从运行中的 WebView 读回相同冻结 WASM。选张飞进八席并提交过河拆桥，版本均 0→1。返回后选诸葛亮进八席，观星将 28 放底，86、85 自然进入手牌。Windows 1280×720、Android 640×272，均无 JS 异常。证据在 `C:\Users\evanpatchouli\.codex\visualizations\2026\11\18\01a1f06a-3e4d-75e1-8413-ce32e97cc306\f160124`，包含 `native\windows`、`native\android`、`production`、`features` 及测试与测量日志。
- 复用现有 Rust target、pnpm 10.20.0、wasm-bindgen 0.2.130 和 SDK。Windows 工具目录见 `docs/runbooks/development.md`。本任使用 `CARGO_INCREMENTAL=0`、`CARGO_BUILD_JOBS=2`。E 盘由 `C:\Work\Work.vhdx` 承载，大型构建与模拟器需检查 C/E 空间并错峰。WSL Gradle 临时工程 `/tmp/mak-apk-fast/project`，其受控 Android 文件应先与待测提交核对。两端可复用的 CDP 脚本为 `E:\Temp\mak-p1-native-01a1f06a\native-verify.cjs`。
- 上游独立检出在 WSL Ubuntu 的 `/tmp/mak-oracle-noname-c7327094`，Node 目录为 `/home/evanpatchouli/.vite-plus/js_runtime/node/24.16.0/bin`。将此目录置于 WSL PATH，在 `packages/fs` 运行 `../../node_modules/.bin/tsx src/entry.ts --debug --dirname=../../apps/core`（8089），在 `apps/core` 运行 `node node_modules/vite/bin/vite.js dev --strictPort`（8081）。Windows 设置 `REFERENCE_PLAYWRIGHT_PATH=E:\Temp\mak-oracle-tools-1.65.1\node_modules\playwright`、`REFERENCE_CHROMIUM_EXECUTABLE_PATH=C:\Program Files\Google\Chrome\Application\chrome.exe`。命令与样本说明见 `tests/reference/README.md`。

## 交接原因

本任最新有效采样为 2026-11-19T03:04:02.695Z，last_token_usage.total_tokens = 637,209，model_context_window = 828,400，占 76.92%。距 80% 余 25,511 个 token。 从体力上限工作仍在进行时的 513,956 到本次收口，实际增长了 123,253 个 token。下一项要同时处理同步与事件复活、公开身份、标记存储、两种名单顺序、死亡中途回调和奖惩，并完成输入和牌桌复验。再开这一单元可能挤占交接空间。本任在完整发行和原生验收后交班。
