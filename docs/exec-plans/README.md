# Execution plans

只有复杂、长周期、需要跨会话持续维护的任务才建立 Exec Plan。

- `active/`：仍在执行的计划。
- `completed/`：值得保留的完成记录。

计划建议包含：Goal、Scope、Constraints、Work units、Dependencies、Validation、Rollback、Progress 和 Decisions。
简单任务只使用 `.agents/current-task.md`，不要为流程制造额外文档。
