# P1 对局底座迁移与验收

目标为在随心杀中重建 noname 的通用对局机制，再用新接口承载经典身份模式。上游固定为 `c73270948d72949aa80009536b9fd5400767d091`，下表的路径相对 `.reference/noname/apps/core/`。各行是需要逐项展开的机制族。上游版本有变动时，应重新检查差异。用户确认内容可适配新的接口，既有 JS 脚本执行兼容层不在需求内。

### 完成证据

每个机制须关联具体上游入口、输入局面、选择序列、关键时机与各观察者的状态投影。迁移后的自动测试要检查中途状态以及最终状态。静态阅读、原版运行观测和新实现运行结果分别标注；有意改变行为须记录旧行为、理由以及回归测试。发布前检查机制族是否遗漏，另从首页的经典模式入口验收多人局和完整选将范围。现有两将双人 TypeScript 应用为回归基线。

| 机制族 | 上游定位 | 要验证的关键交互 |
|---|---|---|
| 事件与时机 | `noname/game/index.js` createEvent；`noname/library/element/gameEvent.ts` start/loop/waitNext、cancel、insertAfter；`GameEvent/GameEventManager.ts`（同目录） | 父事件挂起、子事件结果回传、Before/Begin/End/After、取消、跳过、追加事件与清理顺序 |
| 技能派发与规则修饰 | `gameEvent.ts` trigger；`element/content.ts` arrangeTrigger/createTrigger；`game/index.js` checkMod；`library/index.js` cardEnabled/cardUsable/targetEnabled/targetInRange | player/source/target/global、座次、firstDo/lastDo、同优先级排序、临时技能、发动成本、范围和目标合法性 |
| 玩家选择 | `element/player.js` chooseToUse/chooseToRespond/chooseToDiscard/chooseToCompare；`content.ts` 相应执行体 | 卡牌/角色/选项/组合、强制/可选、代答、多张同时确认、取消与重试、超时或 AI 接管、过期命令拒绝 |
| 牌与区域 | `content.ts` useCard/respond/gain/lose/addToExpansion/orderingDiscard；`element/card.js` 与 `vcard.js` | 实体和转化牌、成本与所有权、牌堆重洗、结算区、装备替换、延迟锦囊、特殊/扩展区，公开与私有移动 |
| 轮与回合 | `content.ts` phaseLoop/phase/phaseJudge/phaseDraw/phaseUse/phaseDiscard | 准备至结束阶段、跳过与额外阶段、插入回合、死亡移座、回合和轮次历史、使用次数重置 |
| 体力与状态 | `content.ts` damage/changeHp/dying/die；`library/skill.js` _lianhuan、_lianhuan4 | damageBegin1–4 与数值修改、护甲、体力流失与回复、属性与横置、濒死多人求桃、死亡清理 |
| 经典模式 | `mode/identity.js` chooseCharacter/checkResult/dieAfter/dieAfter2；`noname/get/index.js` identityList | `game/config.json` 的普通身份表（2–10 席，默认 8 席）与 get.identityList 配合；隐藏身份与主公公开、主公加成、身份配比、主忠反内胜负、击杀反贼奖励和主杀忠惩罚，死亡与判胜先后 |
| 经典内容 | `character/standard/sort.js` standard_2008；同包 character/skill/translate；`card/standard.js` | 25 将均能自选并发动符合局面的技能；104 张实体牌及装备/延时牌。重点覆盖鬼才、激将、流离、无双等跨角色互动 |
| 视角、AI 与呈现 | `mode/identity.js` logAi/rawAttitude；`content.ts` 手牌展示；`game/index.js` 日志和录像；`element/player.js` 牌及伤害动画 | 本人/对手/旁观投影、AI 只消费可用信息、事件重放、摸牌牌背、出牌和响应、装备/判定常显、伤害/回复反馈及减动效 |
| 跨内容包通用能力 | `card/extra.js` 铁索与重铸；`library/skill.js` 属性传播；`mode/guozhan/src/patch/player.js` 暗将；标准包以外技能的扩展区用法 | 用小型用例验证拼点、重铸、多个目标、连环、明置/暗置、临时与复合技能等。无需向玩家开放额外玩法入口 |

### 第一条对照轨迹

不带武将技、武器或防具。A 有可用次数且 B 在范围内。A 对 B 使用一张实体杀，B 选择打闪或放弃。记录使用前合法目标、实体从手牌至结算区、指定目标和应答时机、哪名玩家能看到闪、伤害或取消伤害、清理结算牌、再次使用杀的合法性。恢复旧命令不应重复执行。多人时须验证攻击目标可以是任一合法座位，应答仅向目标开放，其他角色的触发机会仍须依次计算。

上游静态线索：`content.ts` 的 useCard 与逐目标处理、`card/standard.js` 的 sha、`content.ts` 的 damage 和 orderingDiscard。Rust 的初始可执行切片只覆盖上述轨迹的部分节点，在范围、使用次数、触发器及身份规则完善后再迁移整个模式入口。
