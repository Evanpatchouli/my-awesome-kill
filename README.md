# my-awesome-kill

my-awesome-kill 的目标是打造一款单机、可自定义卡牌的类三国杀游戏，面向桌面和 Android 平台。Android 将复用桌面端技术栈，并强制横屏。

项目目前处于文档规划阶段：仓库尚无应用代码或依赖，以下技术选型均为待实现目标。

- 应用：Tauri 2（Rust 后端）、Vite 8、Vue 3、vue-router 和 Pinia，以 pnpm monorepo 组织。
- 界面与动效：Tailwind 负责布局、外观和基础动效；GSAP 负责与游戏流程绑定的动画序列及复杂动效；@vueuse/motion 负责纯装饰性微交互。
- 本地数据：SQLite。

产品方向见[产品愿景](docs/specs/product-vision.md)，技术方向见[目标架构](docs/architecture/target-architecture.md)，阶段目标见[路线图](docs/roadmap/roadmap.md)。开发、测试和发布流程尚待实现后建立，入口见 [runbooks](docs/runbooks/README.md)。完整文档入口见[文档地图](docs/README.md)。
