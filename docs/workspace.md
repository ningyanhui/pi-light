# 工作区与 cwd（开发者说明）

产品主路径把「换工作区」写成无感知同步：顶栏徽章、文件树和 Git 跟随**当前活动工作区**。同时只有一个活动 cwd，不要验收或宣传同会话真多 cwd。

## 上游限制

Pi v1.4.0 会话 cwd 在 spawn 时由 `Command::current_dir` 钉死。官方 RPC **没有** `set_cwd`。壳不能假装已经原地改目录。

## 壳侧策略

| 情况 | 行为 |
|------|------|
| 切会话但路径相同 | 复用现有 sidecar，只发 `switch_session` / `new_session` |
| 徽章换目录，或切到不同 workdir 的会话 | 静默 kill + spawn 新 cwd，再按活动会话 `switch_session` / `new_session` |
| 校验失败或 spawn 失败 | 徽章与树不停留在假路径；文案为「切换工作区失败」，可重试；并尝试拉回原 cwd 的 sidecar |

用户主路径、README、设置脚注 **不要**写「重启引擎 / 应用并重启 Pi」。进程级切换只写在本页。

每会话 `workdir` 落在 `~/.pi-light/sessions.json`。全局 `workdir` 字段仍是最近一次活动路径，供旧文件兼容。启动时缺字段的会话继承该回退值。

文件树默认忽略的目录见 `workspace_tree::DEFAULT_IGNORED_DIRS`。
