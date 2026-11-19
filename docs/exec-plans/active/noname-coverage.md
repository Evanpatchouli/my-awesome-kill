# 上游对局机制覆盖盘点

参考提交 `c73270948d72949aa80009536b9fd5400767d091`。以下是对当前 Rust 经典模式及上游源码的盘点。**尚未通过全底座等价验收。** 经典模式已运行的用例、源代码中出现的机制、完整上游能力是三个不同的证据层级。

## 现有证据

- Rust 调度器已运行嵌套事件、按序子事件和 after 收尾、暂停/恢复、取消、版本拒绝与不同席位投影。普通身份局支持六阶段、存活距离、主公与隐藏身份、求桃、死亡、奖惩和终局。2/4/8 席完整局检查实体守恒，单独的八人性能测量覆盖 100 局。
- 全部 standard_2008 25 将（40 个技能）及 104 张实体牌已对照数据。武将能力、锦囊及装备在同一调度器内运行。用例包含替换装、双目标借刀、虚拟材料、无懈链、全桌求助、改判与私看、用杀次数与材料成本。运行 `cargo test --locked --manifest-path packages/duel-core/Cargo.toml`，查看针对具体行为命名的测试。
- 上游原版八人身份局实跑一次物理杀、目标无闪，记录 83 次时机请求。Rust/WASM 对照其中六个里程碑及 HP、牌区。请求 `damage` 时目标仍为 4 HP；另一条受伤技能实测显示，对应回调开始时 `changeHp` 已将体力减至 3。`useCardAfter` 时杀仍在处理区，随后移入弃牌堆。[原版对照步骤](../../../tests/reference/README.md)
- 独立上游局记录了取消伤害的 126 次时机调用。A 的同优先级选项为 Alpha、Beta，实选 Beta 后，原先可用的低优先级候选被重新筛除。随后 B 支付一张桃，A 的 lastDo 取消伤害。B 仍有 4 体力，杀已计次，`useCardAfter` 与后续弃置都发生。记录同时保留技能事件的父事件、被触发的伤害及其父事件。见[时机与取消轨迹](../../../tests/reference/trigger-order-cancel.json)。
- 针对带 1 护甲的横置角色，原版另有 76 条体力子事件取消观测。取消 `changeHpBefore` 会保留其体力、护甲和横置，父伤害的来源和伤害后时机继续。取消 `changeHpEnd` 会保留已扣体力、已消耗护甲和解链，随后传导同一实体火杀的两点伤害。[体力子事件取消轨迹](../../../tests/reference/hp-cancel.json)
- 编译后的 Rust/WASM 经真实 Vue 入口运行。种子 20 验证 25 将筛选、赵云、多人隐藏信息、整组弃牌、牌移动与受伤、换装、响应、退出/重开；另测诸葛亮观星与刘备仁德。核心与 UI 独立复核发现的无懈窗口、连弩材料、技能发动日志问题已修复。测试结果详见[运行手册](../../runbooks/testing.md)。

## 入口清单

`apps/core/noname/library/element/content.ts` 在固定版本有 **128 个顶层执行体**：93 个 async 入口、35 个数组入口。名称中有 34 个以 choose 起头。此索引只覆盖该文件；调度器、player/game、规则技能、AI 与模式入口另计。包含同一件事的不同包装，不能用入口数直接计算迁移百分比。

| 族 | 个数 | 上游入口 |
|---|---:|---|
| 派发 | 3 | emptyEvent, arrangeTrigger, createTrigger |
| 开局、轮、阶段与终局 | 11 | gameDraw, phaseLoop, phase, phaseZhunbei, phaseJudge, phaseDraw, phaseUse, phaseDiscard, phaseJieshu, forceOver, replaceHandcards |
| 选择、比较、发现 | 35 | chooseNumbers, chooseToDebate, chooseCooperationFor, chooseToPlayBeatmap, chooseToMove, chooseUseTarget, chooseToDuiben, chooseToPSS, chooseToEnable, chooseToDisable, chooseToUse, chooseToRespond, chooseToGive, chooseToDiscard, chooseToCompareLose, chooseToCompareMeanwhile, chooseToCompareMultiple, chooseToCompare, chooseToCompareEffect, chooseSkill, discoverCard, chooseButton, chooseCardOL, chooseButtonOL, chooseAnyOL, chooseCard, chooseTarget, chooseCardTarget, chooseButtonTarget, chooseControl, chooseBool, chooseDrawRecover, choosePlayerCard, chooseToGuanxing, chooseToMove_new |
| 区域与转移 | 24 | cardsDiscard, orderingDiscard, cardsGotoOrdering, cardsGotoSpecial, cardsGotoPile, gaincardMultiple, discardMultiple, addToExpansionMultiple, loseToDiscardpileMultiple, discardPlayerCard, gainPlayerCard, moveCard, draw, discard, loseToDiscardpile, swapHandcards, swapHandcardsx, gainMultiple, gain, addToExpansion, lose, gift, recast, doubleDraw |
| 实体、装备、判定、用牌 | 22 | connectCards, resetConnectedCards, addShownCards, hideShownCards, executeDelayCardEffect, disableEquip, enableEquip, expandEquip, replaceEquip, equip, swapEquip, disableJudge, enableJudge, addExtraTarget, reverseOrder, addJudgeCard, equipCard, useCard, useSkill, respond, addJudge, judge |
| 角色与状态 | 24 | _save, changeCharacter, changeSkills, changeGroup, showCharacter, removeCharacter, phasing, toggleSubPlayer, exitSubPlayer, callSubPlayer, damage, recover, loseHp, loseMaxHp, gainMaxHp, changeHp, changeHujia, dying, die, rest, restEnd, revive, turnOver, link |
| 亮牌与查看 | 3 | showHandcards, showCards, viewCards |
| 加载、会话与呈现 | 6 | loadPackage, loadMode, playVideoContent, waitForPlayer, replaceHandcardsOL, delay |

