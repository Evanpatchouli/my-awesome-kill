# 验证对局与应用

先按[开发手册](development.md)准备 pnpm、Rust、WASM target、wasm-bindgen 及浏览器依赖。

| 检查 | 从仓库根目录执行 |
|---|---|
| Rust 单元、集成与 WASM 协议 | `pnpm engine:test` |
| 旧版 TS 规则基线 | `pnpm test` |
| 两个前端包类型与生产构建 | `pnpm typecheck`、`pnpm build` |
| 与已检出上游核对全部标准数据和原图 | `node scripts/check-standard-content.mjs` |
| SQLite 首次写入及重复对局 ID 不覆盖 | `cargo test --locked --manifest-path apps/game/src-tauri/Cargo.toml --lib` |
| Rust 格式与静态检查 | `cargo fmt --check --manifest-path packages/duel-core/Cargo.toml`、`cargo clippy --locked --all-targets --manifest-path packages/duel-core/Cargo.toml -- -D warnings` |

### 真正的原版对照

[参考轨迹与复现方法](../../tests/reference/README.md)说明如何用独立的 noname 检出运行真实 identity 模式，并将结果与 `tests/reference/sha-no-shan.json` 对照。编译后的 Rust/WASM 使用相同初始体力和实体杀/无闪局面，自动比对能映射的六个时机及最终状态。开发、运行与打包随心杀均不依赖参考仓库。

### 真实玩家入口

1. 首次运行 `pnpm --filter @mak/game exec playwright install chromium`。
2. 启动 `pnpm dev --host 127.0.0.1`，然后在另一终端运行 `pnpm --filter @mak/game smoke`。默认连接 1420。另可运行 Vite preview 并通过 `MAK_SMOKE_URL` 指定生产构建服务地址。
3. `MAK_SMOKE_SCREENSHOTS` 可指定仓库外截图目录。调试时可用 `MAK_QA_PLAYWRIGHT` 与 `MAK_QA_BROWSER` 指定已安装的模块与浏览器。
4. 检查八人座位及身份可见性，选择非原型两将、手牌与合法目标，测试响应、多选与撤选、装备替换、技能使用、动画锁、退出并重开。在 844×390 横屏确认手牌、响应按钮和当前行动者可见。
5. 在 640×272 的可用视口检查短横屏。用种子 20 从画廊选张飞和八人局，点过河拆桥、目标、确认，并在之后的闪响应中点放弃。牌桌根节点应无需上下滚动，最左席可点击，手牌和响应按钮的中心均在屏内。此尺寸对应 640×320、160 dpi Android 模拟器上下各 24 px 系统栏之间的可用区域。

### 性能与覆盖范围

运行 `cargo test --release --locked --manifest-path packages/duel-core/Cargo.toml --lib benchmark_seeded_eight_player_games -- --ignored --nocapture`。游戏使用真实 Rust 规则，按固定种子跑完一局后验牌区守恒；进程启动、浏览器动画和 I/O 不算作结算耗时。[Rust 核心说明](../../packages/duel-core/README.md)记录测量方法和详细结果。

### 重铸的真实 WebAssembly 交互

在项目根目录构建带测试内容的独立绑定，随后用普通 Vite 开发入口运行烟测。示例中的第二套绑定放在临时目录，正常发行版保持默认 Cargo features。

```powershell
$env:MAK_RECAST_BINDINGS = Join-Path $env:TEMP 'mak-recast-bindings'
cargo build --release --locked --target wasm32-unknown-unknown --target-dir packages/duel-web/target --manifest-path packages/duel-web/Cargo.toml --features qa-recast
wasm-bindgen --target web --out-name duel --out-dir $env:MAK_RECAST_BINDINGS packages/duel-web/target/wasm32-unknown-unknown/release/duel_web.wasm
# 另一个终端运行 pnpm dev
node apps/game/smoke-recast.mjs
```

该脚本从普通首页选将，验证标准局没有重铸操作，再从实际 Rust 测试内容检查只可重铸的闪和亦可指定目标的杀。种子 3 的两次重铸使版本 0→1→2、牌堆 94→93→92，手牌保持六张。它还验证暗摸与明获对另一席的投影、动作锁以及旧版本拒绝。

### 铁索、属性牌面与护甲

按上述流程把 feature 改为 `qa-elemental`，将新绑定目录设为 `MAK_ELEMENTAL_BINDINGS`，运行 `node apps/game/smoke-elemental.mjs`。种子 317 的八席局使用真实内容技能创建 ID 104 以上的新牌。测试从后排和前排各选一个目标，确认两席横置，再分别使用火杀与雷杀，核对护甲吸收、传播、解链以及公开记录。其他席在出牌前看不到新牌的花色、点数或伤害性质。普通构建的 104 张牌、25 名武将及其图像继续由 `node scripts/check-standard-content.mjs` 验证。

2026-11-18 的经典模式检查结果：

- 初版经典身份局的 84 项 Rust 测试和旧 TS 基线 33 项通过。加入通用派发后，从暂存的发布配置源码复跑 98 项 Rust 测试，100 场八人局全部终局并校验牌区守恒。Windows / Intel i7-13620H 上这一轮共 55,247 次决定，决策与执行 p95 为 7.5 µs，单席投影加 JSON 序列化 p95 为 11.7 µs；耗时随本机负载变化。
- 生产构建经 Playwright 1.65.1 / Chromium 完成八人选将、隐藏身份及等待时退出，双人完整局、终局重开和战绩；另有观星私看、仁德选牌和选目标。观察到摸牌、装备、出牌、响应、判定、伤害与回复视觉事件。最新横屏实测手牌下缘 386px，视口高度 390px。
- 独立复核的关羽种子 57（以连弩支付额外杀）、周瑜种子 0（放弃英姿）、刘备种子 1/21（无懈 AI 和响应公开信息）已有针对性回归。WASM 探查另外跑过轮换 25 将、2–10 席的 300 个种子，均结束且守恒。
- 稳定可重现的平局有 Rust 测试，未从普通 UI 种子观察到平局画面。Android 设备运行情况见开发手册。这些用例证明对应的经典玩法行为。全底座的剩余机制依[覆盖清单](../exec-plans/active/noname-coverage.md)继续验证。
