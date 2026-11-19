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

上游静态线索：`content.ts` 的 useCard 与逐目标处理、`card/standard.js` 的 sha、`content.ts` 的 damage 和 orderingDiscard。上游真实局已采集 83 次时机调用；Rust/WASM 已比对其中六个共同节点及最终局面。经典身份模式已接入应用。继续迁移的通用机制、候选对照用例与范围见[覆盖盘点](noname-coverage.md)。

### 拼点后的临时规则与用牌目标（Q2b）

两席私选、双方共同翻牌、有效点数、结果归属和可取得的实体由 `TriggerContext::compare` 及其续步提供。下面四项是 Q2b 的验收口径。平负到期和胜方修改下一次真实用牌的断言已落在 `classic/compare/tests`，覆盖边界见[对照盘点](noname-coverage.md)和[巧说、纵适原版三局](../../../tests/reference/compare.json)。

1. 从自然身份局的 Turn/Phase 发出 `phaseUseEnd`、`phaseEnd`、`phaseAfter` 及相应的 `phaseBeforeStart`。临时登记应能绑定持有席、时机和到期清理。先写失败用例检查：平局/负局保留 `qiaoshui2` 到 `phaseAfter` 请求，其首个实际回调执行前已到期，同席原有永久技能仍可用。降级或移除临时登记时，已经入队但尚未执行的候选仍须重验。
2. 让实际 `play_choices`、`plan_play`、转化用牌和需要采用 `cardEnabled` 的响应路径查询同一规则。巧说失利期间基本牌可用，锦囊无法使用。重铸的权限由它自己的规则提供。
3. 在真实用牌帧上提供 `useCard2` 及受检的增删目标。保留同一主体 ID、支付和清理。杀在支付后至逐目标结算前、锦囊在建立逐目标无懈窗口前处理。采用上游 `targetEnabled2` 的局部语义，区分牌的固有目标限制和距离。借刀的持刀者和受杀者分别校验。胜方临时技能在下次合格用牌时先消耗，放弃修改也只消耗一次。
4. 在同一张八人桌上核对下张杀可新增原先不在攻击范围内的 D，B、D 各扣一点，随后决斗仅指定 B。平/负只能指定 B，锦囊限制在回合末解除。补测取消、移除多目标之一、死席和重复目标；用实际浏览器入口与既有 100 局回归收口。

上述到期机制也为忠义提供 `roundStart` 清理入口。该用例还需要通用扩展区、实体标签、限定使用、基于模式的友方判定及可修改的伤害数值，见[跨轮实测](../../../tests/reference/zhongyi-expansion.json)。其他机制族仍按[覆盖盘点](noname-coverage.md)逐项推进。

### 忠义的实体扩展区和跨轮结算（X2）

1. 从真实出牌选择和提交授予、发动限定主动技能。材料为自己的任意一张手牌或装备。`useSkillBegin` 尚未消耗限定次数，正文先消耗本局次数并授予临时效果，再建立 `addToExpansion` 子事件。取消或未通过的指令保留版本与材料。
2. 扩展区复用实体 ID，记录公开的牌及各标签。`addToExpansionBefore` 仍见原区域，实际 `loseAfter` 见中转区，`addToExpansionAfter` 已在 x 并带标签。投影、已知牌面、装备与手牌失去技能、死亡清理和全局守恒都须包含对应移动。中转或到期前实体可能已被其他技能取走。
3. 在 `damageBegin1` 的真实伤害父帧判断是否由杀直接造成，复用该帧的有效来源和模式队伍。固定上游普通身份局无 `side` 时只认自身，黄金局显式设置两队。后续技能、护甲、实际扣血、历史和传导共用修改后的数值。连环传导保留杀的牌实体，但父事件是连环。
4. 下一轮 `roundStart` 请求仍见 x、标签和临时效果。撤销授予后排入 `loseToDiscardpile` 并在首个观察回调前完成。只弃仍在该席扩展区且具有相应标签的实体。限定状态跨轮保留。正常八席 UI 与非默认 WASM 工厂共同验证公开信息、实体牌区、选择提交和实际伤害；独立验证取消、转移及多持有者边界。

### 自然与额外阶段、回合（P3）

持续的 `PhaseLoop` 保存自然席次。普通与额外回合共用子帧实现，回合内的阶段计划可由规则修改。`phaseBefore` 请求时读取旧计数，翻面取消可以增加回合号，额外回合的注册来源不触发自然过轮。每个实际出牌段在 `phaseUseBefore` 前重设用量。待办额外回合全部结清后才向原自然席发出 `phaseOver`。各项时机须读取实际父事件、物理牌区与私有应答。[当先与放权十个完整回合](../../../tests/reference/phase-scheduling.json) · [队首、队尾与阵亡席](../../../tests/reference/phase-queue.json)

后续将阶段数量与物理判定材料的移动逐族核实。正常身份局、非默认规则工厂、玩家提交与 AI 都须走同一份合法候选和实际结算。

### 多席密封选择与两种拼点（Q3）

