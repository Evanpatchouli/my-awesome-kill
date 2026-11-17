# Rust 对局核心

`Game::classic(seed, human_seat, hero_id, seat_count)` 从标准武将和牌堆创建 2–10 人普通身份局。`classic_data::roster()` 提供选将数据和技能原文。固定种子决定身份、其他武将和牌堆顺序。主公先行，各座初始摸四张，五人以上时主公体力上限加一。

`Game::submit` 校验带版本的指令后推进到下一个选择或终局。`Game::view` 公开牌区、当前阶段和等待席位，向观察席提供自己的身份、手牌和可选项。主公和已阵亡角色的身份公开，完局公开所有身份。`Game::bot_command` 给出一条常规指令，外层可以按呈现节奏提交。`Game::new(GameSetup)` 继续作为直接构造杀与闪结算的测试入口。

`events` 包含开局或最近一次接受指令产生的事件。回放按版本累计批次，事件开始与结束共享 ID，子事件保留父 ID。非法或过期指令维持状态与事件批次。`card_faces` 提供观察视角当前牌区和该批事件中公开实体牌的花色及点数。

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

```sh
cargo test --manifest-path packages/duel-core/Cargo.toml
```

### 八人局测量

```sh
cargo test --release --locked --manifest-path packages/duel-core/Cargo.toml --lib benchmark_seeded_eight_player_games -- --ignored --nocapture
```

Windows 10.0.26200、Intel i7-13620H，按种子 1–100 运行八席 AI 局。100 局均结束，共处理 18,440 次决定，每局中位数 174 次，范围 39–390 次。AI 生成命令和核心执行合计 p95 为 15.3 微秒，单席投影及 JSON 序列化合计 p95 为 19.7 微秒。最大单批投影有 167 条事件，共 20,851 字节。每个终局核对 104 张实体牌各处于唯一牌区，结算区为空。
