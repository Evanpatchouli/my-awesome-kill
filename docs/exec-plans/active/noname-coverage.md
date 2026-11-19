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
2. **牌及共享区域。** 批量重铸、扩展区标签、多人拼点、国色单材料与判定区废除均已有真实对照。普通分马及显式合马装备栏已有部分真实对照和隔离牌桌验证；多武器效果仍需扩展实战差分。其余批量同时转移、更多类型的牌区标记、洗牌耗尽、多材料虚拟牌及跨来源目标仍需逐项验证。
3. **角色生命周期与修饰。** 数值护甲、火雷伤害、横置传导、翻面、体力上限、死亡中断、同步/事件复活及休整计时已在八席运行。多重与其他属性、不同来源的护甲修饰、明暗置、副将和更换势力、觉醒及临时规则组合仍需迁移。曹操的护驾技能与数值护甲分别测试。
4. **投影、重放与模式接口。** 已有伤害、拼点和部分实体移动历史。录制宿主可从指定初始局面或种子恢复任意已接受的命令前缀，并分别取得八席与旁观视图。独立内容工厂使用显式标识，重新安装同版本的规则与初始状态。完整的模式无关事件历史、录像呈现、其他私有事件及联合选择仍需逐项验收。原版联机运输和沙盒按对局行为边界处理。[回放验收](../../runbooks/testing.md)

## 跨内容探针与迁移状态

