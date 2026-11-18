# Rust 对局核心

`Game::classic(seed, human_seat, hero_id, seat_count)` 从标准武将和牌堆创建 2–10 人普通身份局。`classic_data::roster()` 提供选将数据和技能原文。固定种子决定身份、其他武将和牌堆顺序。主公先行，各座初始摸四张，五人以上时主公体力上限加一。

`Game::submit` 校验带版本的指令后推进到下一个选择或终局。`Game::view` 公开牌区、当前阶段和等待席位，向观察席提供自己的身份、手牌和可选项。主公和已阵亡角色的身份公开，完局公开所有身份。`Game::bot_command` 给出一条常规指令，外层可以按呈现节奏提交。`Game::new(GameSetup)` 继续作为直接构造杀与闪结算的测试入口。

`events` 包含开局或最近一次接受指令产生的事件。回放按版本累计批次，事件开始与结束共享 ID，子事件保留父 ID。非法或过期指令维持状态与事件批次。`card_faces` 提供观察视角当前牌区和该批事件中公开实体牌的花色及点数。

### 从 Rust 注册技能时机

`Game::register_trigger` 接受 `TriggerRegistration` 并返回可撤销的句柄。内容指定持有席、技能标识、时机名称、匹配的 player/source/target/global 角色、first/normal/last 组、优先级、显示名、资格谓词和结算回调。`Game::classic_with_triggers` 可以在经典开局之前安装注册项。回调可在结算中注册、撤销技能或通过 `TriggerContext::emit` 派发内容自定义时机。

普通组从回合角色起逐席处理。每轮重验当前组的候选后，处于最高有效优先级的不同具名技能可由持有者通过 `Action::Select` 排序，只有持有席能看到选项。首末组按有效优先级、座次自动结算。新增候选可进入尚未走完的组。内置伤害技及麒麟弓也经过这个顺序，已消耗的候选按同一次时机记录。

回调用 `TriggerStep` 指定结束、选择、下一阶段或取消当前被触发事件。`TriggerContext::discard_hand` 校验整批实体，在先前子事件完成后再次核对所有权，再实际付款。中途已被移走的材料会终止后续付款和技能正文，先前已经结算的费用保留。`damageBegin4` 取消可阻止扣血；`damage` 技能执行时体力已更新，此时取消会保留伤害造成的体力变化。两条路径的杀都在 `useCardAfter` 之后完成实体清理。[可复现的上游局面](../../tests/reference/README.md)

### 单张重铸

可信内容可用 `Game::register_recast` 按牌种或实体 ID 登记重铸，实体登记优先。出牌阶段仅向有权的持有者列出可用的 `Action::Recast`。技能在其他时机也可通过 `TriggerContext::recast` 发起同一结算。谓词与移动工厂读取 `RecastState` 中的牌、位置、座位和回合信息。默认规则弃掉材料再摸牌；内容可替换为交给另一席或明获指定实体。经典牌堆仍为 104 张。

原手牌在通过 `recastBefore` 和 `recastBegin` 后才公开。暗摸的替代牌只对本人可见，明获的牌通过 `knownHand` 可见，离开手牌后清除这项知情状态。内层失去、摸牌和获得的取消分别结算，已排队的子事件按顺序运行。电脑会在缺乏更好出牌时尝试一次重铸；即便该次被取消也可结束出牌。[原版请求与实际执行对照](../../tests/reference/recast-callbacks.json)

当前身份局覆盖准备、判定、摸牌、出牌、弃牌和结束阶段。杀按存活座次计算距离，普通出牌阶段限用一张。闪应答取消该杀的伤害。桃可在出牌阶段治疗自己或在濒死时救人。弃牌阶段一次提交整组选中牌。用过的实体牌在对应事件结束后离开结算区，摸牌堆耗尽时洗回弃牌堆。弃牌堆也无法补足所需牌数时判平局。濒死从当前回合的存活角色起逐座求桃，失败则公开阵亡者身份、弃置其牌，检查胜负并结算继续进行时的击杀奖惩。

出牌、转化及响应共用版本校验。内容事件可以排入子事件，等待玩家选择或更新续步。选择可声明若干分支，每个分支限定实体牌与目标的候选范围和数量。虚拟出牌沿实体牌成本结算，并保留实体原有的牌面。伤害事件先开始，体力变化由独立子事件报告修改前后的体力。

