# 上游对照轨迹

`sha-no-shan.json` 记录无名杀上游提交 `c73270948d72949aa80009536b9fd5400767d091` 的实际运行结果。它是一个狭窄的规则对照样例，不能代表完整底座已迁移或已经过全部武将、模式验收。

场景为八人标准身份局：测试驱动技能在游戏开始时移除随机武将技能；在首个出牌阶段清空 A、B 的手牌和装备，将双方体力设为 4；A 获得一张通过上游 `game.createCard2` 创建的黑桃 7 实体杀，B 是攻击范围内、无手牌的 AI。驱动器先检查 `canUse`，再调用上游 `A.useCard`；B 的闪选择、伤害、事件调度和牌区移动均由上游处理。驱动技能和固定随机种子仅用于建立可重复局面。A 的 `useCard` 由驱动器直接调用，本样例不验证 A 的人类选牌流程或非法出牌拦截。

`events` 的列名保存在 `eventColumns`。每行是对上游 `GameEvent.trigger` 的一次调用及当时状态：它记录被请求的时机，不表示一定存在或执行了对应技能。`A` 和 `B` 是被测角色，`other` 是其余角色；牌区 `h`、`o`、`d` 分别表示手牌区、处理区和弃牌堆。记录包含处理区清理前后的内部事件，便于核对顺序。

`trigger-order-cancel.json` 使用同样的实体杀局面，再给 B 一张桃作为费用材料。上游 `finishSkill` 和 `addSkill` 注册诊断技能：伤害 `damageBegin4` 的 firstDo 先执行；A 的两个同优先级 source 技能通过真实 `chooseControl` 选择第二项 Beta，Beta 使低优先级候选失效；B 的 player 技能尽管数值优先级更高，仍在 A 的普通技能之后执行，并在真实 cost 事件中弃置桃；最后 A 的 global lastDo 取消伤害。`filterResults` 记录候选筛选的真实结果，`skillSequence` 和事件的 `parent` 列记录父事件。此轨迹观察到 `damageCancelled` 而没有 `damageEnd` / `damageAfter`，但仍有 `shaDamage`、`useCardAfter` 和处理区清理；B 的体力保持 4，杀的使用次数为 1。事件名 `shaDamage` 本身不证明体力发生变化。

`damage-callback-cancel.json` 在同样的杀局面注册诊断技能，在 `damage` 技能回调中取消伤害。`events` 记录的 `damage` 请求发生时体力仍为 4；`skillCallbacks` 记录实际执行回调时体力已降至 3，并记录回调和伤害事件的父事件。取消保留已扣体力，随后执行 `damageCancelled`、`shaDamage` 和 `useCardAfter`，不执行 `damageSource`、`damageEnd` 或 `damageAfter`。

`tiesuo-recast.json` 记录直接调用上游 `Player.recast` 的两次结算。A 持有铁索和杀，牌堆顶固定一张桃。上游严格判定允许铁索重铸。默认结算将铁索弃置，A 摸到桃。第二次通过上游公开回调将另一张铁索交给 B，并让 A 获得指定的闪。两次均保持 A 的手牌数、铁索使用次数及双方横置状态。`events` 记录原版请求时机时的实体位置，`A:h` 和 `B:h` 表示所在席的手牌。`recastingLost`、`recastingGained` 的请求发生在相应子事件移动实体之前，`recast`、`recastEnd` 分别可见两次移动的结果。该场景覆盖内容调用与牌区变化，玩家点选重铸的交互仍需单独验证。

`linked-damage.json` 记录同一身份局前三个连续回合。每回合的 A 是当前出牌角色，B 是相邻的 AI，C 是另一名 AI。驱动清空八席手牌和装备、重设体力与护甲并公开身份，将上游法恩授予 B。A 对 B、C 使用梅花 10 实体铁索，再对 B 使用红桃 4 实体火杀。牌堆顶两张固定的桃使法恩的摸牌可以逐张核对。前两次采样完成后，驱动结束当前出牌阶段，原版继续推进回合。

- 第一回合，B 在两次横置后的 `linkAfter` 都发动法恩，B、C 各摸一张桃。火杀令两人分别从 4 降到 3 并解除横置。A 的两次伤害历史分别以 `sha`、`_lianhuan` 为父事件，均引用同一张实体火杀。
- 第二回合，B 的 AI 接受为自己发动法恩，拒绝为 C 发动。B 的 1 点护甲吸收全部火焰伤害。B 的 `changeHp` 请求记录 `num = 0`、护甲 1。`changeHujiaAfter` 请求时护甲已降至 0，随后的 `changeHp` 技能回调也读到护甲 0。B 保持 4 体力、解除横置并传导 1 点火焰伤害，C 降至 3。解除横置仍请求 `linkAfter`，法恩只在横置时给出选择。
- 第三回合，D 在用牌前已横置。C 受到传导伤害并进入 `damageEnd` 时，驱动技能在原版待传播名单中读到 D，调用原版 `D.link(false)`。D 保持 4 体力，受伤历史为零。

