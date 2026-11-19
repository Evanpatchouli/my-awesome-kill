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

普通生产入口的观星烟测从选将开始，实际打开私有分区面板，移动实体、撤销，再确认新的顶底顺序。脚本核对随后摸到的实体，在 640×272 横屏检查两行牌面和确认按钮的位置，截图名为 `private-choice-short-landscape.png`。

### 性能与覆盖范围

运行 `cargo test --release --locked --manifest-path packages/duel-core/Cargo.toml --lib benchmark_seeded_eight_player_games -- --ignored --nocapture`。游戏使用真实 Rust 规则，按固定种子跑完一局后验牌区守恒；进程启动、浏览器动画和 I/O 不算作结算耗时。[Rust 核心说明](../../packages/duel-core/README.md)记录测量方法和详细结果。

在加入休整后的 `fb070b7` 上，100 局共 54,675 次决定。以同一逻辑 P 核预热后交替运行基线与直接遍历座次环的候选版本，十对测量中规则 p95 的中位数从 10.7 µs 降至 9.8 µs。逐步命令和 0 号席视图轨迹共 269,241,747 字节，两版 SHA-256 同为 `6854C77B6146B2F853AFFCA971A096A3B9DB4B2A4DE19F9A76CA92DF0FF111F7`。与更早版本的 55,216 次决定之差来自反贼摸牌放在旧手牌前面；固定原版的 `rewardOrder` 和 Rust 实体回归均核对了实际顺序。[奖励顺序](../../tests/reference/revival.json)

### 命令回放

`pnpm engine:test` 包含指定牌桌的私有响应与完整种子对局的 Rust 回放，以及发布 WASM 的逐命令前缀恢复。运行 Vite 开发入口后执行 `node apps/game/smoke-replay.mjs`。脚本从正常选将开八人局，用发布模块的录制工厂接受玩家和 AI 决定，然后在全新的浏览器上下文比较全部八席与旁观者的视图。种子 317 的样例包含初始局面、私有弃牌及三次电脑决策，共六个检查点。过期命令被拒绝后日志和各视图保持原值。

### 恂恂的私有牌堆分组

使用 `qa-xunxun` 单独构建 WASM，把绑定目录设为 `MAK_XUNXUN_BINDINGS` 后执行 `node apps/game/smoke-xunxun.mjs`。开局的摸牌阶段可选择发动恂恂。将四张实体中的两张按指定顺序移到底部，确认后由该席自然摸取顶行。人类牌桌测试底行零张、一张、两张时的确认权限和行首插入，并在其他七席检查牌面隐私。

### 体力上限与威重

使用 `qa-vitality` 构建独立绑定，设置 `MAK_VITALITY_BINDINGS`，执行 `node apps/game/smoke-vitality.mjs`。八席样例在实际牌桌增加和减少 1 号席的体力上限，核对 4/4 → 4/5 → 4/4 → 3/3。每次变化只触发一次威重，分别暗摸 2、1、1 张。继续提升至 3/9 时，横屏座位以数字展示体力。取消死亡、死亡后的变动与求桃次序由 Rust 测试和[固定上游轨迹](../../tests/reference/max-hp.json)核对。

### 复活与角色身份

复活牌桌使用独立的 `qa-revival` 绑定和 `MAK_REVIVAL_BINDINGS`。在 Vite 运行期间执行 `node apps/game/smoke-revival.mjs`。种子 317 的八席局由人类指定 1 号席阵亡、取消复活后再允许复活，并再次阵亡后直接复活至 2/4。检查反贼身份持续公开、原四张手牌留在弃牌堆，随后按物理座次由 1 号席进入下一自然回合。

休整牌桌使用 `qa-rest`、`MAK_REST_BINDINGS` 和 `node apps/game/smoke-rest.mjs`。在 640×272 横屏上选择、反选再确认目标，比较休整前后同一张公开赠牌在手牌与已知手牌中的位置。另开一桌，正常结束出牌并按要求弃牌，让 1 号电脑角色完成回合，自然发动殁亡。返回当回合的三张新手牌分别来自返场技能和自然摸牌。[休整参考数据](../../tests/reference/rest.json)

### 判定区废除与恢复

判定区烟测使用 `qa-judge-area` 和 `MAK_JUDGE_AREA_BINDINGS`，运行 `node apps/game/smoke-judge-area.mjs`。种子 317 可用大乔的国色将方片杀 28 作为乐不思蜀放入 1 号席。废除该席判定区并取消弃置，牌桌会同时显示“已废除”和原判定牌。再次废除并允许弃置后 28 只进入弃牌堆一次，恢复区域后可用方片闪 40 再次国色。[普通与取消](../../tests/reference/judge-area.json) · [落地时机](../../tests/reference/judge-interruptions.json)

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

