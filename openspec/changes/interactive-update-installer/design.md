## Architecture Decisions

### 1. 去掉静默安装参数

**决策**：NSIS 不再使用 `/S`，MSI 不再使用 `/quiet`

**修改**：
- `Command::new(&installer_path).arg("/S")` → `Command::new(&installer_path)` （无参数）
- `Command::new("msiexec").args(["/i", &installer_path, "/quiet", "/norestart"])` → `Command::new("msiexec").args(["/i", &installer_path])`

### 2. 保持退出后启动安装程序的模式

**决策**：仍然在启动安装程序后退出应用（`std::process::exit(0)`），因为安装程序需要替换应用文件，必须先释放文件锁。

**流程变化**：
```
之前：下载 → /S 静默安装 → exit(0) → 用户无感知
之后：下载 → 提示"请完成安装" → 启动安装向导 → exit(0) → 用户手动完成安装 → 手动打开新版本
```

### 3. 前端提示文案优化

**决策**：修改确认弹窗文案，明确告知用户需要手动操作

**修改**：
- 之前：`安装过程中应用会自动关闭并重启`
- 之后：`应用将关闭并打开安装程序，请手动完成安装后重新启动应用`