`choices` 来自法恩的真实 `chooseBool` 结果。`effects` 通过原版内容执行回调记录铁索的横置变化和法恩实际摸到的牌。`events` 只收集所列相关时机，`request` 是调用 `GameEvent.trigger` 当时的状态，`callback` 是首个观察技能执行时的状态。两者之间的子事件可以改变体力、护甲和横置状态。

`recast-callbacks.json` 在同样的局面注册上游观察技能，把时机请求、创建移牌子事件的函数调用、实际技能回调分别记录到一条时间线。请求 `recastingLost` 时材料仍在 A 手中，技能执行时材料已进入弃牌堆或 B 手牌。普通摸到的桃只对 A 可见，显式 `gain2` 获得的闪在 A 手牌中也为第三席所知。两种中断有独立实测：在 `recast` 时机取消保留已弃的材料，获得函数虽被调用，替代牌仍留在牌堆；在 `loseBefore` 将材料转交给 B 时，外层重铸继续摸牌。可检查 `subjectCards`、各席位置以及 `interrupted` 终态。

前置技能取消和底层移牌取消的观测也在这份轨迹中。取消 `recastingGain` 时已经排队的摸牌先完成；只取消 `drawBefore` 时上层仍完成重铸。只取消 `loseBefore` 时材料留在 A 手中，外层也继续摸到一张。这些场景共记录 184 次时机请求、177 次技能执行和 16 次工厂调用。

`recast-batch.json` 在八席身份局用原版 `Player.recast` 观测批量材料。默认输入为按序排列的明手铁索、暗手铁索及装备区青釭剑。两次工厂各收到一次同一份有序数组，`recastingLosingEvents` 记一件 `loseToDiscardpile`，`recastingGainingEvents` 默认记一件摸三张的 `draw`。同一个公开的 `lose` 把两张手牌和一张装备按输入顺序移入弃牌堆，随后通过一个 `draw` 和一个 `gain` 暗摸三张。公告在共享日志显示三张材料的牌面，此时旁观席读到的手牌知情记录仍保持原状。默认摸牌后旁观席只认识原来的控制牌。改用一个自定义 `gain2` 工厂时，三张替代牌通过一件公开获得事件进入手牌，旁观席可以识别三张。

- 内层 `loseBefore` 暗给 B 一张材料时，它先经过暂存区再进入 B 手牌。底层 `loseAfter.cards` 只含已弃的另外两张，`stockcards`、外层重铸和两次工厂仍引用原三张，因此 A 仍摸三张。B 认识新得牌，C 的手牌知情查询仍不认识它。暗给同时清除了 C 对 A 剩余手牌的旧知情记录。
- 只取消内层的整批 `lose` 时，三张材料留在手牌和装备区，A 仍摸三张。只取消 `drawBefore` 时，三张材料已弃，三张替代牌仍按原顺序留在牌堆顶。两次重铸都进入 `recastingGained`、`recastAfter`，被取消的事件仍可从其父或工厂事件列表找到。
- 名典由真实 `damageEnd` 发起 `chooseCard`，选牌范围为手牌及装备区，至少一张，夹具只设置 AI 偏好。两个基本牌获选，非基本控制牌保留。选完时材料尚在手中，C 的知情记录未变。两个基本牌均通过宽松重铸检查，经名典进入同一重铸事件，一次弃两张、摸两张。

`recast-batch-boundaries.json` 另观测两次完整批量重铸。每次提交明手铁索、暗手铁索和装备区青釭剑。

- 在 `loseToDiscardpileBegin` 取消失牌包装时，包装请求 `loseToDiscardpileCancelled`，其状态从请求时的未完成变为首个实际回调时的已完成。包装保留在 `recastingLosingEvents`，没有创建内层 `lose`，没有失牌记录。三张材料和旁观者手牌知情维持原状。外层仍执行 `recastingLost`、一次摸三张和 `recastAfter`。
- 在内层 `loseBefore` 把三张材料暗给 B 时，实体先经暂存区，再一起进入 B 手牌。原内层正常完成，`loseEnd` 和 `loseAfter` 的请求、回调及失牌历史均含空 `cards` 与完整 `stockcards`。父包装照常执行其主时机和结束时机，两次工厂都使用原三张有序数组，A 仍暗摸三张。C 只认识 B 新得的原装备牌，B 认识三张，C 对 A 原公开控制牌的手牌知情被清除。

