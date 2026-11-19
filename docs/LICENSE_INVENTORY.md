# 第三方组件与许可

本分支许可迁移后的 OCR Trans 自有材料采用 GPL-3.0-only，仅适用于 SinkFish 有权授权的部分；已发布 0.6.5 文件的许可不随此文档变更。范围和版本边界见 [许可说明](LICENSING.md)。第三方组件继续适用各自的许可，使用者可依上游条款单独使用这些组件。安装包内的 `licenses/THIRD-PARTY-NOTICES.txt` 包含许可证、署名、适用声明及源码获取地址。也可以在“设置 → 关于”中查看。[声明全文](../src-tauri/licenses/THIRD-PARTY-NOTICES.txt)

## 模型与数据

| 内容 | 来源 | 分发方式和处理 | 许可 |
| --- | --- | --- | --- |
| PP-OCRv5 mobile 检测、识别模型与字典 | PaddleOCR；ONNX 版本来自 RapidOCR | 安装包内置转换后的模型和字典 | Apache-2.0 |
| OPUS-MT 英译中 | 赫尔辛基大学 Helsinki-NLP，eng-zho opus-2020-07-17 | 内置版本已转为 CTranslate2 并量化为 int8；模型管理器另提供 gaudi 的 float16 转换版本 | 原始模型发行包标明 CC BY 4.0；随附作者、来源、许可与修改说明 |
| M2M100 418M | Meta；CTranslate2 下载版本由 michaelfeil 转换 | 按需下载 | MIT |
| Qwen3-1.7B | Qwen 团队；GGUF Q4_K_M 下载版本由 Unsloth 量化 | 按需下载 | Apache-2.0 |

OPUS-MT 的模型卡和原始发行包对许可的表述有差异。本项目保留原始包的署名与修改说明。完整版本号和下载地址见声明全文。[模型卡](https://huggingface.co/Helsinki-NLP/opus-mt-en-zh) · [原始发行包](https://object.pouta.csc.fi/Tatoeba-MT-models/eng-zho/opus-2020-07-17.zip) · [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)

## 软件与图稿

| 内容 | 许可或来源 |
| --- | --- |
| ONNX Runtime | macOS 14 及以上与 Windows 使用 1.31.0，macOS 13 使用 1.18.1；MIT，随附两个版本的 Microsoft 第三方声明 |
| CTranslate2、llama.cpp / ggml、SentencePiece 及其依赖 | 分别保留 MIT、Apache-2.0、BSD 等上游声明；Windows 另附 Vulkan 和 SPIR-V 头文件声明 |
| Rust 依赖 | 依据 Cargo 锁定版本生成。声明全文列出 MPL-2.0 组件的对应版本源码地址，并保留 rav1e 附带的 AOM 专利许可 |
| 前端运行代码 | React、React DOM、scheduler、Tauri API 及所用插件的声明见全文 |
| 桌面入口与补丁 | Tauri 入口样板适用 MIT 或 Apache-2.0。`scripts/patches/` 包含 ct2rs 的 MIT 构建代码、其依赖 cpuinfo 的 BSD-2-Clause 头文件片段，以及 llama-cpp-sys-2（MIT 或 Apache-2.0）与 llama.cpp（MIT）的构建上下文。各段原有许可继续适用 |
| 应用与菜单栏图标 | “文”字形来自 Adobe 思源黑体，采用 SIL OFL 1.1。详见[图稿来源](brand/LICENSES.md) |

## Windows 运行环境

安装器检测外部 Visual C++ x64 运行库，缺少时提供微软官方下载页。安装器不附带独立的 VC++ 运行库安装程序。当前构建的 Rust 主程序和 CTranslate2 使用静态 CRT，llama.cpp / ggml 动态库使用共享 CRT。两种链接方式应分别核对。构建和再分发须遵守适用于构建者的 Visual Studio、Build Tools 和 Windows SDK 条款。[微软构建工具许可](https://visualstudio.microsoft.com/license-terms/) · [微软再分发说明](https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution)

应用使用 WebView2 SDK 加载器，包含 SDK 的 LICENSE 和 NOTICE。WebView2 Runtime 由系统提供或由微软安装程序获取。[WebView2 分发文档](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)

## 更新声明

在获取锁定的 Cargo 和前端依赖后，运行 `python3 scripts/gen_notices.py` 更新声明全文。修改依赖、模型版本、补丁或图稿时，应同步检查该版本的许可证、NOTICE、署名与源码提供要求。
