// 0.6.1: high-quality translation speed and idle use. The Windows build of llama.cpp ran on plain SSE2 and used
// efficiency cores; long text came back in English; Metal kept a thread waking every 5 ms. These guards read the
// sources (the logic itself has unit tests in crates/core: cpu.rs, llm.rs, and tests/llm_long.rs on the real model).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dirname, "..");
const read = (p: string) => readFileSync(join(ROOT, p), "utf8");

test("Windows builds llama.cpp with run-time CPU variants and Vulkan", () => {
  const toml = read("crates/core/Cargo.toml");
  assert.match(toml, /\[target\.'cfg\(target_os = "windows"\)'\.dependencies\][^\[]*llama-cpp-2 = \{[^}]*features = \["dynamic-backends", "vulkan"\]/);
  const llm = read("crates/core/src/llm.rs");
  assert.match(llm, /if cpu_only \{ load_cpu_backend\(dir\)[\s\S]*?else \{ llama_cpp_2::llama_backend::load_backends_from_path\(dir\); \}[\s\S]*?LlamaBackend::init\(\)/);
  const sh = read("scripts/build_windows.sh");
  assert.match(sh, /"\$OUT"\/bin\/\*\.dll "\$OUT"\/backends\/\*\.dll/, "every llama DLL goes into the installer");
  assert.match(sh, /grep -q "ggml-cpu"/);
  assert.match(sh, /llama-cpp-sys-2-clang-cl\.patch/);
});

test("threads follow the processor's performance cores", () => {
  const llm = read("crates/core/src/llm.rs");
  assert.ok(!/available_parallelism\(\)\.map\(\|n\| n\.get\(\)\.min\(8\)/.test(llm), "no more 'all cores up to 8'");
  assert.match(llm, /crate::cpu::llm_threads\(\)/);
  assert.match(read("crates/core/src/cpu.rs"), /GetLogicalProcessorInformationEx/);
  assert.match(read("crates/core/src/cpu.rs"), /hw\.perflevel0\.physicalcpu/);
});

test("untranslated paragraphs go to the fast engine, with a note", () => {
  const lib = read("crates/core/src/lib.rs");
  assert.match(lib, /段已用快速翻译/);
  assert.match(lib, /Err\(e\) if used_llm => \{[\s\S]*continue;/, "a failing fast engine keeps the LLM's translations");
  const llm = read("crates/core/src/llm.rs");
  assert.match(llm, /if r\.as_deref\(\)\.is_some_and\(\|t\| untranslated\(src, t\)\)/);
  assert.match(llm, /deadline = Some\(t_gen \+ gen_budget\(exp, tps\)\)/);
});

test("idle: no spinning ONNX Runtime pool, no Metal heartbeat, the model released after idle minutes", () => {
  assert.match(read("crates/core/src/ocr.rs"), /\.with_intra_op_spinning\(false\)/);
  assert.match(read("src-tauri/src/main.rs"), /set_var\("GGML_METAL_NO_RESIDENCY", "1"\)/);
  const st = read("src-tauri/src/state.rs");
  assert.match(st, /let minutes = self\.settings\.lock_safe\(\)\.llm_idle_minutes;/);
  assert.match(read("src-tauri/src/settings.rs"), /llm_idle_minutes: 5,/);
});

test("settings: device choice, current device, GPU failure remembered", () => {
  const s = read("src/views/Settings.tsx");
  assert.match(s, /options=\{\[\["auto", "自动"\], \["cpu", "仅 CPU"\]\]\}/);
  assert.match(s, /重新尝试显卡/);
  assert.match(s, /api\.llmInfo\(\)/);
  const cmd = read("src-tauri/src/commands.rs");
  assert.match(cmd, /fn remember_gpu_failure\(app: &AppHandle\)/);
  assert.match(cmd, /"正在等待或加载高质量模型…"/);
  assert.match(read("src/views/Pin.tsx"), /win\.listen<string>\("pin-busy"/);
});