## 仍需迁移或证明的能力

1. **一般事件时机。** Rust 已提供按名称和 player/source/target/global 角色注册的实际技能派发，在普通出杀、受伤、阶段和判定等已接入时机运行。已验证 first/last、按座次选序、逐次重验、开放组动态增删、真实支付与伤害取消。其余事件须逐个落实真实的 Before/Begin/End/After/Omitted/Skipped/Cancelled、临时技能到期、额外回合、插入/跳过阶段、事件历史、neutralize/unneutralize。依据为 gameEvent.ts 的 loop、trigger、insert/insertAfter、checkSkipped、untrigger、cancel 以及 content.ts 的 arrangeTrigger/createTrigger。
2. **牌及共享区域。** 处理区在途、普通区和标准延时牌已有用例。继续验证批量同时转移、洗牌耗尽、扩展区与标签、重铸、同时暗选再亮牌、装备栏/判定区禁用、扩展装备位与重排、动态目标和跨来源判定。
3. **角色生命周期与修饰。** 已接入数值护甲、火雷伤害、横置传导、翻面跳回合及本局伤害历史。继续验证多重和其他性质、忽略护甲的修饰、体力上限改变、角色重生/休整、明置/暗置、副将和更换势力、限定与觉醒、临时技能到期及其组合。曹操的护驾技能与数值护甲分别测试。
4. **投影、重放与模式接口。** 已有伤害、拼点和部分实体移动历史。录制宿主可从指定初始局面或种子恢复任意已接受的命令前缀，并分别取得八席与旁观视图。独立内容工厂使用显式标识，重新安装同版本的规则与初始状态。完整的模式无关事件历史、录像呈现、其他私有事件及联合选择仍需逐项验收。原版联机运输和沙盒按对局行为边界处理。[回放验收](../../runbooks/testing.md)

## 跨内容探针与迁移状态

下列用例使用固定的原版内容和独立诊断局面。每项单列上游观测、Rust 对照及剩余工作，玩家入口保持普通经典身份局。

- **一般派发与取消。** 同席选序、跨席高优先级、firstDo/lastDo、费用、取消与 after 清理已用上游轨迹对照 Rust。target 角色、公开入口、动态增删与麒麟弓有针对性回归。阶段跳过与中途取消仍需独立场景；`loseAfter` 等跨动作时机的通用数据须随对应机制继续接入。
- **铁索与法恩。** Rust 已按三个真实上游回合逐行对照 106 条请求和执行观测。B 的一甲可以完全吸收火杀，B 仍解除横置，并向 C 传导原先的一点火焰伤害。第三回合待传播的 D 先被解除横置，受伤历史为零。轨迹包含同一实体杀、来源、座次、法恩的实际选择与摸牌。另按 76 行对照体力子事件取消。雷击和翻面跳回合另有 Rust 回归，法恩的选择使用已录决定。[连环伤害轨迹](../../../tests/reference/linked-damage.json)
- **巧说与纵适。** 三个自然回合的改点胜、平和负有 104 条观测。Rust 已逐行比对其中 51 条拼点请求，以及另一份 78 条支付与知情边界观测。平负局的锦囊禁用维持到 `phaseAfter` 请求，首个观察回调可再使用决斗。胜局额外杀到通常距离外的 D，B/D 均受伤，随后决斗只有 B。另一份上游 105 条观测记录了借刀排序后的按位询问、南蛮减一、铁索放弃改动，以及主动闪/无懈和被动打出杀的区别。Rust 八席测试比对上述选择、支付及终态。多席、牌堆拼点与其他比较修饰仍待核对。[巧说拼点三局](../../../tests/reference/compare.json) · [支付和知情](../../../tests/reference/compare-boundaries.json) · [巧说目标与响应](../../../tests/reference/qiaoshui-targets.json)
- **逐目标用牌时机。** 普通即时锦囊按座次逐层发出 `useCardToPlayer`、`useCardToTarget`、`useCardToPlayered`、`useCardToTargeted`；八席借刀测试逐条对照上游目标时机和父子事件。延时锦囊、判定、基本牌及装备的对应四层流程仍待单独取证和接入；杀现有原生时机保留原路径。
- **忠义扩展区。** Rust 自然八席已逐行对齐跨两轮的 42 条观测。主动限定技可选择手牌或装备，同一实体经历失牌、特殊区、标签与扩展区，下一轮的首个回调看到清理结果和仍然已用的限定次数。47 条补充观测覆盖主动选择、转出后持续增伤、外层与内层取消、双持有者、虚拟火杀与传导、死亡清理。持有者死于材料移入特殊区的失牌回调另有独立对照。标准关羽仍只具武圣，额外内容通过独立工厂在正常牌桌验证。更多区域权限、批量调度、复活和模式关系继续单独核对。[自然跨轮](../../../tests/reference/zhongyi-expansion.json) · [取消、转移、死亡](../../../tests/reference/zhongyi-boundaries.json) · [中转区死亡](../../../tests/reference/zhongyi-transit-death.json)
- **铁索重铸。** 上游 `Player.recast` 默认和替换 lose/gain 的结算分别记录 30 和 21 次时机请求。Rust 已使用真实铁索逐行核对另一份轨迹的全部 377 条观测（184 次请求、177 次技能回调和 16 次工厂调用），另测公开时机、知情状态、内容发起、费用重验和子事件取消。多张同时重铸和其他移动工厂仍须扩展取证。[时机请求](../../../tests/reference/tiesuo-recast.json) · [回调、知情状态与中断](../../../tests/reference/recast-callbacks.json)