`batch-give-death.json` 直接调用原版批量 `A.give`，将暗手铁索和已公开的装备青釭剑交给非主公 B。两张实体进入无主 `ui.special` 后，夹具在失牌 `loseAfter` 回调中让 B 死亡；独立旁观席 C 仍存活。B 死亡前后两牌都各只存在一处，原暗牌仍未知、原装备仍已知。外层 `gain` 未取消，保留原两张牌并继续执行 `gainEnd` 和 `gainAfter`；两牌最终仍在无主暂存区。

`compare.json` 记录八人离线身份局中的巧说和纵适。实际浏览器席 C 旁观三个 AI 出牌回合。每次 A、B 各暗摸两张实体牌。夹具给真实 `chooseTarget`、`chooseCard` 的 AI 指定偏好，巧说的目标过滤、选牌、移牌、比点和技能内容均由原版执行。巧说的 `qiaoshui_cost` 选中 B 时双方拼点牌仍在手中。三例的 `fixedResultSupplied` 均为 `false`。

原版 `chooseToCompare` 通过 `chooseCardOL` 依次让 A、B 执行 `chooseCard`。`privateSelections` 在每次选择开始与结束时读取当前选择结果、A/B/C 的已知手牌及 C 牌桌上的公开拼点牌面。四个时刻双方均只知自己的两张手牌，C 的两组已知牌和公开牌面均为空。双方结果汇总后，两张牌依次由手牌移到处理区。原版以一次 `$compare` 将两张牌交给展示层，再分别为 A、B 调用 `showCards`。`reveals` 记录那一次双牌展示的输入，`spectatorSawFlip` 记录 C 的牌桌上实际翻为正面的两张牌。

- **改点后获胜。** A 出红桃 7 桃，B 出方片 9 闪。测试席额外持有「天辩」的原版点数子技能 `tianbian_number`，其 `compare` 回调将 7 改为 13，`compareFixing` 时点数为 13 对 9。A 发动纵适，从处理区获得 B 的闪，自己的桃随后弃置。A 先以 B 为下一张杀的唯一目标，D 在普通攻击范围外。巧说在 `useCard2` 消耗临时技能，再让 A 选择并加入 D。B、D 各失去 1 点体力，A 接着使用的决斗只有 B 一个目标。
- **平点与失利。** 7 对 7 的结果为 `bool: false`、`tie: true`，无赢家。4 对 10 的赢家为 B。两次 A 都通过纵适取回自己的桃，B 的闪进入弃牌堆。A 在该回合仍能对 B 使用杀，手中的决斗受巧说限制。`phaseEnd` 及 `phaseAfter` 请求时 A 仍有 `qiaoshui2`，实际执行 `phaseAfter` 观察技能时该技能已经移除，决斗重新可用。

`qiaoshui-targets.json` 在八席身份局的同一个出牌阶段，通过 `addTempSkill` 直接授予巧说的胜利子技能，再使用实体牌。A 是 AI，H 是浏览器席。夹具设置 AI 选择偏好，并用 `Player.useCard` 提交初始目标。过滤、选择结果、时机和卡牌结算都由原版执行。

- **借刀的额外持刀者与配对错位。** A 初始指定 D 持刀、E 受杀。`useCard2` 中巧说先消耗，再由 A 可选地新增持刀者 B。`useCard` 开始时持刀者是 D、B，集智在此时摸牌。原版接着把持刀者按座次排成 B、D，发出目标时机，之后才在 `jiedaoContentBefore` 要求 A 为 B 强制选一名受杀者。候选依据 B 的杀范围和目标合法性计算，A 选 C。原版保留已有的 `[E]`，又追加 C、E，得到 `[E,C,E]`。两份效果按下标实际发出 B→E、D→C。B 用出实体杀且保留青龙偃月刀，杀的使用次数仍为零。E 使用实体闪，体力保持 4。D 拒绝使用杀，A 获得 D 的诸葛连弩。借刀的实体牌只从 A 手牌支付一次。
- **自动多目标与取消。** 南蛮入侵的初始目标为 B 至 H，巧说只给出可取消的减目标选择。A 移除 H，H 未受影响。铁索连环的初始目标为 B、C，巧说先给出增加、减少、取消三个选项。A 取消，仍消耗巧说且保留初始目标。D 对 B 的铁索效果使用无懈可击，只有 C 横置。
- **响应与临时技能期限。** E 的闪和 D 的无懈可击都进入 `useCard2`，两人各自的 `qiaoshui3` 在没有目标选择的情况下消耗。D 在南蛮入侵中打出杀走 `respond`，支付实体牌后保留 `qiaoshui3`。最后为 A 授予 `qiaoshui2`，在原版 `phaseAfter` 请求时决斗仍受限，首个 `firstDo` 观察技能执行时限制已经解除。