标准装备通过同一出牌入口放入装备区，同槽装备先弃置后装入新装备。范围计入武器和坐骑。青釭剑只在该次杀中忽略防具。雌雄双股剑先于响应生效，八卦阵可能用判定提供虚拟闪，青龙偃月刀与贯石斧在杀被闪抵消时处理，麒麟弓在扣体力前处理。丈八蛇矛从手牌支付两张实体牌，方天画戟在使用最后一张手牌杀时允许多选至三名合格目标。普通锦囊能按目标处理无懈，延时牌在判定阶段结算。

标准武将的主动技使用 `skillPlays` 和 `useSkill`，转化牌使用 `virtualPlays` 和 `playVirtual`。技能判断可以接在回合阶段、改判、体力变化、失牌和出牌流程。选择分支提供中文标签。观星、遗计等私有预览在决策席的 `privateChoiceCards` 和 `cardFaces` 中提供，五谷亮牌留在公开结算区。AI 仅以自己的身份、公开主公和当前桌面信息判断目标。

## 行为对照

固定参考版本为 `.reference/noname` 的 `c73270948d72949aa80009536b9fd5400767d091`。

| 上游路径与符号 | 此纵切面的可验证行为 |
| --- | --- |
| `apps/core/noname/library/element/gameEvent.ts` 的 `next`、`after`、`waitNext`、`loop`、`finish`、`cancel` | 按顺序结算嵌套子事件。挂起期间保留事件 ID 和父链。事件结束或取消后继续结算清理队列。 |
| `apps/core/card/standard.js` 的 `card.sha.content`、`card.shan.content` | 普通杀询问目标用闪。闪标识本次杀未命中，放弃则结算一点伤害。 |
| `apps/core/noname/library/element/content.ts` 的 `chooseToUse`、`useCard`、`lose`、`orderingDiscard`、`damage` | 已使用的实体牌留在结算区，完成对应事件后清理；伤害发生在防御选择完成之后。 |
| `apps/core/noname/library/element/content.ts` 的 `gameDraw`、`phase`、`phaseDraw`、`phaseDiscard`、`dying`、`_save`、`die` | 开局和各阶段的次序、超限整组弃牌、从当前行动席依次求桃、死亡移出活人座次与清理牌区。 |
| `apps/core/noname/get/index.js` 的 `distance`、`cards` | 沿存活座次取较短距离，牌堆不足时从弃牌堆补充。 |
| `apps/core/mode/identity.js` 的 `checkResult`、`dieAfter`、`dieAfter2` | 阵亡公开身份并判断胜负，未结束时再执行击杀反贼奖励与主杀忠惩罚。 |
| `apps/core/card/standard.js` 的 `bagua_skill`、`zhuge_skill`、`cixiong_skill`、`qinggang_skill`、`qinglong_skill`、`zhangba_skill`、`guanshi_skill`、`fangtian_skill`、`qilin_skill` | 武器修饰、判定、转化成本、目标数和装备主动选择；七条装备测试覆盖实体牌安装与各结算分支。 |

为保持隐藏手牌的视角一致性，杀和濒死响应即使没有符合条件的牌也提供放弃。上游 `_save` 会依据 `canSave` 跳过没有合法牌的角色。引擎将置入弃牌区的死亡清理及当前结算清理完成后再发布终局，终局后冻结命令。

每个锦囊目标以当前行动席为起点，按存活座次发出同形的无懈窗口。公开 `counterWindow` 只记录目前正询问无懈。当前决策席的 `choice.counter` 提供原锦囊、来源、目标和 `negated` 状态。有无可用实体只影响该席看到的选项。连续一轮无人回应后，按当前奇偶结算原效果。种子 21 的五谷回归核对了有牌与无牌时相同的公开状态，种子 1 回归验证 AI 保留无懈并完成自己的无中生有。

```sh
cargo test --manifest-path packages/duel-core/Cargo.toml
```

### 八人局测量

```sh
cargo test --release --locked --manifest-path packages/duel-core/Cargo.toml --lib benchmark_seeded_eight_player_games -- --ignored --nocapture
```

Windows 10.0.26200、Intel i7-13620H，按种子 1–100 运行八席 AI 局。100 局均结束，共处理 55,247 次决定，每局 p50 为 529 次，范围 63–1,211 次。决定计入所有无懈窗口，包括直接放弃的窗口。AI 生成命令和核心执行合计 p95 为 7.7 微秒，单席投影及 JSON 序列化合计 p95 为 15.8 微秒。最大单批投影有 37 条事件，最大投影为 9,876 字节。每个终局核对 104 张实体牌各处于唯一牌区，结算区为空。
