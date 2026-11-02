## 1. 后端：去掉静默安装参数

- [x] 1.1 在 `commands.rs` 的 `download_update` 中，去掉 NSIS `/S` 静默参数
- [x] 1.2 在 `commands.rs` 的 `download_update` 中，去掉 MSI `/quiet /norestart` 静默参数

## 2. 前端：更新提示文案

- [x] 2.1 在 `Settings.vue` 的 `handleDownloadUpdate` 中，修改确认弹窗文案

## 3. 验证

- [x] 3.1 `cargo check` 通过
- [x] 3.2 `pnpm run build` 通过
