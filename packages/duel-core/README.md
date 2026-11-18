# Rust 对局核心

`Game::classic(seed, human_seat, hero_id, seat_count)` 从标准武将和牌堆创建 2–10 人普通身份局。`classic_data::roster()` 提供选将数据和技能原文。固定种子决定身份、其他武将和牌堆顺序。主公先行，各座初始摸四张，五人以上时主公体力上限加一。

`Game::submit` 校验带版本的指令后推进到下一个选择或终局。`Game::view` 公开牌区、当前阶段和等待席位，向观察席提供自己的身份、手牌和可选项。主公和已阵亡角色的身份公开，完局公开所有身份。`Game::bot_command` 给出一条常规指令，外层可以按呈现节奏提交。`Game::new(GameSetup)` 继续作为直接构造杀与闪结算的测试入口。

`events` 包含开局或最近一次接受指令产生的事件。回放按版本累计批次，事件开始与结束共享 ID，子事件保留父 ID。非法或过期指令维持状态与事件批次。`card_faces` 提供观察视角当前牌区和该批事件中公开实体牌的花色及点数。

### 从 Rust 注册技能时机

`Game::register_trigger` 接受 `TriggerRegistration` 并返回可撤销的句柄。内容指定持有席、技能标识、时机名称、匹配的 player/source/target/global 角色、first/normal/last 组、优先级、显示名、资格谓词和结算回调。`Game::classic_with_triggers` 可以在经典开局之前安装注册项。回调可在结算中注册、撤销技能或通过 `TriggerContext::emit` 派发内容自定义时机。

普通组从回合角色起逐席处理。每轮重验当前组的候选后，处于最高有效优先级的不同具名技能可由持有者通过 `Action::Select` 排序，只有持有席能看到选项。首末组按有效优先级、座次自动结算。新增候选可进入尚未走完的组。内置伤害技及麒麟弓也经过这个顺序，已消耗的候选按同一次时机记录。

回调用 `TriggerStep` 指定结束、选择、下一阶段或取消当前被触发事件。`TriggerContext::discard_hand` 校验整批实体，在先前子事件完成后再次核对所有权，再实际付款。中途已被移走的材料会终止后续付款和技能正文，先前已经结算的费用保留。`damageBegin4` 取消可阻止扣血；`damage` 技能执行时体力已更新，此时取消会保留伤害造成的体力变化。两条路径的杀都在 `useCardAfter` 之后完成实体清理。[可复现的上游局面](../../tests/reference/README.md)

### 临时技能与用牌权限

可信技能通过 `TriggerContext::grant_temporary` 为指定座位添加 `TemporarySkill`。它可以携带可撤销的触发登记和按生效牌种判断的 `card_enabled` 谓词。`SkillExpiry` 将时机与持有者的 player/source/target/global 角色关联。同名临时技能再次授予时更新期限，原有武将技能继续保留。内容可通过 `remove_temporary` 提前移除。

`TemporarySkill::until_turn_end` 在全局 `phaseAfter` 或下次 `phaseBeforeStart` 到期。时机请求先记录当时状态，随即撤销这次授予的登记和修饰，然后选择实际执行的技能。`Game::has_skill` 和 `Game::card_enabled` 供可信内容查询。经典选牌、直接提交和 AI 使用相同的生效牌种约束，重铸、打出和救援分别走对应的规则。在巧说的八人平局和负局中，`phaseAfter` 请求时决斗仍被禁止，第一个技能回调里已可使用。[上游观测](../../tests/reference/compare.json)

### 自然轮转、额外回合和阶段

经典模式的 `PhaseLoop` 保存自然席次，所有普通与额外回合都是它的子事件。`TriggerContext::insert_next_phase` 可在 `phaseBegin` 或 `phaseChange` 修改当前回合仍待执行的阶段。`skip_next_phase` 在指定席位下次进入该阶段时生效，同一席位同一阶段的重复申请只消费一次。普通跳过发出 `Skipped`，时机回调取消正在运行的主体会发出 `Cancelled`。每个实际出牌阶段有独立的使用次数。克己读取整回合的出牌与跳过记录。

`insert_extra_turn` 将回合追加到自然循环的待办队列。当前自然回合的 `phaseAfter` 后会执行全部待办，然后才对原自然席请求 `phaseOver`。额外回合有独立的回合号。排队期间阵亡的额外席仍处理回合首尾，但不进入六个阶段；翻面席则翻正并取消该回合。`queued_turns` 可查看包含运行中事件的队列。`TurnPosition::Front` 保留锁定上游的出队行为。在自然回合运行中把 B 前插到该事件之前，B 会在当前等待结束时从队首移除，没有启动。`turn_info`、`phase_info`、`turn_history` 与 `current_phase_player` 可分别查询帧、来源与活动状态。[自然与额外阶段](../../tests/reference/phase-scheduling.json) · [队列实测](../../tests/reference/phase-queue.json)

`register_indexed_trigger` 可为一次请求登记多次激活。已有登记在请求时确定次数，每次运行前重新检查资格，持有者能在同组具名技能间继续选择。`content-phase-skills` 用原版当先与放权验证这些规则，玩家可以在每个出牌段独立决定是否放权。回合末实际弃置一张手牌后，才选择获得额外回合的角色。

