fn main() {
    // 1) 校验 tauri.conf.json 并生成能力/模式 schema
    // 2) Windows 下把 icon.ico 嵌入 exe 资源（任务栏/文件管理器图标）
    tauri_build::build()
}
