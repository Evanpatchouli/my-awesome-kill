# Rust 对局核心

`GameSetup` 用向量定义座次顺序，`Game::seat_order_from` 从指定座位绕桌一周。`Game::submit` 提交带版本的指令并推进到下一个选择点或本次结算结束。指令校验在状态修改之前。`Game::view` 返回公开状态，以及观察座位自己的手牌和可选项。事件开始与结束使用同一 ID，子事件保留父 ID。

当前可执行路径覆盖实体杀、对目标的闪选择、实体闪抵消、放弃时造成一点伤害，以及实体牌从手牌经结算区进入弃牌堆。目标没有闪时也需要明确放弃，其他席位始终只看到等待中的席位。当前由 `GameSetup` 指定行动席，回合、目标距离、用杀次数、技能以及濒死仍待实现。

## 行为对照

固定参考版本为 `.reference/noname` 的 `c73270948d72949aa80009536b9fd5400767d091`。

| 上游路径与符号 | 此纵切面的可验证行为 |
| --- | --- |
| `apps/core/noname/library/element/gameEvent.ts` 的 `next`、`after`、`waitNext`、`loop`、`finish`、`cancel` | 按顺序结算嵌套子事件。挂起期间保留事件 ID 和父链。事件结束或取消后继续结算清理队列。 |
| `apps/core/card/standard.js` 的 `card.sha.content`、`card.shan.content` | 普通杀询问目标用闪。闪标识本次杀未命中，放弃则结算一点伤害。 |
| `apps/core/noname/library/element/content.ts` 的 `chooseToUse`、`useCard`、`lose`、`orderingDiscard`、`damage` | 已使用的实体牌留在结算区，完成对应事件后清理；伤害发生在防御选择完成之后。 |

```sh
cargo test --manifest-path packages/duel-core/Cargo.toml
```