两席既有行为、多人共同选牌与批量支付使用同一组失牌和按根归属清理的原语。鼓舌逐对揭晓，胆烈一次共同揭晓。多人根需要分别保存局部工作点数、已提交的结果及作废状态。固定上游在胆烈的 fixing 前保存分数，取消支付后尚未展示的共同根保留空分数数组。正常八席桌、真实主动技能、各席可见性和牌桌交互分别验证，不从两席试验推导多人结论。[多人原版五局](../../../tests/reference/compare-multiple.json)

### 一次重铸多张实体牌（R3）

固定上游六局、190 条观测。前三张依次来自已知手牌、暗手牌和装备。默认只建一个重铸根，一个失牌包装及其 `lose`，一个 `draw(3)` 及其 `gain`。两次工厂分别只执行一次，收到同一份原始有序列表。在内层失牌前截走第二张时，`loseAfter` 只含实际移走的第一和第三张，获得工厂仍收到三张。整次底层失牌被取消也仍摸三张；只取消摸牌则已弃材料留在弃牌堆，三张替代牌保留原堆顺序。[六局批量重铸](../../../tests/reference/recast-batch.json)

实施时先写失败用例，从真实注册主动技一次选择两张手牌和一件装备，按上述截获场景核对根、工厂调用、原选择与实际失牌、一次摸三张、各席知情和守恒。保留现有普通严格重铸动作及 `RecastRule` 单张接口。可信批量入口验证材料唯一、归属和费用冲突；复用同一个重铸状态机及按真实区域执行的移动，不把一批拆成多个根。批次公告和手牌知情记录各自独立。公开替代样例的 `gain2` 公开整批替代牌，另有原版明典的真实多选入口。另两局实测了失牌包装中断与全部截获。取消包装时没有内层 lose 或失牌历史；若三张都在内层 loseBefore 被转走，原 lose 以空 cards 和原 stockcards 正常发 End/After，并写入失牌历史。两种情况下获得工厂均收到最初三张，默认摸三张。[62 条包装与空失牌观测](../../../tests/reference/recast-batch-boundaries.json)

### 有序分区选择

1. 八席真实观星与恂恂分别把五张或四张实体交给顺序选择。观星提交的两行依次为 `[c2,c0]` 与 `[c4,c1,c3]`。顶行在父结果中逆序，物理堆顶和随后的自然摸牌仍按 `[c2,c0]`。恂恂的原规则拒绝第三张迁入目标行，只确认恰好两张的分组。[上游轨迹](../../../tests/reference/ordered-move.json)
2. 选择使用独立帧及受版本约束的提交。提交按序携带实体、目标行及插入位置。Rust 复核每一步的身份、位置与移入条件，再复核最终容量和确认条件。选择席可读完整布局和牌面，其他席可读等待中的席位。可信技能和 AI 走同一规则，技能取得结算后的有序分区。
3. 非强制放弃正常返回空结果并执行结束时机。子选择在 Begin 显式取消时先发 Cancelled，父观星正常返回空两行，仍归该父事件处理的实体随后一起清理。实际区域的移动由调用者结算，独立验证实体守恒、截获和私有预览。正式观星与额外恂恂分别通过真实 Vue/WASM 入口验证。

### 复活、死亡清理与名单

Rust 的八席身份局已对照普通死亡后复活、重复调用、零或负上限与事件版取消。下述行为由注册技能的实际提交验收。[复活轨迹](../../../tests/reference/revival.json)

1. 从普通注册技能及 `Game::submit` 建立可继续的八席局。非主公目标持一张手牌和一件装备，身份待揭示，处于横置和翻面状态。完整死亡后，核对实体去向、身份公开、标记的显示与存储、存活与死亡名单。追加测试在 `die` 的真实回调中立刻复活目标，检查跳过亡者清理后的牌与技能，并核对原死亡的击杀归属和奖惩。
2. 同步的 `revive(hp)` 在当前回调立即生效。上限至少为 1，当前体力采用传入值，省略或零取 1，再按上限夹断。重复给存活角色传入负值时仍可保持存活。原角色和公开身份延续，已弃置的牌仍在实际区域。复活者追加到存活名单尾部，活席环回到其原座位与最近存活邻座之间。
3. 事件入口 `reviveEvent` 共用恢复操作，发 Before、Begin、End、After 或 Cancelled。用独立非默认内容验证复活前取消及成功后自然入轮。区分标记存储与界面显示，按实际 `onunmark` 和临时技能清理规则处理。轮/阶段倒计时、出局标志和 `restEnd` 调度属于后续休整单元。

补充实测中，真实鼓舌在死前、完整死亡后及正常复活后的存储计数均为 7，标记分别为显示、隐藏、隐藏。在另一名反贼的 `die` 主时机同步复活，手牌、装备、七个标记和临时技能仍保留，来源已取得一次击杀归属，并在 `dieEnd` 前摸到三张奖励牌。记录分别保留初始名单、复活后实际追加次序和按座位连接的存活环。[复活与奖惩观测](../../../tests/reference/revival.json)
