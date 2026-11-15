# OCR Trans 第三方许可证清单（0.6.4）

本文列出 OCR Trans 的 macOS 安装包（`OCR-Trans_<版本>_arm64.dmg`）和 Windows 安装包（`OCR-Trans_<版本>_x64-setup.exe`）里分发的、在运行时下载的、以及只在构建时用到的第三方组件，写明各自的许可证和由此产生的义务，并说明这些义务落实在什么地方。

- 许可证原文的汇总文件：`src-tauri/licenses/THIRD-PARTY-NOTICES.txt`，由 `python3 scripts/gen_notices.py` 生成（约 1.3 MB）。两个安装包都带这个文件（macOS：`OCR Trans.app/Contents/Resources/licenses/`；Windows：安装目录下的 `licenses\`），在应用里也能看：*设置 → 关于 → 查看全部第三方许可证*。
- 依赖有变化（`Cargo.lock`、`package.json`、模型版本）就重新运行 `scripts/gen_notices.py`。缺少任何一份许可证原文时它会直接报错退出，不会悄悄漏掉。
- 本清单是工程上的盘点，不是法律意见。

## 1. 结论

| 项目 | 结论 |
|---|---|
| Copyleft | 没有 GPL、LGPL、AGPL 组件。仅有的“弱 copyleft”是 5 个 MPL-2.0 的 Rust crate（文件级 copyleft，未作修改）。 |
| 需要随附许可证原文的 | MIT、BSD、ISC、Zlib、Apache-2.0、Unicode-3.0、MPL-2.0、OFL 等：原文都在 THIRD-PARTY-NOTICES.txt 里，两个安装包都带。 |
| Apache-2.0 的 NOTICE 文件（§4(d)） | 已逐一检查：注册表里的 Rust crate、CTranslate2 及其第三方代码、llama.cpp、SentencePiece、PaddleOCR、RapidOCR、Qwen3 都**没有** NOTICE 文件，所以没有需要照抄的 NOTICE。ONNX Runtime 和 WebView2 SDK 是 Microsoft 的，它们的声明文件已全文收录。 |
| 署名（CC BY 4.0） | opus-mt-en-zh 模型：*设置 → 关于* 和 THIRD-PARTY-NOTICES.txt 都写了作者、来源、许可证链接，以及“已转换为 CTranslate2 格式并量化，未作其他修改”。 |
| MPL-2.0 源代码 | 5 个 crate 都未修改，THIRD-PARTY-NOTICES.txt 写明了各自的源代码地址（crates.io 和代码仓库），满足 MPL §3.2 的要求。 |
| 专利条款 | rav1e（BSD-2-Clause，图片编码依赖）附带的 *Alliance for Open Media Patent License 1.0* 已全文收录。 |
| 字体 | 应用里没有字体文件。Logo 中的“文”取自思源黑体 Bold 的字形并转成了矢量轮廓，OFL 1.1 原文已收录。 |
| 应用本身 | 自有材料按仓库根 LICENSE 的非商业条款提供。任何商业用途须另获书面授权。第三方材料按各自许可分发。 |

## 2. 安装包里有什么

| 内容 | macOS（dmg） | Windows（NSIS exe） | 许可证 |
|---|---|---|---|
| 应用主程序（Rust，静态链接下面第 4、5 节的库） | `Contents/MacOS/ocr-trans` | `ocr-trans.exe` | 见第 4、5 节 |
| ONNX Runtime 1.31.0 | `Contents/Resources/runtime/libonnxruntime.dylib` | `onnxruntime.dll` | MIT，另附 Microsoft 的 ThirdPartyNotices 全文 |
| OCR 模型 PP-OCRv5 mobile 检测和识别、字典 | `Resources/models/ocr/` | `models\ocr\` | Apache-2.0 |
| 英→中翻译模型 opus-mt-en-zh（CTranslate2，int8） | `Resources/models/opus-mt-en-zh/` | `models\opus-mt-en-zh\` | CC BY 4.0 |
| 网页界面（React 等，打包进主程序） | 内嵌 | 内嵌 | MIT / Apache-2.0 |
| 图标（含“文”字形） | `icon.icns`、`tray.png` | `icon.ico`、`tray.png` | 自有；字形来自思源黑体（OFL 1.1） |
| 许可证汇总 | `Resources/licenses/THIRD-PARTY-NOTICES.txt` | `licenses\THIRD-PARTY-NOTICES.txt` | — |
| Microsoft WebView2 SDK 的 `WebView2LoaderStatic.lib` | — | 静态链接 | Microsoft 的 BSD-3-Clause 风格许可，LICENSE 和 NOTICE 已收录 |
| llama.cpp / ggml 的 DLL（`llama.dll`、`ggml*.dll`、按指令集分的 `ggml-cpu-*.dll`、`ggml-vulkan.dll`，0.6.1 起） | — | 安装目录 | MIT（同第 4 节的 llama.cpp） |
| Vulkan-Headers 1.4.363（`vulkan.h` / `vulkan.hpp`，编译进 `ggml-vulkan.dll`） | — | 编译进 DLL | Apache-2.0 OR MIT（按 MIT 使用），LICENSE 已收录 |
| SPIRV-Headers 1.4.363（`spirv.hpp`，编译进 `ggml-vulkan.dll`） | — | 编译进 DLL | Khronos 的 MIT 式许可，LICENSE 已收录 |
| Vulkan 加载器 `vulkan-1.dll` | — | **不随包**：显卡驱动自带；没有时 Vulkan 后端不加载，自动改用 CPU | 驱动厂商的条款 |
| Microsoft C/C++ 运行库（静态链接，`+crt-static`） | — | 静态链接 | Visual Studio / Build Tools 许可中的“可分发代码”，见第 7 节 |
| NSIS 安装程序外壳 | — | 安装程序本身 | zlib/libpng（二进制分发没有附带义务） |

**不在安装包里的：**
- Microsoft Visual C++ x64 运行库由用户从微软获取。安装器会检测先决条件，缺少时提供官方页面。
- WebView2 Runtime：Windows 的系统组件。Tauri 安装程序的默认设置是：电脑上没有时，下载 Microsoft 的引导程序来安装。适用 Microsoft 自己的条款，我们不分发它。
- macOS 的 WebKit、系统框架，以及截图用的系统 API。

## 3. 模型

| 模型 | 来源与版本 | 许可证 | 随包 / 下载 | 修改 | 我们做了什么 |
|---|---|---|---|---|---|
| PP-OCRv5 mobile 检测和识别（ONNX） | PaddleOCR（© PaddlePaddle Authors），ONNX 由 RapidOCR 转换，ModelScope `RapidAI/RapidOCR` 的 v3.10.0 版 | Apache-2.0 | 随包 | 无 | 写明版权和来源，附许可证原文 |
| PP-OCRv5 字典 `ppocrv5_dict.txt` | PaddleOCR | Apache-2.0 | 随包 | 无 | 同上 |
| opus-mt-en-zh | Helsinki-NLP（赫尔辛基大学，Jörg Tiedemann），Tatoeba eng-zho opus-2020-07-17 | CC BY 4.0 | 随包（int8 CTranslate2）；也可在设置里改为下载 gaudi 的 float16 CTranslate2 版（固定版本 dcd22168…） | 格式转换、量化 | 关于页和 notices 写明作者、来源、许可证链接、“有修改：格式转换与量化”和免责声明，并附推荐引用的论文 |
| M2M100 418M | Meta AI（fairseq），CTranslate2 版由 michaelfeil 转换（固定版本 06bd8329…） | MIT | 用户在设置里下载 | 无（第三方转换） | 版权和许可证写进 notices 和关于页 |
| Qwen3-1.7B | 阿里巴巴通义千问团队，GGUF Q4_K_M 量化版由 unsloth 制作（固定版本 d7f544ee…） | Apache-2.0 | 用户在设置里下载 | 无（第三方量化） | 同上 |

说明：
- opus-mt-en-zh 在 Hugging Face 页面上标的是 apache-2.0，但原始发布包里的 LICENSE 是 CC BY 4.0。我们按要求更严的 CC BY 4.0 处理。
- 所有下载地址都固定到具体版本，并校验文件大小和 SHA-256（0.5.6 起），所以下载到的内容就是上面写的版本。

## 4. 编译进主程序的原生库（C/C++）

| 组件 | 版本 / 来源 | 许可证 |
|---|---|---|
| CTranslate2 | 4.8，通过 ct2rs 0.10.2 静态链接 | MIT |
| ct2rs（Rust 绑定） | 0.10.2（crate 里没有许可证文件，原文放在 `scripts/notices/`） | MIT |
| Ruy（GEMM 后端，macOS 和 Windows 都用它） | CTranslate2 自带 | Apache-2.0 |
| cpuinfo、clog（Ruy 自带） | — | BSD-2-Clause |
| cpu_features | CTranslate2 自带 | Apache-2.0 |
| spdlog（只有头文件） | CTranslate2 自带 | MIT |
| BS::thread_pool（只有头文件） | CTranslate2 自带 | MIT |
| avx512_mathfun.h（SIMD_Utils） | CTranslate2 自带 | BSD-2-Clause |
| avx_mathfun.h / neon_mathfun.h | CTranslate2 自带 | MIT（Marian 的修改）+ zlib（Garberoglio、Pommier 的原版）。没有单独的许可证文件，许可证写在头文件开头的注释里，生成器按原样摘录（0.6.0 补上） |
| llama.cpp / ggml | 通过 llama-cpp-sys-2 0.1.160 | MIT |
| SentencePiece | 通过 sentencepiece-sys 0.13.2 | Apache-2.0 |
| Abseil、protobuf-lite、Darts-clone、esaxx（SentencePiece 自带） | — | Apache-2.0、BSD-3-Clause、BSD-2-Clause、MIT |

没有编译进来、所以不在清单里的：CTranslate2 的 CUDA/cuDNN/cutlass/thrust 路径、测试（googletest）、命令行工具（cxxopts）；llama.cpp 的服务器和多模态工具。

Windows 不再用 Intel oneMKL（0.6.0 改动）：它的预编译包没法在 macOS 上交叉编译，而且会把 Intel 的专有再分发许可带进安装包。现在 Windows 也用 Ruy，许可证只有 Apache-2.0。

## 5. Rust crate

生成器从 `cargo metadata` 出发，沿真实的依赖图（只算普通依赖）分别统计 macOS（aarch64-apple-darwin）和 Windows（x86_64-pc-windows-msvc）实际链接的 crate，再取两者的并集：

- 当前生成结果覆盖两个目标共 454 个 Rust crate。仅一个平台使用的依赖在通知中带有平台标记。
- 许可证文本一样的（大多数 crate 带的是同一份 MIT / Apache-2.0）只印一次，前面列出所有使用这份文本的 crate。

需要单独说明的：
- **MPL-2.0**：cssparser 0.37.0、cssparser-macros 0.7.1、dtoa-short 0.3.5、selectors 0.38.0（Tauri 处理 HTML 时用到），option-ext 0.2.0（dirs 用到）。都没有修改，notices 里写了源代码地址。
- **BSL-1.0**：clipboard-win、error-code（只在 Windows 用）。只以二进制形式分发时 BSL 不要求附带任何东西，原文照样收录了。
- **Unicode-3.0**：ICU4X 的数据和代码。要求随附许可证原文，已收录。
- **rav1e**：BSD-2-Clause，外加 AOM Patent License 1.0。生成器连同 `PATENTS` 文件一起收录了。
- **ring**：Apache-2.0 AND ISC，原文已收录。
- 自己没带许可证文件的 crate（如 alloc-stdlib、objc2-* 等）在 notices 末尾按名字列出，它们的 SPDX 许可证标准文本前面已有（例如 alloc-stdlib 和 alloc-no-stdlib 属于同一个仓库，许可证一样：BSD-3-Clause，© Dropbox）。objc2、ct2rs、webview2-com 这些上游有许可证文件、只是 crate 包里没带的，原文从上游取来放在 `scripts/notices/`。

## 6. 网页界面（npm）

打包进主程序网页界面的运行时代码：react 19.3.1、react-dom 19.3.1、scheduler（MIT），@tauri-apps/api 2.12.4、@tauri-apps/plugin-autostart、@tauri-apps/plugin-opener（Apache-2.0 OR MIT）。还有打包器（vite / rolldown，MIT）注入的几行辅助代码，它们的许可证也一并收录。只在开发时用的包（TypeScript、Vite 本身等）不分发，不列出。

## 7. 只在构建时用、不分发的工具

| 工具 | 许可证 / 条款 | 备注 |
|---|---|---|
| Rust 工具链、Cargo、Node、pnpm、Vite、TypeScript | MIT / Apache-2.0 等 | 不进安装包 |
| cargo-xwin + Microsoft CRT 与 Windows SDK 的头文件和库 | Microsoft 的 Visual Studio / Build Tools 许可条款 | cargo-xwin 第一次运行时下载，并代为接受 Microsoft 的许可。分发者应核对适用于自己的微软许可及可分发清单。 静态链接的 C/C++ 运行库属于这些条款里允许随应用分发的“可分发代码”。 |
| LLVM（clang-cl、lld-link、llvm-rc、llvm-dlltool） | Apache-2.0 WITH LLVM-exception | 编译器例外条款：用它编译出的程序没有附带义务。`vulkan-1.lib` 导入库由 llvm-dlltool 从 Vulkan 头文件生成（`scripts/dev/vulkan_def.py`） |
| shaderc（glslc）、glslang、SPIRV-Tools（Homebrew，0.6.1 起） | Apache-2.0 等 | 只用来在构建时把 ggml 的 GLSL 计算着色器编译成 SPIR-V；生成的着色器属于 llama.cpp（MIT），工具本身不分发 |
| NSIS（makensis） | zlib/libpng，部分为 bzip2 / CPL | 安装程序外壳的二进制分发没有附带义务 |
| `scripts/patches/ct2rs-target-cfg.patch` | 修改的是 ct2rs 的 build.rs（MIT） | 只为交叉编译 Windows 时按目标平台、而不是按构建机器来判断；不改动分发的源代码或库 |

## 8. 落实情况

- [x] THIRD-PARTY-NOTICES.txt 覆盖第 3 至 6 节的所有组件，两个安装包都带（`tests/about.test.ts` 检查两个平台的打包配置都包含它，以及几条关键署名都在）。
- [x] 应用内能看全文：*设置 → 关于 → 查看全部第三方许可证*（也能用文本编辑器打开）。
- [x] CC BY 4.0 的署名和修改说明（关于页 + notices）。
- [x] MPL-2.0 源代码地址、AOM 专利许可、ONNX Runtime 和 WebView2 的声明文件。
- [x] 头文件里注释形式的许可证（avx_mathfun.h、neon_mathfun.h）：生成器按原样摘录（0.6.0 补上，之前因此生成失败、notices 停在旧版本）。
- [x] 模型下载固定版本，并校验大小和 SHA-256。

## 公开源码中的上游材料

- `scripts/patches/ct2rs-target-cfg.patch` 含 ct2rs 0.10.2 的构建脚本上下文，随附上游 MIT 文本见 `scripts/notices/ct2rs-0.10.2-LICENSE.txt`。
- `scripts/patches/llama-cpp-sys-2-clang-cl.patch` 含 llama-cpp-sys-2 0.1.160 的 Cargo 配置及其所带 llama.cpp 的 CMake 上下文。绑定声明 MIT OR Apache-2.0，llama.cpp 使用 MIT。上游文字及其权利人保留原许可。
- 桌面入口使用 Tauri 常见的 Windows 子系统配置和启动样板。Tauri 项目与脚手架采用 MIT 或 Apache-2.0。项目的非商业条款仅适用于自有实现。
- 品牌字形来源见 [品牌素材记录](brand/LICENSES.md)。第三方声明文件和模型授权依照各自所列条款。