- **自然阶段、当先与放权。** 固定上游两条八席轨迹分别有 494、112 条观测。Rust 自然局组件比对第一条中的 154 条阶段请求/回调，另验阶段来源、费用、标记、实际用牌和各段用杀次数。队列场景核对 102 条回合/轮转时机、4 条死亡时机与 6 项操作的共同字段，涵盖队首前插、嵌套追加、等待期间死亡及自然席次恢复。Rust 通过濒死进入死亡，上游队列夹具直接调用死亡。判定区实体和无懈支付另有独立的物理轨迹。[阶段与回合](../../../tests/reference/phase-scheduling.json) · [队列边界](../../../tests/reference/phase-queue.json)

- **延时锦囊与判定区。** 六个八席样例覆盖乐不思蜀成功/失败、天妒、闪电命中/移交与两种锦囊各自被无懈。Rust 比对 158 行中的每个时机请求、首位回调及内容首尾，分别核对延时实体和翻开的判定牌的身份、位置、所有者与结果。无懈经真正的反制窗口发起选择并支付；天妒通过获得事件收取翻牌。实体仍在对应处理区保留组时才会弃置。[判定区的两张牌](../../../tests/reference/judgment-zones.json)
- **多人拼点。** 上游鼓舌与胆烈五局共 323 条观测。Rust 的真实四人拼点比对了其中 173 条时机请求和全部 118 条首位回调，检查主体与父事件、角色、四张实体的位置及有效点数，另验原始和排序目标、局部及存档赢家、真实结果数组。其余 32 条包括展示、技能内容首尾、点数探针和技能后续伤害或失去体力的时点。公开可见性、技能后续效果、逐对暂停、败方私选、单席或整批失牌取消及根取消有单独行为回归。五局及原两席采样范围以外的固定结果、牌堆代付及联机运输仍需另验。[多人拼点](../../../tests/reference/compare-multiple.json)

按探针扩展通用接口，每个可验证部分按 AGENTS.md 提交，并继续更新此表。上述探针通过以后仍须逐族对账并完成真实应用验收。

- **批量重铸。** 上游六局 190 条，另两局 62 条涵盖失牌包装取消与全部被截获。Rust 八席通过正式主动技提交，逐行对照其中七个非明典场景的 211 条共同请求、回调和工厂观测。核对材料、实际物理区域、失牌记录及对应轨迹提供的手牌顺序、弃置顺序和各席知情。默认一次摸牌、一次获得。另验明获、取消获得后恢复牌堆顺序、受赠席死亡后实体留在原特殊区。单张重铸的 377 条继续通过。明典从正常出牌及伤害后进入实际选牌，同一轮记录已重铸牌名，使用过的对应基本牌允许按规则公开额外获得实体。独立 WASM 的真实八席牌桌实测人类选两张普通杀，随后实跑 AI 发动。[批量参考轨迹](../../../tests/reference/recast-batch.json) · [取消与全部截获](../../../tests/reference/recast-batch-boundaries.json) · [给牌中转死亡](../../../tests/reference/batch-give-death.json) · [牌桌复现](../../runbooks/testing.md)
- **有序分区和私有牌堆预览。** 上游 82 条请求覆盖两种实际内容、选择取消后弃置、正常放弃。Rust 通用选择通过实体 ID 和逐步插入命令校验布局，拒绝非法步骤且保留待选状态。AI 计划与普通提交共用校验，局终及无法安排的强制 AI 选择会取消。随心杀的牌堆预览通过私有处理区保留真实实体，核对上下行顺序、实际摸牌、选牌席和旁观席的投影。取消后的清理只操作其仍持有的牌，若清理也被取消则依原序恢复。其他卡牌移动和任意选项分区继续单独覆盖。[有序分区上游轨迹](../../../tests/reference/ordered-move.json)
