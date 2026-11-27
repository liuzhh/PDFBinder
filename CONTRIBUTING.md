# 贡献指南

感谢你愿意改进 PDF Binder。这个项目很小，改动请尽量保持集中。

## 开发环境

- Rust **1.95+**（stable 即可）
- Windows 10 / 11 + MSVC 工具链（Visual Studio 或 Build Tools 的 C++ 桌面开发工作负载）
- 只有发版脚本需要 Node 18+，构建与测试本身不需要 Node

```powershell
cargo build --release
cargo test
```

终端里 `cargo` 找不到 `link.exe` 时，用 `pwsh -File scripts/build.ps1` 自动进入 MSVC 编译环境。

## 提交前必须通过

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI 会在 `windows-latest` 上重复这些检查，并额外构建一次 release 产物。

## 代码风格

- **Rust**：`rustfmt` 默认配置，`clippy` 不允许 warning
- 只在需要解释「为什么」的地方写注释，不要解释「做了什么」
- 面向用户的提示文案统一用中文，措辞与现有界面保持一致
- 不引入不必要的依赖：项目刻意保持「单个 exe、无 WebView、无内置字体」的取舍

## 修改合并逻辑时

`src/merge.rs` 的 `merges_in_requested_order` 测试覆盖合并顺序、页面内容跟随页面、倒序合并与输出页数校验。合并行为一旦变化：

1. 在 `sample_pdf()` 中构造需要的新结构
2. 在 `merges_in_requested_order` 中增加或修正对应断言
3. 如果属于「已知限制」的解除，请同步更新 README 的对应条目

## 关于样例 PDF（重要）

**不要提交真实的 PDF 文档。** 合同、发票、证件扫描件等 PDF 包含姓名、证件号、
金额、签名和印章——提交进公开仓库就是泄露个人信息与商业机密。

- 测试样例请在测试代码里用 `lopdf` 现场生成
- `*.pdf` 已在 `.gitignore` 中，请勿用 `git add -f` 强行加入
- 自己调试用的真实文档放在本地即可，不要放进仓库

## 分支与提交

- 从 `main` 切分支，命名建议 `fix/xxx`、`feat/xxx`、`docs/xxx`
- 提交信息用英文的祈使句，例如 `fix: keep page resources for inherited boxes`
- 一个 PR 只做一件事；大改动请先开 Issue 讨论

## 许可证

你的贡献将以 [MIT](./LICENSE) 许可证发布。