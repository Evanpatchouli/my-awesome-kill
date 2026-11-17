# 应用架构

pnpm workspace 组织 Tauri 2、Vite 8、Vue 3、vue-router、Pinia 应用。Rust 规则核心编译为 WebAssembly，浏览器与原生 WebView 共用同一实现。Tauri 的 Rust 宿主还负责 SQLite；Android Activity 在 manifest 中声明游戏类别及横屏。用户已确认兼容目标是原版机制与可观察行为，允许新内容接口。

## 权威状态与输入

`packages/duel-core` 持有实体牌、座次、身份、阶段、事件栈和等待的选择。`Game::classic` 依种子、选将、人数初始化。调度器处理父子事件、下一步、取消及收尾队列。每个 `Command` 携带座位、版本和动作。校验通过后推进到下一个选择或终局；失败保留状态和已有事件。AI 沿用同一输入管线。

`protocol.rs` 定义输入和各席可见的投影。每次成功提交只返回最近一批事件，客户端可以按版本和事件编号增量记录。`classic.rs` 与 `classic/lifecycle.rs` 处理身份和回合，`classic/equipment*`、`classic/tricks.rs`、`classic/skills*` 对应装备应答、锦囊和武将行为。数据在 `packages/duel-core/data/`，含 25 将、技能说明和全部 104 张实体牌。扩展行为应通过相同调度、选择和牌区操作。通用时机与内容接口仍在[逐项迁移](../exec-plans/active/p1-engine-migration.md)。

`packages/duel-web` 是 wasm-bindgen 边界，只负责编解码、选择观察席、提交指令和单步推进电脑。`scripts/build-engine.mjs` 构建锁定依赖的 Rust 模块并生成 Vite 所用的加载器。原型的 `packages/rules` 保留 TS 对照测试和显示用牌面定义；正式对局从 WASM 启动。

## 玩家界面与持久化

`apps/game/src/ClassicSelection.vue` 使用标准武将数据和本地原图；`ClassicBattle.vue` 展示各席的公开资料，维护局部选择草稿。选择分支由服务端投影给出，客户端按版本提交。Pinia 的 `game.ts` 管理 WASM 实例的生命周期、动作锁、电脑决策节奏和战绩重试。换局或离开会释放实例并取消旧回调。

原始武将、卡牌和当前使用的主题资源在 `apps/game/src/assets/standard`。Tailwind/CSS 管布局和基础交互，GSAP 根据结算事件列表播放牌的移动与体力反馈，@vueuse/motion 负责装饰性入场。开启减动效时显示结果和操作提示。

`persistence.ts` 将原生战绩交给 `apps/game/src-tauri/src/lib.rs`；SQLite 按对局 ID 去重。浏览器以本站 localStorage 保存战绩。中途离开会放弃本局；写入失败的完局记录在重试队列中保留。