### 明典的批量重铸

按重铸流程使用 `qa-mingdian` 构建独立绑定，将目录设为 `MAK_MINGDIAN_BINDINGS`，运行 `node apps/game/smoke-mingdian.mjs`。种子 317 从首页选关羽进入八席，点击明典依次选中实体杀 28、11。一次确认生成一个重铸根、一个批量失牌、一次摸两张及一次暗获。日志按选择顺序合并显示两张材料。脚本逐席核对两张替代牌的隐私，在 844×390 验证两张所选牌与确认按钮的位置，处理人类响应后观察 1 号 AI 席实际发动明典。普通局从同一首页走默认绑定。

### 铁索、属性牌面与护甲

按上述流程把 feature 改为 `qa-elemental`，将新绑定目录设为 `MAK_ELEMENTAL_BINDINGS`，运行 `node apps/game/smoke-elemental.mjs`。种子 317 的八席局使用真实内容技能创建 ID 104 以上的新牌。测试从后排和前排各选一个目标，确认两席横置，再分别使用火杀与雷杀，核对护甲吸收、传播、解链以及公开记录。其他席在出牌前看不到新牌的花色、点数或伤害性质。普通构建的 104 张牌、25 名武将及其图像继续由 `node scripts/check-standard-content.mjs` 验证。

### 拼点的私选与共同翻牌

用 `qa-compare` 构建独立的 web 绑定，将目录设为 `MAK_COMPARE_BINDINGS`，执行 `node apps/game/smoke-compare.mjs`。脚本从正常首页进入八人桌，比较八席和旁观投影，并验证三组实体牌。种子 9282 为红桃 7 经改点后以 13 胜 9，47319 为 7 比 7 平局，233835 为 4 比 10 负。最后一组在共同亮牌之后、判胜之前留出一次真实的规则选择，检查两张原牌同时可见，以及点数和胜者随后才出现。关闭展示后可从对局记录再查看。

2026-11-18 的经典模式检查结果：

- 初版经典身份局的 84 项 Rust 测试和旧 TS 基线 33 项通过。加入通用派发后，从暂存的发布配置源码复跑 98 项 Rust 测试，100 场八人局全部终局并校验牌区守恒。Windows / Intel i7-13620H 上这一轮共 55,247 次决定，决策与执行 p95 为 7.5 µs，单席投影加 JSON 序列化 p95 为 11.7 µs；耗时随本机负载变化。
- 生产构建经 Playwright 1.65.1 / Chromium 完成八人选将、隐藏身份及等待时退出，双人完整局、终局重开和战绩；另有观星私看、仁德选牌和选目标。观察到摸牌、装备、出牌、响应、判定、伤害与回复视觉事件。最新横屏实测手牌下缘 386px，视口高度 390px。
- 独立复核的关羽种子 57（以连弩支付额外杀）、周瑜种子 0（放弃英姿）、刘备种子 1/21（无懈 AI 和响应公开信息）已有针对性回归。WASM 探查另外跑过轮换 25 将、2–10 席的 300 个种子，均结束且守恒。
- 稳定可重现的平局有 Rust 测试，未从普通 UI 种子观察到平局画面。Android 设备运行情况见开发手册。这些用例证明对应的经典玩法行为。全底座的剩余机制依[覆盖清单](../exec-plans/active/noname-coverage.md)继续验证。

2026-11-19 对 `bbe1f51` 的收口复验通过 147 项 Rust 常规测试，另跑 100 局八席，全部终局并通过实体守恒。共 55,247 次决策，规则 p95 为 7.9 µs，单席投影与序列化 p95 为 13.0 µs。`pnpm engine:test`、`pnpm test`、`pnpm typecheck`、`pnpm build` 通过。普通生产入口 smoke 和三条独立 WASM 内容烟测顺序复跑均通过。默认发行绑定未导出 QA 工厂。

### 巧说的真实目标选择

按重铸流程使用 `qa-qiaoshui` 构建独立绑定，将目录设为 `MAK_QIAOSHUI_BINDINGS`，运行 `node apps/game/smoke-qiaoshui.mjs`。种子 317 从普通首页进入八席，真实巧说选择对手并以 13 比 1 获胜；实体杀先按普通范围选择 1 号席，再由私有选择增加范围外的 3 号席，两席各受一点伤害。随后实体决斗只作用于 1 号席。普通发行绑定不包含该测试工厂。