`zhongyi-expansion.json` 记录忠义跨两个自然轮次的 42 条时机观测。夹具在身份牌桌设置两支布尔队伍，直接调用原版登记的忠义、`useSkill` 和 `isFriendOf`。一张实体桃从手牌移入扩展区并获得 `zhongyi2` 标签，限定技成为已使用。友方的杀从一伤变二伤，敌方的杀和友方的其他伤害都是一伤。下一轮请求 `roundStart` 时牌仍在扩展区；处理该时机的技能执行时，临时效果已移除，材料已弃置一次，限定技的使用记录保留。

`zhongyi-boundaries.json` 记录同一身份局的四组忠义边界。夹具设置布尔队伍，主动选择使用带 AI 偏好的原版 `chooseToUse` 出牌选择，其余样本用 `useSkill` 直接提交材料。`inSpecial` 指材料位于 `ui.special` 中转区，`knownToSpectator` 读取实体牌的 `isKnownBy`，`markShowsMaterial` 检查原版标记弹层中是否有该实体的正面按钮。

- **装备与转移。** A 主动选中黑桃 6 的已装备青釭剑。即使仍有手牌，下一次出牌选择也没有忠义选项。材料经 `A:e` → `s` → `A:x` 移动，旁观者的知情记录全程保留，标记弹层显示实体牌。转给 B 后材料保留在 `B:h`，A 的标签和标记移除，增伤仍在。没有实体材料的虚拟火杀直接打 2 点，连环伤害沿用 2 点。自然 `roundStart` 请求时增伤仍在，首个观察回调时已到期，材料仍在 B 手牌，随后杀只打 1 点。
- **转移取消。** 取消 `addToExpansionBefore` 或内层 `loseBefore` 都让材料留手，且没有扩展标签或标记。两次都保留限定消耗、技能禁用和增伤，实际杀均造成 2 点。外层取消保持原暗手牌的知情记录。内层取消发生在公开给牌已登记知情之后，旁观者此时认识留在手中的材料。
- **死亡与叠加。** A 的回合中 E 的暗手桃经 `s` 进入扩展区。原版在 `s` 和 `x` 读取的 `isKnownBy` 为假，忠义标记弹层仍显示实体牌面。两名持有者令 F 的杀叠到 3 点。E 的 `die` 回调仍可见材料、标记与增伤，`dieAfter` 时材料已实际进入弃牌堆一次，标记和增伤均已清除，限定消耗保留。另一持有者仍使 F 后续的杀打 2 点。

`zhongyi-transit-death.json` 在忠义材料已从装备区移入 `ui.special` 时，于原版 `loseAfter` 实际回调内让非本回合的持有者死亡。持有者是非主公，游戏继续，剩七席。`die` 回调仍能看到增伤，`dieAfter` 时增伤已移除，限定消耗和禁用保留。原版外层标记为完成且未显式取消，并请求 `addToExpansionEnd`、`addToExpansionAfter`，随后执行后者的观察回调。外层仍引用材料。从死亡到外层结束，实体青釭剑留在无主的 `ui.special`，没有进入扩展区或弃牌堆，也没有标签或标记。旁观者仍认识这张原本公开的装备。

`compare-boundaries.json` 用四局两席拼点观测已知手牌和取消边界。第三席原先认识 A 的一张桃；A 暗付这张桃或另一张牌时，其余手牌的旧知情记录都在 A 的 `loseAfter` 清除。双方已支付后在 `compareCardShowBefore` 取消，根事件没有比较结果，两张实体在收尾各弃置一次。仅取消 B 的 `loseBefore` 时，外层仍按原先锁定的两张牌结算 10 比 5，A 的材料弃置，B 的仍在手中。

`compare-multiple.json` 在八席离线身份局中用原版鼓舌和胆烈完成五次多人拼点。浏览器席 C 旁观。夹具为 AI 提供实体手牌、选牌偏好和三名合法目标，按 E、B、D 直接调用 `useSkill`。原版重新按座次排序，`chooseCardOL` 依次让 A、B、D、E 暗选。每次选择开始和结束时，四张拼点材料仍各在手中，选择席只认识自己的两张牌，C 的手牌知情和公开牌面均为空。原版收齐选择后只发起一次 `loseAsync`，四个 `lose` 均以它为父。

- 鼓舌逐对展示 A–B、A–D、A–E，C 的桌面实际翻明三对牌，A 三次复用同一实体。B 局的 `compareFixing` 把 A 从 7 改到 10，原版回调以 10:8 判 A 胜。10 继续用于 D 的 10:10 与 E 的 10:12，逐对回调的赢家依次为 A、空、E。根结果保存双方的点数数组和实体，没有全局赢家字段。
- 胆烈一次共同展示并实际翻明四张。发起者的 `compare` 增加 A 的点数至 11，随后 B、D、E 的各次 `compare` 仅设目标席。最后统一进行修正，首次 `compareFixing` 恢复发起者字段，同时仍引用上一比较的 E 和其牌。探针把 A 的临时点数改成 1、D 的改成 13，但存档仍为 11、11、11 对 9、10、8，赢家仍为 A，实际伤害 B、D、E 各一点。最高点并列时根结果的 `winner` 为空，胆烈令 A 失去一点体力。
- 取消 D 的内层 `loseBefore` 时，已锁定的实体仍留在 D 手牌，实际翻明并参与结算。C 的手牌知情查询仍不认识这张牌，收尾只弃置其他三张。支付后在 `compareCardShowBefore` 取消共同拼点时，根仍保留初始化的实体引用、空点数数组和空赢家。它请求 `chooseToCompareCancelled`，四张已支付的实体均弃置，胆烈仍令 A 失去一点体力。

