import fs from "node:fs";

const args = process.argv.slice(2);
const tag = args.find((arg) => !arg.startsWith("--"));
if (!tag) {
  console.error("usage: node scripts/release-notes.mjs vX.Y.Z [--raw]");
  process.exit(1);
}

const version = tag.replace(/^v/, "");
const changelog = fs.readFileSync("CHANGELOG.md", "utf8");
const escaped = version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const heading = new RegExp(`^## \\[${escaped}\\](?:\\s*-\\s*.*)?\\s*$`, "m");
const match = heading.exec(changelog);

if (!match) {
  console.error(`CHANGELOG.md 中找不到版本 ${version} 的章节（## [${version}]）`);
  process.exit(1);
}

const afterHeading = changelog.slice(match.index + match[0].length);
const nextHeading = afterHeading.search(/^## \[/m);
const section = (nextHeading === -1 ? afterHeading : afterHeading.slice(0, nextHeading)).trim();

if (!section) {
  console.error(`CHANGELOG.md 中版本 ${version} 的章节为空`);
  process.exit(1);
}

const body = `跨平台 SSH 本地端口转发（ssh -L）管理器 — https://hmilyld.com

Windows：下载 \`*-setup.exe\`（NSIS）或 \`.msi\`。
macOS：下载对应架构的 \`.dmg\`；安装包未签名/公证，首次打开需在系统设置中放行。

## 更新内容

${section}

---

完整变更记录：https://github.com/hmilyld/SSHTunnel/blob/main/CHANGELOG.md
`;

if (args.includes("--raw")) {
  process.stdout.write(body);
} else {
  process.stdout.write(`body<<CHANGELOG_EOF\n${body}\nCHANGELOG_EOF\n`);
}