### 忠义的主动技能与扩展牌

按重铸流程使用 `qa-zhongyi` 构建独立绑定，将目录设为 `MAK_ZHONGYI_BINDINGS`，运行 `node apps/game/smoke-zhongyi.mjs`。普通关羽仍使用武圣，发行绑定不包含测试工厂。八席场景以实际主动技能选择实体手牌或已装备的黑色八卦阵，检查公开扩展牌、标签和限定已用标记；友方实体杀造成 2 点伤害。自然推进到下一轮后材料进入弃牌堆，限定标记仍保留。脚本也检查横屏展示和各席投影。

### 当先与放权的阶段、额外回合

按重铸流程使用 `qa-phase-skills` 构建独立绑定，将目录设为 `MAK_PHASE_BINDINGS`，运行 `node apps/game/smoke-phase-skills.mjs`。种子 317 从普通首页进入八席，真实当先的额外出牌阶段与正常出牌阶段各使用一张实体杀。放权用例分别拒绝和接受：接受后跳过出牌，经正常弃牌阶段，在结束阶段另付一张手牌并私选目标；目标获得额外回合，标记显示来源，结束后回到自然席次。测试检查阶段/回合来源、七个对手的私有投影、横屏和普通绑定隔离。

### 多人拼点的私选和公开揭示

按重铸流程使用 `qa-multi-compare` 构建独立绑定，将目录设为 `MAK_MULTI_BINDINGS`，运行 `node apps/game/smoke-multi-compare.mjs`。脚本从首页选将进入八席，使用真实鼓舌与胆烈选择 A、B、D、E 的实体牌，并在真实 fixing 选择暂停检查公开结果。鼓舌逐对显示，点数依次为 10:8、10:10、10:12；胆烈共同亮牌，点数为 11 对 9、10、8，A 获胜。测试逐个检查私选席和旁观者投影、未揭示牌不可见、横屏牌面与普通绑定隔离。

### 本轮收口复验

2026-11-19，对 `509a9a8` 的代码运行了 257 项常规 release Rust 测试（238 项单元、19 项集成）和单独的 100 局八席实体守恒测量。100 局全部终局，55,283 次决定。相同固定种子的三次独立测量中，规则执行 p95 为 9.5、9.8、9.7 µs，单席投影加 JSON p95 为 12.5、12.7、12.7 µs。`pnpm engine:test`、33 项 TS 测试、typecheck、生产构建、55 张原始图片及标准内容检查通过。最终普通发布 WASM SHA-256 为 `41400E8D9638D99ADDEC94F8B9EE47C061C0ADD21EAA747A65FFAA9E12FEDEC2`，生成绑定与 dist 内的文件哈希一致，默认模块未导出 QA 工厂。

普通生产首页到终局与重开已实际运行。另对重铸、属性与连环、两席拼点、巧说、忠义、阶段技能和多人拼点七种非默认 feature 分别生成绑定，逐个在真实 Vue 牌桌跑通。多人拼点对额外的人类选择、未翻牌隐私与横屏亮牌均有断言。Windows/ARM64 原生实测使用独立冻结的 `042c6df` 版本，见[开发手册](development.md)。

### 批量重铸、有序分区与体力上限的发行验收

2026-11-19，`f160124` 通过 259 项 Rust release 单元测试和 19 项集成测试。单独运行的 100 个固定种子八席局全部终局并通过实体守恒检查，共 55,216 次决定。三次测量的规则 p95 为 10.2、10.0、10.1 µs，单席投影与 JSON 序列化 p95 为 13.0、12.9、13.0 µs。

默认 WASM 为 1,340,410 字节，生成目录、生产构建和冻结分发目录的 SHA-256 均为 `23C64D2FC44DF8109E1C9A16E9C632089AD1470E5125FD5735B7076A0DE1B9AD`。`pnpm engine:test`、33 项 TS 测试、typecheck、生产构建及标准内容和原图核对通过。普通生产入口实际走通选将、八席和双席对局、终局与重开。640×272 的观星界面可同时看到两组牌与确认按钮，提交的顶牌在自然摸牌阶段进入手牌。

重铸、属性、两席拼点、巧说、忠义、阶段、多席拼点、明典、恂恂、威重十个非默认 Cargo feature 分别编译并通过牌桌烟测。忠义和明典的普通局检查曾在独立导入的 WASM 绑定上过早调用构造函数，修正初始化后用相同的绑定重跑通过。回放另在两个独立浏览器上下文中核对六个前缀的八席与旁观视图。