`phase-scheduling.json` 在八席身份局运行原版当先和放权。A 至 H 从自然循环起始席编号，`A1`、`A2`、`A3` 依次表示 A 开始处理的三次回合事件。夹具移除随机武将技能，设置体力、牌区、牌堆顶和 AI 偏好。浏览器席也开启托管，选牌、支付、判定、技能效果及调度均由原版执行。

- A 的自然回合与额外回合均在 `phaseBegin` 实际执行当先，把带来源的 `phaseUse` 插到准备阶段之前。两个回合的阶段索引 0 和 4 各使用一次实体杀。每段首次原版选牌允许杀，第二次仍持有其他杀，但原版过滤拒绝。普通段在 `phaseUseBefore` 请求前已将杀的使用统计重置为零，同回合历史仍计入前一段。四张杀均由 A 手牌经处理区进入弃牌堆，H 从 9 体力降到 5。
- B 和 E 接受放权后，原版只请求其出牌阶段的 `phaseUseBefore`、`phaseUseCancelled`。取消请求时阶段尚未结束，放权标记尚未添加。首个取消回调时两项状态均已更新。结束时各自通过真实选择先弃一张指定手牌，再强制选择其他席。C 拒绝首次选择，继续出牌阶段。D 接受跳过但拒绝付牌，手牌保留且没有选人，待授予标记在 `phaseAfter` 请求后、首个回调前移除。
- 放权创建的回合以 `phaseLoop` 为父，与自然回合同层，来源为 `fangquan2`。授予者完成 `phaseAfter` 后处理额外回合，随后 `phaseOver` 仍属于授予者所在的自然席，下一自然席分别为 C 和 F。A2 增加全局回合号，轮号仍为 1。G1 遇到翻面被取消，全局回合号增加，但没有设定当前席或增加 G 的个人回合号。请求 `phaseCancelled` 时 G 仍背面朝上，首个实际回调已正面朝上，随后解除放权授予标记。G2 正常进入自然回合。
- C 的摸牌阶段由 `Player.skip` 跳过。H 的乐不思蜀以黑桃 8 判定失败，原版效果登记跳过出牌。两者都只有对应的 `Skipped` 阶段时机。A1 正常从牌堆摸两张指定实体牌，并在弃牌阶段经强制选牌弃去其中一张。H1 结束后请求第 1 轮的 `roundEnd`，A3 的 `roundStart` 进入第 2 轮，采集到该回合第一次 `phaseBegin` 回调为止。

时机请求、首个观察技能回调与实际技能或乐不思蜀内容分别记为 `request`、`callback`、`content`。`stage` 和 `state` 使用各自的列名表，`state.flags` 使用 `flagColumns`。当前席为空表示原版尚未赋值或已经清除该状态。牌区前缀为持有席，`c`、`o`、`d`、`s` 分别为牌堆、处理区、弃牌堆、暂存区。

`judgment-zones.json` 在八席身份局独立复跑六种延时判定，区分放入判定区的实体牌和从牌堆翻开的判定牌。乐不思蜀失败时只登记跳过出牌，天妒可获得成功的判定牌；闪电命中造成无来源三点雷伤，未命中或被无懈抵消则转入下家判定区。无懈抵消乐时不开始判定，预置判定牌留在牌堆。两张牌独立移动：延时牌先经以 `phaseJudge` 为父的失牌进入处理区，判定事件的父也是 `phaseJudge`；通常判定牌在效果前弃置，延时牌在阶段 `After` 后弃置或在效果中迁走。`judge` 请求瞬间上游 `get.position` 尚无结果，但 `get.owner` 已指向判定者；到 `judgeFixing` 才观察到判定牌位于处理区。`revealedOwner` 和 `revealed` 分别保留这两项观测。

`counter-payment.json` 记录同一八席身份局三次实体无懈可击。每次 B 的闪电原会翻到黑桃 8，A 通过原版离线选择接受无懈，随后选择均按夹具的 AI 偏好拒绝。三种干预都保留 `useCard1`、`useCard2`、`useCardAfter` 和无懈内容执行。B 保持 8 点体力，判定牌留在牌堆，实体闪电移到下家。

