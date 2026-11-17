# my-awesome-kill · 随心杀

项目名为 `my-awesome-kill`，中文产品名为「随心杀」。P1 提供「经典三国杀」单机双人对局。你选择张飞或黄月英，与使用另一名武将的电脑对手对决。游戏使用 104 张经典标准牌，胜负在濒死救援结束后结算。

安装 Node.js、pnpm 10 后，在仓库根目录运行：

```sh
pnpm install
pnpm dev
```

打开 http://localhost:1420 ，从“开始游玩”进入“经典三国杀”，选将后开始对局。桌面原生入口为 `pnpm tauri dev`，需配置 Rust 与 Tauri 系统依赖。Android 工程与桌面共用规则和界面。

- [启动与原生工具链](docs/runbooks/development.md)
- [规则、牌表与本期范围](docs/specs/classic-duel.md)
- [验证与性能测量](docs/runbooks/testing.md)
- [模块边界](docs/architecture/target-architecture.md) · [路线图](docs/roadmap/roadmap.md)