### 已支付用牌的目标

`useCard2` 在实体进入处理区并完成失牌回调之后、逐目标结算之前派发。`Game::use_targets(subject)` 读取该次用牌的主目标，`additional_use_targets` 和 `removable_use_targets` 返回当前合法选项。内容响应这个时机时，可以调用 `TriggerContext::add_use_target` 或 `remove_use_target`。两者只修改本次用牌，保留事件 ID、支付和清理，并在执行时重新核对目标。额外的杀与顺手目标仍须满足牌面、武将和牌区条件，可以越过通常的距离限制。

可选 Cargo feature `content-qiaoshui` 提供巧说的 Rust 内容登记。胜方在下一次使用基本牌或普通锦囊时先消耗临时技能，再选择增加、减少或放弃。南蛮中打出杀会保留机会，防御时使用闪和反制时使用无懈会消耗机会。借刀的主目标是持刀者，新增受杀者由用牌者按相应持刀者的攻击范围选择。锁定版本保留原受杀者数组并追加新选择，按排序后的持刀者位置结算。在八席实测中，最初指定 D 对 E 出杀，追加 B 并为 B 选 C，最终询问依次是 B 对 E 和 D 对 C。每份询问会重验实际受杀者。[借刀、南蛮与铁索的上游实测](../../tests/reference/qiaoshui-targets.json)

### 判定阶段与无懈窗口

乐不思蜀和闪电从判定区进入处理区后才进入 `phaseJudge`。翻开的判定牌具有独立的实体 ID；刚请求 `judge` 时，其归属已指向受判定者，此后经处理区、改判和 `judgeFixing` 决定结果。`card_location` 可供可信内容分别读取实际牌区与所有者。天妒可取得翻牌，未命中或被无懈的闪电可移入下一名合法角色的判定区。原延时牌仍在本阶段持有的处理区时，在 `phaseJudgeAfter` 之后才清理。

每份生效目标的无懈链有单独的父事件。实际持有可用无懈的响应者先完成 `chooseToUse`，随后在反制事件中用牌。杀的防御者使用闪则是在完成 `chooseToUse` 之前结算。无懈与反无懈复用同一链和逐席私有窗口。[判定、天妒与无懈实测](../../tests/reference/judgment-zones.json) · [主动用闪实测](../../tests/reference/qiaoshui-targets.json)

### 多人拼点

`TriggerContext::compare_many` 在同一个根事件里接收发起席、目标、`CompareMode` 和可选的逐对内容回调。各席通过 `choice.compareGroup` 私下提交一张实体手牌，最后才统一处理支付。`Game::group_comparison` 和 `compare_many_result` 是可信内容接口；桌面端以 `groupUpdates` 接收已经公开的牌面和结果。

- `Pairwise` 每次只亮发起者与当前对手的两张牌。该对点数修正后写入结果，逐对内容及其中的选择结算完才公开下一对。发起者沿用同一实体和已修正的点数。
- `Simultaneous` 一次公开全部实体，以当时的有效点数记录数组和唯一最高席。随后的 fixing 回调仍可读取并修改工作点数，已记录的数组保持原值，最终处理显式指定的赢家。并列最高时没有赢家。
- 取消一席的失牌可以令其实体留在手中并参与比较；公共翻牌与该席手牌的知情记录各有独立状态。在支付后、首次亮牌前取消根事件，等待的内容仍可取得已初始化的结果。收尾只处理仍由该根在处理区持有的实体。

`content-multi-compare` 提供鼓舌和胆烈。`TriggerStep::ChooseFor` 可让另一席在逐对回调内作私有选择。主动技能可通过 `active_skill_uses_this_phase` 查询本出牌段的实际使用次数。[五个实测八席场景](../../tests/reference/compare-multiple.json)

### 伤害数值

`TriggerContext::add_damage` 可在同一伤害帧的 `damageBegin1` 到 `damageBegin4` 增加结算值。每个实际回调读取之前技能已经修改的数值。体力、护甲、历史以及连环传导在该帧提交后使用同一数值。`Game::event_kind` 可按活跃父事件 ID 区分杀的直接伤害和带有同一实体牌的传导伤害。

### 主动技能、限定次数与扩展区

内容通过 `Game::register_active_skill` 登记出牌阶段技能。持有席从 `activePlays` 获取费用与目标候选，提交 `activateSkill`。引擎在提交时重验座位、登记和选择。限定技能在 `useSkillBegin` 完成、正文开始前按座位和技能名消耗一次；开始时被取消或发动者已死亡会保留次数。实体移动由技能正文执行。`limitedSpent` 记录已发动的限定技能，跨轮及撤销登记后仍可查询。离线 AI 对每项登记在一个出牌阶段尝试一次。