- **选牌后转移。** `_wuxie` 的 `chooseToUseAfter` 回调将 A 已选的实体交给 C。随后原版仍为 A 创建包含同一实体的 `useCard`。支付失牌的角色为 C，牌从 C 的手牌移入处理区，最后弃置一次。
- **取消支付。** `useCard` 创建后，在支付的 `loseBefore` 回调取消失牌。原版请求 `loseCancelled`，实体留在 A 手牌，无懈用牌事件仍引用该牌。
- **支付时转移。** `useCard` 创建后，在支付的 `loseBefore` 回调将实体交给 C。原支付仍发出 `loseAfter`，其材料列表已经不含该实体。无懈继续引用同一张牌，实体留在 C 手牌。

`phase-queue.json` 在另一局八席身份牌桌用观察技能直接调用上游 `Player.insertPhase`，并用原版 `die` 让排队的非主公 E 死亡。`operation` 给出每次调用使用的前插参数与队列前后状态。第一轮的 A 自然回合结束时追加 C、D、E，再把 B 前插到尚未出队的 A 之前。

- A 的 `phaseAfter` 请求仍显示 B 在队首。随后的第一个额外回合是 C，B 的额外事件已不在队列、未开始也未完成。B 随后正常进入自然回合。这个样本覆盖已有自然回合占队首时调用 `insertPhase(source, true)` 的行为。
- C 在自己的额外回合中再追加 F，F 排在已经等待的 D、E 后。C、D、E、F 全部处理完后才请求 A 的 `phaseOver` 并推进自然席。
- E 死亡后队列仍持有其额外回合。该事件带 `forceDie` 和 `includeOut`，更新全局、个人回合号和当前席，处理回合首尾时机。E 的六个阶段均未开始，也没有请求 `phaseCancelled`。自然循环随后走到死席 E 时只请求 `phaseOver`，继续到 F 自然回合。

`ordered-move.json` 在八席离线身份局的自然回合中分别授予真实观星和恂恂，并固定五张可区分的牌堆顶实体 `c0` 至 `c4`。测试席旁观；夹具设定 AI 的技能发动偏好及 `processAI` 排列，选择结果、技能正文、调度和实体移动由原版执行。记录的时机是请求，不代表每个时机均有技能回调。

- 观星准备阶段展示五张：子 `chooseToMove` 在 `After` 返回上 `[c2,c0]`、下 `[c4,c1,c3]`；父 `chooseToGuanxing` 原地反转顶行后返回 `[c0,c2]`，实际牌堆顶仍是 `[c2,c0]`，牌堆底为 `[c4,c1,c3]`。该席随后自然摸到 `c2,c0`。选择开始和父事件结束时，旁观席的 `isKnownBy` 与同实体可见卡牌检查均为 false；父事件结束后观星席成为五张牌的知情者。
- 恂恂在真实 `phaseDrawBegin1` 从四张牌中选出上 `[c2,c0]`、下 `[c3,c1]`，归堆后自然摸到前两张。夹具调用原版 `filterMove` 和 `filterOk` 探测约束，前两张可移入下行，第三张被拒绝。下行恰两张时可以确认，布局由离线 AI 提交。
- 另一次观星在子 `chooseToMoveBegin` 由诊断技能显式调用 `cancel()`。子事件以 `Cancelled` 结束，父 `chooseToGuanxing` 返回 `bool: true` 和两个空行。其 `After` 时五张仍在处理区，之后 `orderingDiscard` 把五张弃置。独立的非强制选择使用 AI `processAI` 返回空值，由原版产生 `bool: false`，正常请求 `End`、`After`，两张牌留在手中。

人类交互、在线同步、任意扩展过滤器和 JavaScript 环境隔离需要另行验收。

## 复现

`hp-cancel.json` 在八席身份局用红桃 4 火杀对两名横置角色验证体力子事件。B 有 1 点护甲，伤害为 2。取消 `changeHpBefore` 保留 B 的 4 点体力、护甲和横置状态，伤害历史仍记录 A 对 B 的 2 点伤害。取消 `changeHpEnd` 时 B 已降到 3 点体力、消耗护甲并解除横置，之后 C 受到 2 点传导伤害。两次取消都触发 `changeHpCancelled`，父伤害的 `damage`、`damageSource`、`damageEnd`、`damageAfter` 继续执行。76 条记录同时包含时机请求、实际回调、实体伤害历史是否已存在及当时的护甲吸收值。

`max-hp.json` 记录同一固定版本的八席身份局 20 次样例，含 110 次时机请求、110 次实际观察回调和 6 条真实技能内容首尾。夹具移除随机武将技能、固定初始体力和手牌，直接调用原版 `gainMaxHp` / `loseMaxHp`，并按样例设置事件的 `source`。`events` 记录事件及父事件、角色、数值、体力/上限/存活/手牌快照、原值、变化值、取消与死亡原因；请求和回调是不同观察点。

