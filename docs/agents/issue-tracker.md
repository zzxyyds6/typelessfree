# 问题追踪器

本仓库的问题（Issue）存放在 **GitHub Issues** 中。

- 追踪器：GitHub Issues
- 命令行工具：`gh`（GitHub CLI）
- 仓库：`zzxyyds6/typelessfree`

## 创建问题

使用 `gh issue create`，附上有描述性的标题和正文。正文使用 Markdown 格式。

## 查看问题

使用 `gh issue view <编号>` 或 `gh issue list --state <open|closed>`。

## 将 PR 作为请求入口

默认：**关闭**。外部 PR 不纳入分类（triage）队列。如果你希望 PR 在分类时与问题一起处理，可以打开此开关。
