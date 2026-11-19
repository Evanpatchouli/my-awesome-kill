# Context retrieval / RAG

本目录是 Agent 的检索入口，不是第二份代码仓库。

## 默认检索流水线

1. **Query rewrite**：把用户目标改写成行为、模块、symbol、错误文本、测试名等可检索线索。
2. **Exact retrieval**：优先路径、symbol/LSP、`rg`、引用关系、测试、Git history。
3. **Document retrieval**：根据本页索引读取直接相关的 spec / architecture / ADR / runbook。
4. **Semantic retrieval（可选）**：若项目已经接入 file search、向量库或检索 MCP，再追加语义召回；没有就跳过。
5. **Rerank**：优先当前版本、直接行为证据、与目标最相关的候选；去掉重复和过时内容。
6. **Context Pack**：只装载解决当前工作单元所需的少量上下文，再交给对应 Agent。

## 模块索引

入口在 apps/game；权威规则及测试在 packages/duel-core，WebAssembly 边界在 packages/duel-web，packages/rules 保留原型测试及展示元数据。

| 领域 | 相关文档 | 检索关键词 |
| --- | --- | --- |
| 经典身份局与牌堆 | [P1 对局契约](../specs/classic-duel.md) | Game::classic、card_uses、HeroMoment、GameView、CounterContext |
| 可注册内容与跨包机制 | [Rust 内容接口](../../packages/duel-core/README.md)、[上游覆盖盘点](../exec-plans/active/noname-coverage.md) | TriggerContext、RecastState、DamageRecord、CompareResult、getKnownCards |
| 玩家自定义内容包 | [内容包 v1](../specs/content-packs.md) | ContentPack、CardDefinition、GeneralDefinition、SeededContent |
| 运行与验证 | [启动](../runbooks/development.md)、[验证](../runbooks/testing.md) | Tauri、pnpm、战绩、种子 |
| 产品方向与平台 | [产品愿景](../specs/product-vision.md) | 单机、自定义卡牌、Android、横屏 |
| 技术选型与动效 | [目标架构](../architecture/target-architecture.md) | Tauri、Vue、SQLite、Tailwind、GSAP、motion |
| 阶段目标 | [路线图](../roadmap/roadmap.md) | MVP、游戏模式 |

## 新鲜度规则

- 检索到的内容必须与当前 branch / commit 对齐；历史 PR、事故记录和向量结果只能作为背景。
- 文档与可验证行为冲突时，以当前行为为准并修正文档。
- 若使用向量索引，索引项应保留至少 `path`、`commit_sha/branch`、symbol/section 和 last-modified 信息，避免召回过期代码。

## 上下文预算

- 普通 feature / bugfix：尽量约 20K–60K token。
- 跨多个子系统：先检索、再按需要扩到约 60K–120K。
- 大窗口是安全上限，不是目标；不要为了“信息完整”加载无关文件。
