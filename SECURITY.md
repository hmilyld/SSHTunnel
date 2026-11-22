# Security Policy

## Supported Versions

| Version | Supported |
| --- | --- |
| 1.x | ✅ |
| < 1.0 | ❌ |

## Reporting a Vulnerability

请使用 GitHub 的**私密漏洞报告**，不要开公开 Issue：

<https://github.com/hmilyld/SSHTunnel/security/advisories/new>

报告中请尽量包含：

- 受影响版本（`vX.Y.Z`）与操作系统；
- 复现步骤、预期结果与实际结果；
- 最小化的日志片段（请先移除主机名、用户名、密码、私钥、token 等敏感信息）。

**不要**在 Issue、PR、截图或日志中提交密码、私钥、访问令牌或完整的
`tunnels.json` / `~/.ssh/config` 内容。

我们会尽力在 7 天内确认收到报告，并在评估后给出修复或缓解方案；这是
best-effort 维护，不承诺商业 SLA。

## Scope

重点关注：

- Tauri 命令与前端 IPC 边界；
- `ssh` 参数构造、Host/Bind/Remote 校验、密码 / `SSH_ASKPASS` 流程；
- `tunnels.json`、`app.log` 等本地数据文件的读写与权限；
- CI/CD、发布资产、依赖供应链与许可证问题。

## Safe Harbor

出于善意、且未造成数据破坏或服务中断的安全研究，我们不会追究责任。
请在公开披露前给我们合理的修复时间。