- **复活。** 八席注册技能逐行核对 78 条普通死亡、同步复活和事件复活的请求与回调，并对照十个结算后局面、真实鼓舌的三个状态及死亡中复活的十一个阶段。上游另有四处取消的 35 条观测。Before/Begin 中断时目标仍死，End/After 中断时目标已恢复；死者自己的 `forceDie` 登记可以在恢复前运行。[复活及中断轨迹](../../../tests/reference/revival.json)
- **休整取证。** 原版八席的 67 次请求、67 次实际回调、56 个回合边界和 22 个状态快照包含离场身份、有限与无限计数、竞争计数器及真实殁亡。按回合计数包括额外回合，按轮计数只在本人自然回合扣减。休整中的角色仍在物理座次环，可选下家跳过该席。取消嵌套复活会保留原体力，结束休整仍然生效。[休整与殁亡轨迹](../../../tests/reference/rest.json)
- **休整落地。** 手动与自然两组 Rust 回归覆盖上述 134 条请求和实际回调、56 个回合边界和 22 个状态快照的共同字段。附加的五局对照 110 条伤害中离场的观测，六种时钟对照 30 个状态与 36 个回合边界。死亡后直接复活可留下 `undist`，在场资格、距离和时钟分别核对。普通回合与额外回合均使用真实调度器。非默认八席桌实测了公开赠牌、无限与有限休整、自然殁亡、返场及摸牌。殁亡样例使用最后两名常侍，完整党锢选人与派生技能另计。[中途离场与残留规则](../../../tests/reference/rest-interruptions.json)
- **废除判定区。** 普通、两种弃置取消和空区四局的 200 条请求与观察回调已逐行比较前十二个共同字段。废区可同时保留原判定牌与公开占位，国色实体及其判定身份各自核对；恢复区域不会返还已弃实体。原版 310 条中另有闪电路由、实际用牌和弥笃两项判定分支。单张 `addJudge` 及中断另有取证。[普通与取消](../../../tests/reference/judge-area.json) · [已支付延时牌与中途转移](../../../tests/reference/judge-interruptions.json)
- **延时牌的支付与置入。** 在 828 条补充观测中，实体乐和国色各在六种时机中断，十二局逐行核对了 408 条共同请求/回调的时机、父事件、席位、区域标志和材料位置。六种转移另逐行核对 88 条置入/失牌请求与回调。748 条支付与清理样本中，十局核对了 324 条请求/回调的七个共同字段，同时检查实际材料、知情、终态与根是否完成。另有取消收尾、收尾期间先将材料给他人、四种外部来源、死亡目标和同席转区回归。这些计数属于各自的筛选口径。[支付与清理](../../../tests/reference/judge-payment.json)
- **装备槽、合马和选择。** 默认 `Separate`，显式 `Combined`，重放保留模式，旧格式仍为分马。普通五类槽扩容、废除、恢复与双槽来源/持有者选择已有回归；登记 key 区分实体与 detached，token 仅对选择者可见。两层失牌与中断、八处选择取消、超额一次失牌和重放均有测试。固定原版合马样本 `b6f3c6b` 有 12 个状态、162 条事件；Rust 八席测试比较状态及所选装备槽、换装选择的共同请求字段，并单独覆盖不同 subtype、重复参数、负启用数、别名、坐骑距离、逐席隐私和重放。真实 Chrome 合马 QA 及截图位于 `E:\Temp\mak-combined-mount-qa-run`；范围不代表所有合马边界均已覆盖。[普通装备槽](../../../tests/reference/equip-slots.json) · [合马原版轨迹](../../../tests/reference/combined-mounts.json)
- **多武器与杀目标派发。** 原版多武器轨迹 `d47eea5` 有真实连弩、双青釭和青龙/贯石斧选序；Rust 对射程、连弩次数、两次伤害和牌区等共同字段有对照，另测实体支付与 detached 登记。原版目标与装备轨迹 `d9c90f4` 记录目标子事件、真人选序和同名来源变化；Rust 测试比较相应技能执行顺序及来源状态，并覆盖动态装卸、选序与独立目标事件。多武器青龙/贯石斧完整排序、借刀和其他目标时机仍待验证。[多武器原版](../../../tests/reference/multi-weapon-combat.json) · [杀目标与装备原版](../../../tests/reference/sha-target-equipment.json)
- **青釭标记与杀取消。** 原版 `a122ac2` / `bd12b3a` 分别记录标记清理、残留、独立来源与离场；Rust 对多目标伤害、闪、伤害取消及零伤的高低优先级回调状态作共同字段对照，另测嵌套杀与技能撤销。原版 `df192a7` 记录九种根取消时序与目标排除；Rust 对九场共同请求/回调顺序、目标、标记和材料位置作比较，另测可信根取消、单/全排除、即时请求、临时技能到期和嵌套根隔离。锦囊取消与排除仍待验证。[标记原版](../../../tests/reference/qinggang-expiry.json) · [来源原版](../../../tests/reference/qinggang-sources.json) · [取消与排除原版](../../../tests/reference/sha-exclusion.json)
- **取消请求与清理先后。** 八席 Rust 回归与固定原版比较了返场、晚注册、索引计数变化、移除、同名重加和离场期间采样六个场景的共同请求及回调字段。先取得扩展牌后取消时，接收方已取得该牌，失牌包装仍以原材料运行，内层不再重复移动。先取消再获取时，原版广义 `gain` 可从弃牌堆取得该牌；Rust 的 `gain_expansion` 在执行前重查原区并拒绝此次获得。两者在这一分支尚有范围差异。另测 `emit`、显式撤销和前序操作失败后的清理。[原版八个场景](../../../tests/reference/cancel-deferred.json)
- **单实体虚拟判定与装备登记。** 判定材料已在手中或装备区时，原判定包装仍可显示、阻止同名置入并在判定时被选中。八个获得、判定、废区及内层取消场景核对了 104 个共同请求的状态，另对照实际过河拆桥、顺手牵羊与死亡清理。若判定材料先装上后被原包装的失牌移走，真实青釭实体可在弃牌堆而原登记仍占槽。八卦与黑杀以及借刀分别验证实际防具效果、选目标资格和实体不能被重复取得。仁王样本提供上游额外对照，Rust 的这次防具实战使用八卦。[包装与材料](../../../tests/reference/virtual-judge-probe.json) · [获得或弃置判定对象](../../../tests/reference/virtual-judge-tricks.json) · [次序对照](../../../tests/reference/virtual-equip-lose-first.json) · [实战对照](../../../tests/reference/virtual-equip-combat.json)

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
- **恂恂。** 在实际摸牌阶段，候选四张在移动请求开始时仍位于牌堆，移动完成后由技能正文持有私有处理区实体。独立八席测试逐行核对该局 25 条请求中的 21 条阶段、移动与选择请求以及五张实体的位置。注册内容的四个外层技能时机另计。独立牌桌从首页选关羽，在种子 317 实际预览 45、9、92、33，将两张放底后摸到 92、45；1 号 AI 席也提交自己的排列。仅持有者能读取私选牌面，回堆的底牌维持原有知情。[原版分区](../../../tests/reference/ordered-move.json) · [牌桌入口](../../runbooks/testing.md)
- **有符号体力上限和死亡。** Rust 对照上游 20 局 226 条记录中的全部 220 条请求与实际回调，核对原体力、原上限、实际改变量、来源、各时机的体力及生死状态。另六条记录为技能内容首尾。失去上限至零或以下直接进入死亡事件，取消死亡时原上限变化仍能完成。威重在上限事件的 End 中按存活席手牌数摸一或两张。红色实体杀造成伤害后的恃勇分支有行为对照，酒杀分支随酒的规则另行验收。额外回归检查救援中途阵亡、整型最低值、连续操作和身份遮蔽。[原版体力上限轨迹](../../../tests/reference/max-hp.json)
