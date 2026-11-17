# Rust 对局核心

`Game::classic(seed, human_seat, hero_id, seat_count)` 从标准武将和牌堆创建 2–10 人普通身份局。`classic_data::roster()` 提供选将数据和技能原文。固定种子决定身份、其他武将和牌堆顺序。主公先行，各座初始摸四张，五人以上时主公体力上限加一。

`Game::submit` 校验带版本的指令后推进到下一个选择或终局。`Game::view` 公开牌区、当前阶段和等待席位，向观察席提供自己的身份、手牌和可选项。主公和已阵亡角色的身份公开，完局公开所有身份。`Game::bot_command` 给出一条常规指令，外层可以按呈现节奏提交。`Game::new(GameSetup)` 继续作为直接构造杀与闪结算的测试入口。

`events` 包含开局或最近一次接受指令产生的事件。回放按版本累计批次，事件开始与结束共享 ID，子事件保留父 ID。非法或过期指令维持状态与事件批次。`card_faces` 提供观察视角当前牌区和该批事件中公开实体牌的花色及点数。

当前身份局覆盖准备、判定、摸牌、出牌、弃牌和结束阶段。杀按存活座次计算距离，普通出牌阶段限用一张。闪应答取消该杀的伤害。桃可在出牌阶段治疗自己或在濒死时救人。弃牌阶段一次提交整组选中牌。用过的实体牌在对应事件结束后离开结算区，摸牌堆耗尽时洗回弃牌堆。弃牌堆也无法补足所需牌数时判平局。濒死从当前回合的存活角色起逐座求桃，失败则公开阵亡者身份、弃置其牌，检查胜负并结算继续进行时的击杀奖惩。

所有标准实体牌都能摸取和弃置。当前出牌候选只包含已实现行为的种类，装备、锦囊和武将技能留待接入。判定阶段已有时序，延时锦囊仍待接入。

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

为保持隐藏手牌的视角一致性，杀和濒死响应即使没有符合条件的牌也提供放弃。上游 `_save` 会依据 `canSave` 跳过没有合法牌的角色。引擎将置入弃牌区的死亡清理及当前结算清理完成后再发布终局，终局后冻结命令。

```sh
cargo test --manifest-path packages/duel-core/Cargo.toml
```