`TriggerContext::add_to_expansion` 将手牌或装备通过失牌和特殊区移动到扩展区，`gain_expansion` 可将实体交给另一席。`Game::expansions` 按持有席和标签取牌，`SkillRemoval::discard_expansions` 在临时技能移除时清理仍匹配的实体。各席的 `expansions` 投影包含公开标记中的实体和标签。`Game::is_known_by` 查询牌的知情记录；从暗手移入扩展区的牌即使没有旁观知情记录，其正面也可通过公开标记查看。`knownSpecial` 只显示观察者认识的无主特殊区实体。若装备已离手而持有者死于失牌回调，原版实际保留装备在特殊区，并完成扩展与用技的后续时机。[中转时死亡](../../tests/reference/zhongyi-transit-death.json)

`content-zhongyi` 提供团队模式的忠义。正式身份桌按其模式处理关系，可信内容可以通过 `set_team` 指定队伍。在受控双队桌，直接由友方杀造成的伤害每名有效持有者加一，连环传导沿用已确定的数值。材料提前转出后增伤持续至下一轮。`TriggerContext::use_virtual_sha` 可按实体材料为空的真实虚拟杀结算火、雷或普通伤害。[跨轮实测](../../tests/reference/zhongyi-expansion.json) · [转移、取消与死亡边界](../../tests/reference/zhongyi-boundaries.json)

### 单张重铸

普通暗给与拼点暗付都在手牌实际离开持有者时清除旁观席对其剩余手牌的旧知情。失牌前的选择仍看到原状态，取消或被截获且未实际移动的失牌不会清除该席记录。公开转交和可见弃牌保留其余手牌原有的知情。

可信内容可用 `Game::register_recast` 按牌种或实体 ID 登记重铸，实体登记优先。出牌阶段仅向有权的持有者列出可用的 `Action::Recast`。技能在其他时机也可通过 `TriggerContext::recast` 发起同一结算。谓词与移动工厂读取 `RecastState` 中的牌、位置、座位和回合信息。默认规则弃掉材料再摸牌；内容可替换为交给另一席或明获指定实体。经典牌堆仍为 104 张。

原手牌在通过 `recastBefore` 和 `recastBegin` 后才公开。暗摸的替代牌只对本人可见，明获的牌通过 `knownHand` 可见，离开手牌后清除这项知情状态。内层失去、摸牌和获得的取消分别结算，已排队的子事件按顺序运行。电脑会在缺乏更好出牌时尝试一次重铸；即便该次被取消也可结束出牌。[原版请求与实际执行对照](../../tests/reference/recast-callbacks.json)

### 两席拼点

内容可先用 `Game::can_compare` 检查双方资格，在技能回调里调用 `TriggerContext::compare(initiator, target)`，并用 `TriggerStep::Continue` 等待结算。两席依次通过普通 `Select` 各选一张自己的手牌，持有席的 `choice.compare` 标明拼点双方。锁定原实体后，双方的失牌事件分别结算。每次实际暗付会清除旁观者对该席剩余手牌的公开知情。`compareReveal` 同时公开锁定的两张牌，随后依次派发双方的 `showCardsAfter`。单个失牌事件被取消时，那张牌仍参与揭示和计分，其实体保留在当前牌区。单机 AI 从当前持有席的合法选项中提交一张牌。轮到人类席时，对局保留该席的选择和版本，等待输入。

技能在 `compare` 或 `compareFixing` 读取 `Game::comparison(subject)` 并用 `set_compare_number` 修改一方的有效点数。固定结果记录双方实体牌、最终点数、胜负、回合和父事件。在 `chooseToCompareAfter`，`gain_compare_card` 可经普通获得流程从处理区或弃牌堆领取一张锁定的拼点牌，领取后各席都知道其新归属。收尾只弃置本次拼点仍在处理区的实体。根事件在揭示前被取消时，会先派发 `chooseToCompareCancelled`，随后同样清理已进入处理区的实体。调用方可从 `compare_result()` 取得已结算的结果，也可从 `Game::compare_history()` 回查。

八人普通身份局第 2、3、4 回合的回归逐行对照上游 51 条拼点请求，覆盖改点取胜、平局和负局，另外核对双方私选、共同翻牌、纵适拿牌和弃置后的所有权。另四组离线拼点的 78 条请求与回调覆盖暗付已知牌、暗付未知牌、根事件亮牌前取消和第二席失牌取消。[三回合实录](../../tests/reference/compare.json) · [知情与取消实录](../../tests/reference/compare-boundaries.json)

### 体力、护甲与横置

`TriggerContext` 提供横置、翻面、增减护甲、造成伤害和创建实体牌的受检操作。内容创建的实体分配唯一 ID，按正常获得事件进入对应牌区。普通杀保留原有牌种，火和雷属性依实体 ID 登记，只有当前视角已知的牌才带有对应牌面信息。零护甲、正面朝上和未横置是通常的开局状态。

护甲在体力变化时吸收伤害。完全吸收的属性伤害仍可解除横置，并向其他横置角色传导。每个传导目标在轮到它时再次检查状态，复用原伤害数值、来源、性质和材料。麒麟弓限直接杀伤害，裸衣只对原伤害加成一次。翻面角色在轮到它时翻回正面并跳过该回合。可信内容可以用 `damage_history`、`damage_dealt`、`damage_received` 查询本局实际记录。[上游实测](../../tests/reference/linked-damage.json)

### 经典身份局

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
