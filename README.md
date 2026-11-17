# my-awesome-kill · 随心杀

项目名为 `my-awesome-kill`，中文产品名为「随心杀」。经典模式提供 2–10 人单机身份局，默认八人。玩家从 2008 版 25 名武将中自选，与电脑共用 104 张标准牌。Rust 对局机制的持续迁移及验证范围见[迁移计划](docs/exec-plans/active/p1-engine-migration.md)。

按[开发手册](docs/runbooks/development.md)安装 Node、pnpm 10、Rust WASM target 和匹配版本的 wasm-bindgen 后，在仓库根目录运行：

```sh
pnpm install
pnpm dev
```

打开 http://localhost:1420 ，从“开始游玩”进入“经典三国杀”，选将后开始对局。桌面原生入口为 `pnpm tauri dev`，需配置 Rust 与 Tauri 系统依赖。Android 工程与桌面共用规则和界面。

- [启动与原生工具链](docs/runbooks/development.md)
- [规则、牌表与本期范围](docs/specs/classic-duel.md)
- [验证与性能测量](docs/runbooks/testing.md)
- [模块边界](docs/architecture/target-architecture.md) · [路线图](docs/roadmap/roadmap.md)
