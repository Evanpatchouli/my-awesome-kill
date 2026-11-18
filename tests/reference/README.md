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

## 复现

1. 在独立临时目录检出上述上游提交，保留 `apps/core`、`packages/fs`、`packages/jit` 和根目录的 `scripts`；可从本机 `.reference/noname` 用 `git clone --shared --sparse` 复用已有对象。通过 `git rev-parse HEAD` 确认提交。不要在项目运行时依赖该参考检出。
2. 在临时检出中安装上游依赖并执行 `pnpm dev`。上游当前 Vite 配置使用 `127.0.0.1:8081`，文件服务使用 `127.0.0.1:8089`。本次采集使用 Node 24 和 pnpm 10.20.0；如果本机 Windows 链接限制阻止安装，可使用 WSL 的原生文件系统。
3. 在项目外准备 Playwright 和可执行的 Chromium。设置 `REFERENCE_PLAYWRIGHT_PATH` 指向 Playwright 包目录；若使用现有浏览器，设置 `REFERENCE_CHROMIUM_EXECUTABLE_PATH` 指向其可执行文件。默认 URL 为 `http://127.0.0.1:8081`，可通过 `REFERENCE_URL` 覆盖。
4. 在本项目根目录执行：

   ```powershell
   node scripts/reference-trace.mjs --check tests/reference/sha-no-shan.json
   node scripts/reference-trace.mjs --scenario trigger-order-cancel --check tests/reference/trigger-order-cancel.json
   node scripts/reference-trace.mjs --scenario damage-callback-cancel --check tests/reference/damage-callback-cancel.json
   node scripts/reference-trace.mjs --scenario tiesuo-recast --check tests/reference/tiesuo-recast.json
   node scripts/reference-trace.mjs --scenario recast-callbacks --check tests/reference/recast-callbacks.json
   node scripts/reference-trace.mjs --scenario linked-damage --check tests/reference/linked-damage.json
   ```

运行器在隔离浏览器内核对八个共同上游源文件的 LF 归一化 SHA-256。重铸和连环伤害场景还校验 `card/extra.js`，重铸知情状态另核对 `element/card.js`，连环伤害另校验 `character/yijiang/skill.js`。随后执行场景并逐项比较事件和结果。整个检出的版本由第 1 步确认。`--write <path>` 用于重新采集，默认将结果写至标准输出。