- **基本变化和参数。** 4/4 获得一点上限后是 4/5；4/4 失去一点后直接变 3/3，2/4 失去一点后为 2/3，这两次变化没有 `changeHp` 子事件。零值仍经过 Before、Begin、End、After。3/4 的 `gainMaxHp(-2)` 得到 2/2，但该事件的 `changedHp` 是 0；`loseMaxHp(-2)` 得到 3/6。4/4 的 `gainMaxHp(-5)` 得到 -1/-1，角色仍存活，没有濒死或死亡。角色原始体力为 -1 时，两种事件的 `originalHp` 都是 0。
- **取消与直接死亡。** 取消获得的 Before 或失去的 Begin 保留原值；取消 End 保留已应用的变化；均发各自的 Cancelled 而不发 After。1/1 失去 1 上限时，先变为 0/0，再直接进行 `die` 子事件，不请求濒死或体力变化；该样例死亡无来源。1/1 失去 2 时，先变为 -1/-1，再带来源 A 直接死亡；死亡内部请求 `changeHp(1)`，但再次受上限夹断而维持 -1/-1。两次死亡均在父事件 End/After 之前完成。取消子 `dieBefore` 时角色仍以 0/0 存活，父事件继续 End 和 After。
- **真实跨包技能。** 独立授予威重后，八席均无手牌的并列最少触发一次摸两张；随后持有者两张、其他席均无手牌时触发一次摸一张。摸牌发生在上限事件 End 的实际技能回调内、After 之前。独立授予恃勇后，真实红桃杀的伤害令 B 由 4/4 变成 3/4，`damageEnd` 内的恃勇调用 `loseMaxHp` 后变为 3/3，然后才请求 `damageAfter`。本样例不涵盖恃勇的酒杀条件。

`revival.json` 在八席身份局用原版 `die` 和 `loseMaxHp` 令同一非主公 AI 死亡，再分别调用普通 `revive` 与事件版 `reviveEvent`。B 初始有一张手牌、一张装备、两个标记，处于横置和背面状态。死亡公开身份、弃置两张实体牌、解除横置和翻面，并移除标记显示；该样例的标记存储数仍为 2。`revive(2, false)` 使 B 以 2/4 存活，角色、身份、空牌区和标记状态保持死亡后的结果。B 从死亡名单移回存活名单末尾，但存活角色的座次环回到原来的位置。普通复活是同步调用，不创建体力或复活事件。

`character` 的三个布尔值分别表示 `name`、`name1`、`name2` 是否与该次对局初始值相同，避免将跨进程随机选取的武将名写入金样。夹具在开始时将按座次排列的存活数组循环移位一次，使其以当前席 A 开头；如有需要，将操控移到 A，并交换隐藏的非主公身份，将 B 安排为忠臣、D 安排为反贼，保留身份配比与主公。此后分别记录上游实际数组顺序与存活座次环，不在观察时排序。`events` 区分时机请求与观察技能回调，死亡和体力子事件均记录父事件。

对已存活的 B 再次调用普通复活，传入 9、0、-2 依次得到 4/4、1/4、-2/4，负体力时仍存活且没有濒死事件。1/1 失去 2 点上限后直接死亡，在死亡子事件中请求 `changeHp(1)`，但受负上限夹断而仍为 -1/-1；普通 `revive(5)` 将上限抬至 1，最终为 1/1。1/1 失去 1 点上限时以 0/0 死亡，没有体力子事件；取消 `reviveEvent(2)` 的 Before 后角色仍死亡，随后的 `reviveEvent(0)` 以默认的 1 点体力复活，依次请求 Before、Begin、End、After。此轨迹没有覆盖休整计数或自然跨轮返回。

补充样例将真实鼓舌授予 B 并直接设置七个“饶舌”标记，再调用原版死亡与复活；没有模拟鼓舌拼点。该技能的 `intro` 无 `onunmark`：死亡前、死亡后、复活后 `countMark('gushe')` 均为 7，标记显示依次为有、无、无，技能仍在。

同局另一名非主公反贼由夹具调用带 A 为来源的原版 `die`，观察技能在 `die` 主时机回调中立即调用 `revive(2, false)`。请求 `die` 时该角色已进入死亡名单、身份已公开，但手牌、装备和七个鼓舌标记仍在；回调复活后恢复 2/4 和活席座次，死亡正文后续的死者清理不再弃牌或移除标记及临时技能。击杀数已加一，A 仍在 `dieEnd` 前获得身份局的三张反贼击杀奖励；死亡身份保持公开，对局未结束。

