# 目标架构（待实现）

本页记录已选定的技术方向，而非当前可运行的系统。仓库目前尚无应用代码或依赖；具体模块边界、数据流和接口待实现时确定。

## 平台与应用栈

- 目标平台为桌面和 Android。Android 使用 Tauri 2 mobile，复用桌面端技术栈，并强制横屏。
- 使用 Tauri 2 和 Rust 后端，前端采用 Vite 8、Vue 3、vue-router 和 Pinia。
- 使用 pnpm monorepo 组织项目。
- 使用 SQLite 保存本地数据；具体数据模型和访问方式待确定。

## 界面与动效职责

- Tailwind：布局、外观和基础动效。
- GSAP：与游戏流程绑定的动画序列和复杂动效。
- @vueuse/motion：纯装饰性微交互。

实现应为未来自定义卡牌组和规则保留扩展空间；具体扩展接口与机制尚未确定。产品目标见[产品愿景](../specs/product-vision.md)，阶段目标见[路线图](../roadmap/roadmap.md)。开发、测试与发布步骤将在实现后写入 [runbooks](../runbooks/README.md)。
