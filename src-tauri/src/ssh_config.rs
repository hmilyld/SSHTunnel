//! `~/.ssh/config` 解析器。
//!
//! 只做 GUI 下拉列表需要的最小化解析：
//! - 提取所有 `Host` 别名及其 `HostName` / `User` / `Port` / `IdentityFile`
//! - 忽略含通配符的 Host（`*`、`?`、`!` 开头的否定匹配）
//! - 忽略 `Match` 块内的条件配置（其作用域无法在静态解析中确定）
//! - 支持 `Key Value` 与 `Key=Value` 两种写法、行内 `#` 注释、引号包裹的值

use std::path::PathBuf;

use serde::Serialize;

/// 一个可直接用于 `ssh <alias>` 的主机条目
#[derive(Debug, Clone, Serialize)]
pub struct SshHost {
    /// Host 别名（下拉列表展示与命令行参数）
    pub alias: String,
    /// 实际主机名（未配置时回退为别名）
    pub hostname: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

/// 默认配置路径：Windows `C:\Users\<用户>\.ssh\config`，macOS `/Users/<用户>/.ssh/config`
pub fn default_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".ssh")
        .join("config")
}

/// 是否为通配符 Host（`*` / `?` / 否定 `!`）
fn is_wildcard(token: &str) -> bool {
    token.contains('*') || token.contains('?') || token.starts_with('!')
}

/// 去掉行内注释（`#` 需位于行首或空白之后，且不在引号内）
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_quotes = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_quotes = !in_quotes,
            b'#' if !in_quotes && (i == 0 || bytes[i - 1].is_ascii_whitespace()) => {
                return &line[..i];
            }
            _ => {}
        }
    }
    line
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

/// 解析 SSH 配置文本，按出现顺序返回 Host 别名
pub fn parse(content: &str) -> Vec<SshHost> {
    let mut hosts: Vec<SshHost> = Vec::new();
    // 当前 `Host` 行声明的别名在 hosts 中的下标（一行可声明多个别名，共享后续配置）
    let mut group: Vec<usize> = Vec::new();
    let mut in_match = false;

    for raw in content.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        // 拆分 key / value：允许 `Key Value`、`Key = Value`、`Key=Value`
        let key_end = line
            .find(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or(line.len());
        let key = line[..key_end].to_ascii_lowercase();
        let mut rest = line[key_end..].trim_start();
        if let Some(stripped) = rest.strip_prefix('=') {
            rest = stripped.trim_start();
        }

        match key.as_str() {
            "host" => {
                in_match = false;
                group.clear();
                for token in rest.split_whitespace() {
                    let token = unquote(token);
                    if token.is_empty() || is_wildcard(&token) {
                        continue;
                    }
                    match hosts.iter().position(|h| h.alias.eq_ignore_ascii_case(&token)) {
                        // 同一个别名后出现的 Host 行：OpenSSH 取第一个定义，后续仅补充空缺字段
                        Some(idx) => group.push(idx),
                        None => {
                            hosts.push(SshHost {
                                alias: token.clone(),
                                hostname: token,
                                user: None,
                                port: None,
                                identity_file: None,
                            });
                            group.push(hosts.len() - 1);
                        }
                    }
                }
            }
            "match" => {
                // Match 条件块：作用域依赖运行时环境，静态解析时整块忽略
                in_match = true;
                group.clear();
            }
            _ if in_match || group.is_empty() => {}
            "hostname" => {
                let v = unquote(rest);
                if !v.is_empty() {
                    for &i in &group {
                        hosts[i].hostname = v.clone();
                    }
                }
            }
            "user" => {
                let v = unquote(rest);
                if !v.is_empty() {
                    for &i in &group {
                        hosts[i].user = Some(v.clone());
                    }
                }
            }
            "port" => {
                if let Ok(p) = rest.trim().parse::<u16>() {
                    if p > 0 {
                        for &i in &group {
                            hosts[i].port = Some(p);
                        }
                    }
                }
            }
            "identityfile" => {
                let v = unquote(rest);
                if !v.is_empty() {
                    for &i in &group {
                        hosts[i].identity_file = Some(v.clone());
                    }
                }
            }
            _ => {}
        }
    }

    // 别名不区分大小写去重（保留首个）
    let mut seen = std::collections::HashSet::new();
    hosts.retain(|h| seen.insert(h.alias.to_ascii_lowercase()));
    hosts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_hosts_and_ignores_wildcards() {
        let cfg = r#"
# 开发机
Host dev
    HostName 10.0.0.1
    User alice
    Port 2222
    IdentityFile ~/.ssh/id_ed25519

Host web1 web2
    HostName example.com

Host * !blocked
    ForwardAgent no

Host=equals-host
    HostName equals.example.com

Host plain

Match host badhost
    User ignored
"#;
        let hosts = parse(cfg);
        let aliases: Vec<_> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(aliases, vec!["dev", "web1", "web2", "equals-host", "plain"]);

        let dev = &hosts[0];
        assert_eq!(dev.hostname, "10.0.0.1");
        assert_eq!(dev.user.as_deref(), Some("alice"));
        assert_eq!(dev.port, Some(2222));
        assert_eq!(dev.identity_file.as_deref(), Some("~/.ssh/id_ed25519"));

        // 同一 Host 行的多个别名共享同一条 HostName 指令
        assert_eq!(hosts[1].hostname, "example.com");
        assert_eq!(hosts[2].hostname, "example.com");
        assert_eq!(hosts[3].hostname, "equals.example.com");

        // 未配置 HostName 时回退为别名本身
        assert_eq!(hosts[4].hostname, "plain");
    }

    #[test]
    fn strips_inline_comments() {
        let hosts = parse("Host a # 注释\n  HostName a.example.com # 也是注释\n");
        assert_eq!(hosts[0].hostname, "a.example.com");
    }
}
