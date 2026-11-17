# P1 接棒现场 · 经典模式已运行，完整底座继续

## 先做这一件事

核对本会话绑定和 cwd，读 `.agents/current-task.md`，检查 `git status --short --branch`。**第一项开发工作：扩展 `scripts/reference-trace.mjs` 的独立上游驱动，采一条“同一事件有 firstDo、同席同优先级、另席、lastDo，并由技能取消”的真实轨迹。** 不覆盖已有 sha-no-shan golden。先复跑已知 golden 确认上游环境，再注入纯测试技能固定局面和选择；记录实际派发顺序、代价、父子事件、取消后可观察的 after 清理。用它驱动一般化的 Rust 触发注册/过滤/排队/恢复实现，接已有标准技能做回归。继续后续各族，直到用户定义的完整 P1 验收，而非做完首个探针便结束。

- 参考锁定为 `c73270948d72949aa80009536b9fd5400767d091`。原版观测方法、依赖和 fixture 位于 `tests/reference/README.md`。主仓 `.reference/noname` 保持干净。已有可复用 WSL Ubuntu 独立检出 `/tmp/mak-oracle-noname-c7327094` 及 `/tmp/mak-oracle-bin`，交接前 8081 未在运行。可在该目录将前述 bin 加入 PATH 后执行 pnpm dev，实际入口 8081、fs 8089。若临时文件被清掉，按 README 重建。不可把它变成产品运行时依赖。
- 源码入口：`noname/library/element/gameEvent.ts` 的 loop/trigger/cancel 与 `content.ts` arrangeTrigger/createTrigger；实际少量关键材料先检索。Rust 的 `scheduler.rs` 处理栈和 next/after，`classic/skills/triggers.rs::register_hero_triggers` 只覆盖 11 种 HeroMoment 和标准技能。不要仅把配置名补上便宣布全局迁移成功。

## 分支、提交与可用成果

项目 `E:\my-awesome-kill`，`master`。本轮起点 `4a83427`，当时工作区干净，没有应另行保留的用户未提交改动。工作单元已按 AGENTS.md 本地逐次提交。收口前最近的正式文档提交 `82f3c01`，此后只有交接文件提交。没有仍在写入的 SubAgent，无待合并分支或外部推送。

- `b56e3b5`、`ef2b488`：Rust 事件栈、身份、阶段、死亡求救、判胜与收尾。
- `9da9d2f`、`7fec40d`、`17152ac`：25 名标准武将原始数据/资源、104 张牌、物理和转化材料、全部 40 技能与标准装备锦囊。行为在新 Rust 接口，不要求兼容原 JS 扩展。原版实测轨迹提交 `aa677f1`。
- `c228d7f`：复核后修复无懈选择泄露与 AI 自反、关羽连弩成本、可选技被错误记录发动。不要回退这些守恒、隐私和时序回归。
- `39d820b`、`6923bbe`：Rust 编译为 WASM 供浏览器与 Tauri 共用，正常首页进入 25 将自选、2–10 人身份局。官方 simple 牌桌底图、武将/牌面原资源、装备和判定公开区、逐组弃牌与动效已落地。TS 双人引擎只作原型基线及显示元数据来源。
- Windows `apps/game/src-tauri/target/release/my-awesome-kill.exe`，SHA256 `3F8C060196353FB4E11627D7A54047DB3E8900AA6AA06A943E374EB58743039F`。Android `apps/game/src-tauri/gen/android/app/build/outputs/apk/arm64/debug/app-arm64-debug.apk`，SHA256 `2657E4D9E067F34C3800A12FB31C9D9946F94E4F5F8BA13877356A975966BCD7`。制品未入 Git，存在共享本地工作区。
- 原生窗口已关闭 1420 服务后实际选曹操并进入八人牌桌。Android v2 签名、manifest（横屏、游戏分类）、ARM64 .so 和内嵌 WASM 已核对；临时隔离模拟器返回 0xC0000135。没有在用户私人设备安装。不要把 APK 检查当设备实跑。

## 可复现验证

- `pnpm engine:test`：84 Rust 常规测试及编译后的 WASM 测试，含实际捕获 83 hook 的六个共同里程碑比对；`pnpm test` 对旧 TS 基线为 33 项；另行运行的 Rust 100 局 benchmark 全终局、55,247 决策，p95 7.7 µs；投影和 JSON p95 15.8 µs。
- `pnpm typecheck`、`pnpm build`、`node scripts/check-standard-content.mjs`。正式页面、调试与环境见 `docs/runbooks`。生产浏览器 smoke 覆盖 844×390 和桌面、25 将筛选、观星、仁德、多人目标、换装、批量弃牌、动效和终局/退出重开。截图保存在 `C:/Users/evanpatchouli/.codex/visualizations/2026/11/17/01a1eada-f808-7cc2-b386-ef7c9b1658b5/identity`。
- 此机 pnpm shim / cargo shim 有已知装入点问题。调用 rustup toolchain 的 stable MSVC bin 内 cargo，将它与 wasm-bindgen 0.2.130 的 bin 放在进程 PATH。Node 启动官方 pnpm（已有 `%TEMP%/mak-bin/pnpm.cmd` → `%TEMP%/mak-pnpm/package/bin/pnpm.cjs`）。若临时提取内容丢失，可从 `%TEMP%/mak-pnpm.tgz` 与 `%TEMP%/mak-wasm-cli-0.2.130/cli.tgz` 恢复。已安装 WASM 和 ARM64 target。Playwright 1.65.1 在 `%LOCALAPPDATA%/my-awesome-kill/qa-playwright-1.65.1/node_modules/playwright`，传对应 smoke 或 reference 的模块环境变量即可。本机 Windows Gradle NIO 失败，Android 端采用隔离 WSL Gradle（开发手册），问题与项目 Rust 规则不同。

## 仍须完成与接力原因

`docs/exec-plans/active/noname-coverage.md` 记录核对过的 128 个 content 入口及其以外的 player/game/gameEvent 能力，并区分已实现切片与缺口。后续至少包含一般触发注册优先级/重验与取消、多阶段插入/跳过、临时技能和历史、私密拼点、重铸、扩展区/标记、数值护甲及属性连环、角色明暗置/复生/上限变动、动态区域及复杂选择；逐族核对剩余项。给出了可以实跑的铁索/法恩、巧说/纵适、忠义等探针。只将迁移底座需要的测试内容放进夹具，无需新增玩家模式。大范围完整等价仍未通过；不应以标准身份局可玩替代该结论。

本次在完整经典身份模式验收边界提前交接。首任已携带旧 TS 原型、UI 重做、Rust 身份模式和多次复核的上下文。接下来是跨 `gameEvent.ts`、`content.ts`、`player.js`、`game/index.js` 及跨包能力的通用调度迁移；仅主要文件已超过 1.6 MB，另须保留原版和新实现的轨迹及跨内容验证空间。按用户授权在新材料阶段之前交班，未触及 80%。最近可靠采样时间 2026-11-17T21:48:35.0630000Z，已用 453314，容量 828400，占比 54.72%。后继须用自己的 CODEX_THREAD_ID 核对与采样；继续传递 current-task 中的原样项目绑定和接力授权。