1. 在独立临时目录检出上述上游提交，保留 `apps/core`、`packages/fs`、`packages/jit` 和根目录的 `scripts`；可从本机 `.reference/noname` 用 `git clone --shared --sparse` 复用已有对象。通过 `git rev-parse HEAD` 确认提交。不要在项目运行时依赖该参考检出。
2. 在临时检出中安装上游依赖并执行 `pnpm dev`。上游当前 Vite 配置使用 `127.0.0.1:8081`，文件服务使用 `127.0.0.1:8089`。本次采集使用 Node 24 和 pnpm 10.20.0；如果本机 Windows 链接限制阻止安装，可使用 WSL 的原生文件系统。
3. 在项目外准备 Playwright 和可执行的 Chromium。设置 `REFERENCE_PLAYWRIGHT_PATH` 指向 Playwright 包目录；若使用现有浏览器，设置 `REFERENCE_CHROMIUM_EXECUTABLE_PATH` 指向其可执行文件。默认 URL 为 `http://127.0.0.1:8081`，可通过 `REFERENCE_URL` 覆盖。
4. 在本项目根目录执行：

   ```powershell
   node scripts/reference-trace.mjs --check tests/reference/sha-no-shan.json
   node scripts/reference-trace.mjs --scenario trigger-order-cancel --check tests/reference/trigger-order-cancel.json
   node scripts/reference-trace.mjs --scenario damage-callback-cancel --check tests/reference/damage-callback-cancel.json
   node scripts/reference-trace.mjs --scenario hp-cancel --check tests/reference/hp-cancel.json
   node scripts/reference-trace.mjs --scenario max-hp --check tests/reference/max-hp.json
   node scripts/reference-trace.mjs --scenario revival --check tests/reference/revival.json
   node scripts/reference-trace.mjs --scenario tiesuo-recast --check tests/reference/tiesuo-recast.json
   node scripts/reference-trace.mjs --scenario recast-callbacks --check tests/reference/recast-callbacks.json
   node scripts/reference-trace.mjs --scenario recast-batch --check tests/reference/recast-batch.json
   node scripts/reference-trace.mjs --scenario recast-batch-boundaries --check tests/reference/recast-batch-boundaries.json
   node scripts/reference-trace.mjs --scenario batch-give-death --check tests/reference/batch-give-death.json
   node scripts/reference-trace.mjs --scenario linked-damage --check tests/reference/linked-damage.json
   node scripts/reference-trace.mjs --scenario compare --check tests/reference/compare.json
   node scripts/reference-trace.mjs --scenario qiaoshui-targets --check tests/reference/qiaoshui-targets.json
   node scripts/reference-trace.mjs --scenario compare-boundaries --check tests/reference/compare-boundaries.json
   node scripts/reference-trace.mjs --scenario compare-multiple --check tests/reference/compare-multiple.json
   node scripts/reference-trace.mjs --scenario phase-scheduling --check tests/reference/phase-scheduling.json
   node scripts/reference-trace.mjs --scenario phase-queue --check tests/reference/phase-queue.json
   node scripts/reference-trace.mjs --scenario judgment-zones --check tests/reference/judgment-zones.json
   node scripts/reference-trace.mjs --scenario counter-payment --check tests/reference/counter-payment.json
   node scripts/reference-trace.mjs --scenario ordered-move --check tests/reference/ordered-move.json
   node scripts/reference-trace.mjs --scenario zhongyi-expansion --check tests/reference/zhongyi-expansion.json
   node scripts/reference-trace.mjs --scenario zhongyi-boundaries --check tests/reference/zhongyi-boundaries.json
   node scripts/reference-trace.mjs --scenario zhongyi-transit-death --check tests/reference/zhongyi-transit-death.json
   ```

运行器在隔离浏览器内核对八个共同上游源文件的 LF 归一化 SHA-256。重铸、批量给牌死亡、连环伤害和巧说目标场景还校验 `card/extra.js`，重铸知情状态、批量给牌死亡、拼点和额外忠义场景另核对 `element/card.js`，连环伤害、巧说拼点和巧说目标另校验 `character/yijiang/skill.js`，忠义和巧说目标另校验 `character/standard/skill.js`。体力上限场景核对 `character/yijiang/skill.js` 和 `character/sp/skill.js`，复活场景核对鼓舌所在的 `character/sp/skill.js`。有序移动场景另核对标准与界限突破技能包、牌实体及牌区查询。多人拼点另外校验鼓舌和胆烈所在的两个技能包、牌区查询和事件步骤编译器。忠义边界同时核对牌区和知情查询、出牌选择及标记弹层涉及的源文件。忠义中转死亡和批量给牌死亡核对牌区查询和负责中断事件步骤的编译器源文件。阶段调度另外校验当先和放权所在的两个技能包、牌区查询和事件步骤编译器。随后执行场景并逐项比较事件和结果。整个检出的版本由第 1 步确认。`--write <path>` 用于重新采集，默认将结果写至标准输出。
