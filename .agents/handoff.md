# 随心杀 P1 直接接棒 · 原版牌桌与真实操作

先用自己的完整 CODEX_THREAD_ID 在桌面 list_threads 精确核对 host/project/cwd，再用 list_projects、shell 和 git 核实 local / 56a3f74c-1f60-47c5-8e25-30ee6f6375cb / E:\my-awesome-kill。读项目 AGENTS.md、C:\Users\evanpatchouli\.agents\AGENTS.md、.agents/current-task.md。不要翻旧聊天。绑定不符停止写入和传棒。只保留 Git/status 证实的事实；若有新用户改动不动它们。

## 最优先：做出与原版相符的实战界面

用户明确对玩法 bug 和未能原样复刻 UI 不满。用固定 noname c73270948d72949aa80009536b9fd5400767d091 的普通八席身份局、默认 long2/simple/玻璃体力/默认背景作为第一份基准，避免自造视觉设计。继续保留路线图首页、画廊、25 将自由选择、40 技、104 实体、2–10 席。没有整桌验收前不要用规则测试数推导玩家完成率，之前的 90%–95% 估计已撤回。

桌面和两种横屏的修前/修后图、原版实机图、DOM 测量、浏览器复跑说明及独立上游脚本：
`C:\Users\evanpatchouli\.codex\visualizations\2026\11\19\01a1f25a-a650-74b2-bb36-8a224e29ddf8\table-parity`。先看 noname-1280.png、table-parity-1280x720.png 和两个 dom JSON。两端同是关羽主公首轮出牌，其他将/随机牌序不同。原版选将有无关缩略图 404，场上八张立绘均已实际渲染。

`5e1dda8` 已使 1280×720 七个对手席及立绘矩形与原版最大偏差 0.1 px，座1在右/7在左，三档 1280×720、844×390、640×272 真点实体杀→选另一目标→同牌撤选→重选→点击座位1下缘确认，实体离手，revision 0→1，窗口与牌桌不滚动。新增 apps/game/smoke-table-parity.mjs，只证明上述路径。下个单元直接实现自己的武将/手牌/控件和装备槽，优先可见结果，不要继续用大量孤立规则探针推迟修 UI。原版 1280×720 自己座位 x38.4,y512 宽120高200，首张手牌 [172.4,582,104,120]，结束回合 [178,547,86,29]，武圣 [617,547,46,29]。八卦阵在双端均真装备过，原版第二槽 [41.4,620.4,96,18.4]，当前仍是一行文字。对手牌面的名字、玻璃体力、角标、边框、顶部按钮、公开/私有提示与动效也还明显不同。默认入口必须实点；将用户可见缺陷的原版/现版/修后证据分别留下。保留非默认 feature 桌、私有信息边界、小屏可操作，不宣称单一探针等于全底座。

## 规则与磁盘收口

恢复时的 33 个改动文件与备份逐字节一致。普通/扩容装备栏与稳定登记 `b731f40`、`6cef811`、`9302411`。合马、多武器射程、青釭技能钩子、目标级破防、可信杀根取消、排除、延后队列快照和同步撤销的 FIFO 清理已收口 `1c56648`，合马玩家/AI 桌 `59bd2e2`。原版八场取消/清理样本 `6f2d1f8`。Rust 受限 gain_expansion 在原实体已弃时会拒绝获得；上游广义 gain 可从弃牌堆取得。这是后续公共获得接口缺口。多武器青龙/斧同场选序已有上游实测但 Rust 未完整接入，借刀多武器、其他锦囊目标时机、多材料虚拟牌、联合选择、通用历史/模式接口、弥笃/活墨与酒杀等继续按 docs/exec-plans/active/noname-coverage.md 和 p1-engine-migration.md 跟进。原生发行验收的上一冻结点为 `8dad11d`，其制品与路径仍保存在 a30178e:.agents/handoff.md。

本机 E: 是 C:\Work\Work.vhdx，已经历磁盘满断挂、重建迁移。用户要保有实测 120 GiB 的 C 盘空间。不得继续在 E 上堆叠 Cargo 临时 target、完整快照、模拟器或多个临时构建树。先检查 C/E 余量、VHDX 实占与外部 cache 大小。PowerShell 7 使用 scripts/test-core-budget.ps1（-TestFilter 和 -IncludeIgnored）；C:\Users\evanpatchouli\AppData\Local\my-awesome-kill\p1-cargo-target 作为唯一本任构建缓存，CARGO_TARGET_DIR/CARGO_BUILD_BUILD_DIR 均指向它，CARGO_INCREMENTAL=0、CARGO_BUILD_JOBS=2、离线锁定 release。2 GiB cache、120 GiB C 剩余、一次增长3 GiB 或 VHDX 增长256 MiB 采样保护用于该脚本；其他命令另监控，避免多个构建同时跑。两个 JS 入口已支持外部 target。浏览器通过已存在 Playwright/Chrome 与 Vite 直启，做纯前端工作不要反复 cargo build；WSL 复用 /tmp/mak-oracle-noname-c7327094，勿重装依赖。浏览器证据每单元仅少量仓库外截图，使用完关闭自启进程。最后缓存约238.4 MiB，VHDX 前后同为134458900480字节。

已验 Rust release 390 项库+20项集成；另跑100局八席全终局守恒，54,302次决定。33 项 TS、类型检查、生产构建、WASM conformance/10命令重放、非默认合马和正常 Vue 三视口交互均通过。当前默认 WASM SHA-256 1BE054DFC402541877C25D573E3560A3E4DF63E6DCF0377DADD2A7D5F325D72C。未重跑这一版 EXE/APK。

本任最新阶段样本为 2026-11-19T13:54:14.159Z，last_token_usage.total_tokens=687859，model_context_window=828400（83.03%）。本任收口后只创建一个直接后继，被受理后立即结束，不监测。后继按自己的最新有效样本计算；同样授权继续开发、分级委派、逐单元验收、本地提交，至上下文达到80%或下一单元可能占用交接空间时按原协议续接。P1整体验收完成则结束接力，用户才能解决的阻塞直接报告。项目绑定的强制逐字传递文本见 current-task.md 末节。
