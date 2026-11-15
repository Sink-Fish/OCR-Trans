# OCR Trans

OCR Trans 是适用于 macOS 和 Windows 的离线截图识字与翻译工具。按快捷键框选屏幕，即可提取文字，或将英文、日文原位翻译成中文。结果可以钉在其他窗口上方，切换原文与译文、添加标注或保存为 PNG。

截图、识字和翻译在本机完成。安装包内置 OCR 和英译中快速模型。日译中快速模型和高质量翻译模型可在设置中按需下载。截图历史保存在本机，可以在应用内删除。

## 安装

目前已公开 [v0.6.4 源码](https://github.com/Sink-Fish/OCR-Trans/tree/v0.6.4)。首个安装包正在准备，发布后可从 [Releases](https://github.com/Sink-Fish/OCR-Trans/releases) 下载。

| 平台 | 安装文件 | 首次使用 |
| --- | --- | --- |
| macOS 13 及以上，Apple 芯片 | `arm64.dmg` | 将 OCR Trans 拖入“应用程序”后打开，并授予屏幕录制权限。当前分发包使用本地签名，未经 Apple 公证。 |
| Windows x64 | `x64-setup.exe` | 运行安装程序。缺少 Visual C++ x64 运行库时，按提示从微软官方下载后重新安装。当前分发包未经 Authenticode 签名。 |

## 快速使用

1. 在 Mac 上按 `⌥⇧O`，在 Windows 上按 `Alt+Shift+O`，拖动鼠标框选要处理的区域。快捷键可在设置中修改。
2. 按 `1` 翻译、`2` 识字、`3` 钉图或 `4` 标注。识字窗口可以选取、编辑和复制文字。
3. 在钉图窗口按 `T` 切换原文和译文，滚轮缩放，双击译文段落可以修改文字。

详细操作、模型管理、历史记录和权限排查请参见[使用手册](docs/USER_GUIDE.md)。

## 从源码构建

在 Apple Silicon Mac 上准备 Xcode 命令行工具、Rust 工具链、CMake、Node.js 22.18 或更高版本、pnpm、Python 3 和 uv。在仓库根目录运行：

```sh
pnpm install --frozen-lockfile
./scripts/fetch_models.sh
pnpm tauri dev
```

生成 macOS 安装包可运行 `./scripts/package_mac.sh --no-install`。Windows x64 的原生构建入口为 [build_windows_native.ps1](scripts/build_windows_native.ps1)，需要 x64 MSVC 开发环境以及脚本所列的工具、模型与运行时。项目也提供 [macOS 交叉构建脚本](scripts/build_windows.sh)。

```sh
pnpm test
cargo test --workspace --release --locked
python3 -m unittest discover -s scripts/tests
```

## 使用许可与反馈

官方安装包免费下载。自有代码和文档允许非商业使用、修改和免费再分发。企业内部业务使用、收费销售、集成到商业产品或提供商业服务，均须事先取得 SinkFish 的书面授权。非官方修改版须标明发布者。模型、字体及其他第三方组件适用各自的许可。[项目许可](LICENSE) · [第三方清单](docs/LICENSE_INVENTORY.md)

问题反馈和商业授权联系请使用 [GitHub Issues](https://github.com/Sink-Fish/OCR-Trans/issues)。提交问题时请注明应用版本、操作系统和复现步骤，并先遮盖日志或截图中的私人内容。
