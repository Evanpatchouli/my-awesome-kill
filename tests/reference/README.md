# 上游对照轨迹

`sha-no-shan.json` 记录无名杀上游提交 `c73270948d72949aa80009536b9fd5400767d091` 的实际运行结果。它是一个狭窄的规则对照样例，不能代表完整底座已迁移或已经过全部武将、模式验收。

场景为八人标准身份局：测试驱动技能在游戏开始时移除随机武将技能；在首个出牌阶段清空 A、B 的手牌和装备，将双方体力设为 4；A 获得一张通过上游 `game.createCard2` 创建的黑桃 7 实体杀，B 是攻击范围内、无手牌的 AI。驱动器先检查 `canUse`，再调用上游 `A.useCard`；B 的闪选择、伤害、事件调度和牌区移动均由上游处理。驱动技能和固定随机种子仅用于建立可重复局面。A 的 `useCard` 由驱动器直接调用，本样例不验证 A 的人类选牌流程或非法出牌拦截。

`events` 的列名保存在 `eventColumns`。每行是对上游 `GameEvent.trigger` 的一次调用及当时状态：它记录被请求的时机，不表示一定存在或执行了对应技能。`A` 和 `B` 是被测角色，`other` 是其余角色；牌区 `h`、`o`、`d` 分别表示手牌区、处理区和弃牌堆。记录包含处理区清理前后的内部事件，便于核对顺序。

`trigger-order-cancel.json` 使用同样的实体杀局面，再给 B 一张桃作为费用材料。上游 `finishSkill` 和 `addSkill` 注册诊断技能：伤害 `damageBegin4` 的 firstDo 先执行；A 的两个同优先级 source 技能通过真实 `chooseControl` 选择第二项 Beta，Beta 使低优先级候选失效；B 的 player 技能尽管数值优先级更高，仍在 A 的普通技能之后执行，并在真实 cost 事件中弃置桃；最后 A 的 global lastDo 取消伤害。`filterResults` 记录候选筛选的真实结果，`skillSequence` 和事件的 `parent` 列记录父事件。此轨迹观察到 `damageCancelled` 而没有 `damageEnd` / `damageAfter`，但仍有 `shaDamage`、`useCardAfter` 和处理区清理；B 的体力保持 4，杀的使用次数为 1。事件名 `shaDamage` 本身不证明体力发生变化。

## 复现

1. 在独立临时目录检出上述上游提交，保留 `apps/core`、`packages/fs`、`packages/jit` 和根目录的 `scripts`；可从本机 `.reference/noname` 用 `git clone --shared --sparse` 复用已有对象。通过 `git rev-parse HEAD` 确认提交。不要在项目运行时依赖该参考检出。
2. 在临时检出中安装上游依赖并执行 `pnpm dev`。上游当前 Vite 配置使用 `127.0.0.1:8081`，文件服务使用 `127.0.0.1:8089`。本次采集使用 Node 24 和 pnpm 10.20.0；如果本机 Windows 链接限制阻止安装，可使用 WSL 的原生文件系统。
3. 在项目外准备 Playwright 和可执行的 Chromium。设置 `REFERENCE_PLAYWRIGHT_PATH` 指向 Playwright 包目录；若使用现有浏览器，设置 `REFERENCE_CHROMIUM_EXECUTABLE_PATH` 指向其可执行文件。默认 URL 为 `http://127.0.0.1:8081`，可通过 `REFERENCE_URL` 覆盖。
4. 在本项目根目录执行：

   ```powershell
   node scripts/reference-trace.mjs --check tests/reference/sha-no-shan.json
   node scripts/reference-trace.mjs --scenario trigger-order-cancel --check tests/reference/trigger-order-cancel.json
   ```

运行器会先校验与此场景相关的八个上游源文件的 LF 归一化 SHA-256，再启动隔离浏览器并逐项比较事件和结果。该摘要检查不能替代对整个 Git 检出的提交校验。`--write <path>` 用于明确地重新采集，默认无参数则将结果写至标准输出；不要在未核查上游版本或差异时覆盖现有 golden。
