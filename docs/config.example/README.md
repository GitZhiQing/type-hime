# 用户可编辑配置示例（A7 · config.example 惯例）

本目录的示例文件说明 type-hime 的用户可编辑配置；**复制到对应位置后生效，示例本身不参与运行**。

## 1. `permissions.json` —— bash 工具白名单（R4）

两层叠加（全局 + 作品级），路径：

| 层 | 路径 |
| --- | --- |
| 全局 | `%APPDATA%\type-hime\permissions.json`（Windows；macOS/Linux 为 `~/.config/type-hime/permissions.json`） |
| 作品级 | `<作品目录>/permissions.json` |

字段（都可省略）：

| 字段 | 含义 |
| --- | --- |
| `allow_commands` | 登记的额外只读命令（程序名），如 `rg`、`fd` |
| `deny_patterns` | 额外拦截的子串模式 |
| `git_readonly` | 额外允许的 git 只读子命令，如 `reflog` |
| `pipe_filters` | 额外允许的管道右侧过滤命令，如 `sed` |

**不可豁免**：写产物/改动历史的模式黑名单（重定向、`sed -i`、`rm`/`mv`/`cp`、`git add/commit/push/...` 等）与内建 git 只读白名单之上的 `git branch -d` 类写参数——任何档位与配置都不放行；作品文件修改一律走 propose_edit / propose_write 提议通道。

权限档位（变更前确认 / 完全访问）在会话中决定，不在此文件；细粒度覆盖表在作品 `project.json` 的 `permission` 字段：

```json
"permission": {
  "overrides": [
    { "tool": "bash", "pattern": "git log*", "mode": "full_access" },
    { "tool": "propose_edit", "pattern": "ledger/*", "mode": "full_access" }
  ]
}
```

（`pattern` 对 bash 匹配命令前缀（尾 `*` 通配），对 propose_* 匹配目标路径。）

## 2. `project.json` 的装配偏好（R3）

作品级装配偏好（档位/预算/组件排除）由「上下文面板」写入 `<作品目录>/project.json` 的 `assembly_prefs` 字段，无需手改；示例：

```json
"assembly_prefs": {
  "setting_detail": "full",
  "token_budget": 16000,
  "exclude_components": ["ledger_summary"]
}
```
