# 文档

本目录存放 SSH 隧道管理器的用户与维护者文档。README 仅保留项目简介、快速开始和文档索引。

| 文档 | 内容 |
| --- | --- |
| [architecture.md](architecture.md) | 技术栈、目录与模块边界、数据流、生命周期、Tauri 命令 |
| [configuration.md](configuration.md) | 数据文件、`tunnels.json` 格式、`~/.ssh/config` 与认证配置 |
| [usage.md](usage.md) | 新建、查看、关闭、重启、修改、删除、托盘与主题 |
| [development.md](development.md) | 环境要求、开发/测试/格式化/审计命令、工具链 |
| [release.md](release.md) | 版本号、tag 发布、CI 质量门禁、产物矩阵与资产命名 |
| [security.md](security.md) | 凭据处理、参数校验、进程校验、CSP 与供应链 |
| [troubleshooting.md](troubleshooting.md) | 常见问题、平台差异与已知限制 |
| [verification-history.md](verification-history.md) | 历史构建、测试与实机验证记录 |

项目级政策：

- [SECURITY.md](../SECURITY.md)：漏洞报告渠道
- [CONTRIBUTING.md](../CONTRIBUTING.md)：贡献流程
- [CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md)：社区行为准则
- [CHANGELOG.md](../CHANGELOG.md)：版本变更

文档使用中文；代码标识符、命令与路径保持英文。涉及实现细节时，以当前代码和测试为准。
