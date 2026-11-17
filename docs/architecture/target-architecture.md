# 应用架构

P1 以 pnpm workspace 组织两个包。规则层是独立 TypeScript 包，可在 Node 中复现整局；应用包使用 Vite 8、Vue 3、vue-router 和 Pinia。两端复用同一份游戏核心和界面。Tauri 2 的 Rust 壳承载桌面与 Android WebView，Android 的 Activity 在 manifest 中强制横屏。

## 规则与对手

- `packages/rules/src/cards.ts` 保存 104 张牌的牌面、装备槽、射程、说明。实例编号与牌名分离。武将的体力、性别和技能文案位于 `heroes.ts`。
- `engine.ts` 管理牌区、带种子的洗牌与抽取、回合、弃牌、濒死与胜负。每次等待输入时产生合法选项，响应后继续尚未结束的结算。`combat.ts` 处理杀、闪、武器和防具，`tricks.ts` 处理普通锦囊、无懈链及延时判定。两人局目标条件见[对局契约](../specs/classic-duel.md)。
- `createMatch({seed,hero})` 产生隔离的对局。UI 读取 `view()` 的公开投影，不接触电脑手牌或待执行回调；`act(revision,key)` 在当前玩家有决策权且版本正确时执行一个选项。电脑的 `bot()` 只处理一个选择，分值来自相同的规则选项。规则没有定时器、DOM、文件系统、Vue 或参考项目依赖。
- 改牌先改数据，然后在对应结算模块更新使用条件、支付和效果，并在 `packages/rules/tests` 添加能构造该局面的测试。新增武将先登记元数据，再在对应时机接入能力。P1 支持内置规则的源代码扩展。

## 界面与本地数据

`apps/game/src` 承载路由、页面与对局 store。通过独立 hash 路由进入首页、画廊、选将、对局和个人/设置页面。局面在核心同步提交，Vue 更新投影，GSAP 处理与结算关联的视觉反馈，Tailwind 用于布局和基础样式，@vueuse/motion 处理装饰性入场。离开页面或开始新局会失效旧局定时任务。

`persistence.ts` 将原生端的战绩读写发给 `apps/game/src-tauri/src/lib.rs`。SQLite 以对局 ID 区分战绩，重复保存同一局保留首条。浏览器开发入口将战绩存放在当前站点 localStorage。战局本身驻留内存，进行中退出会放弃本局。
