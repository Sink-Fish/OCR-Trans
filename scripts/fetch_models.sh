#!/usr/bin/env bash
# Download / convert all models used by ocr_trans into ./models (or $1).
#
#   OCR:   PP-OCRv5 mobile det + rec (ONNX, Apache-2.0) from RapidOCR's ModelScope mirror,
#          dictionary from PaddleOCR.
#   MT:    Helsinki-NLP/opus-mt-en-zh (CC-BY-4.0) -> CTranslate2 int8
#          facebook/m2m100_418M (MIT)            -> CTranslate2 int8   (ja->zh, optional: --with-m2m)
#   LLM:   Qwen3-1.7B Q4_K_M GGUF (Apache-2.0)                        (quality tier, optional: --with-llm)
#
# Conversion needs Python; we use `uv` to create a throwaway env so nothing is installed globally.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${MODELS_DIR:-models}"
WITH_M2M=0
WITH_LLM=0
for a in "$@"; do case "$a" in --with-m2m) WITH_M2M=1;; --with-llm) WITH_LLM=1;; --*) echo "unknown option: $a" >&2; exit 2;; *) OUT="$a";; esac; done
mkdir -p "$OUT/ocr"

T=""
CONVERT_TMP=""
CONVERT_DEST=""
cleanup() {
  [ -z "$T" ] || rm -rf "$T"
  if [ -n "$CONVERT_TMP" ]; then
    if [ -e "$CONVERT_TMP/previous" ] && [ ! -e "$CONVERT_DEST" ]; then
      mv "$CONVERT_TMP/previous" "$CONVERT_DEST" || {
        echo "previous model preserved at $CONVERT_TMP/previous" >&2; return
      }
    fi
    rm -rf "$CONVERT_TMP"
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
MS=https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.10.0/onnx/PP-OCRv5
dl() { # url destination sha256; never accept a nonempty but truncated model
  if [ -f "$2" ] && [ "$(shasum -a 256 "$2" | cut -d' ' -f1)" = "$3" ]; then echo "have $2"; return; fi
  echo "get  $2"
  curl -fL --connect-timeout 20 --retry 3 -o "$2.part" "$1"
  [ "$(shasum -a 256 "$2.part" | cut -d' ' -f1)" = "$3" ] || { echo "SHA-256 mismatch: $2" >&2; rm -f "$2.part"; return 1; }
  mv -f "$2.part" "$2"
}

dl "$MS/det/ch_PP-OCRv5_det_mobile.onnx" "$OUT/ocr/det.onnx" 4d97c44a20d30a81aad087d6a396b08f786c4635742afc391f6621f5c6ae78ae
dl "$MS/rec/ch_PP-OCRv5_rec_mobile.onnx" "$OUT/ocr/rec.onnx" 5825fc7ebf84ae7a412be049820b4d86d77620f204a041697b0494669b1742c5
dl "https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/dab3fe35379033fdcb2d0e9572fac0b36c9a9ebf/ppocr/utils/dict/ppocrv5_dict.txt" "$OUT/ocr/dict.txt" d1979e9f794c464c0d2e0b70a7fe14dd978e9dc644c0e71f14158cdf8342af1b

# ONNX Runtime shared library (MIT), loaded at runtime by the app (avoids protobuf symbol clashes
# with SentencePiece when statically linked).
./scripts/fetch_runtime.sh

command -v uv >/dev/null || { echo "uv not found (brew install uv / pip install uv)"; exit 1; }
PY=(uv run --no-project --python 3.12 --with ctranslate2 --with "transformers<5" --with sentencepiece --with torch --with protobuf)

convert() { # hf_id out_dir tokenizer_files fixed_revision
  local complete=1 f snapshot old
  for f in model.bin config.json $3; do [ -s "$2/$f" ] || complete=0; done
  { [ -s "$2/shared_vocabulary.json" ] || [ -s "$2/shared_vocabulary.txt" ]; } || complete=0
  if [ "$complete" = 1 ]; then echo "have $2"; return; fi
  echo "convert $1 -> $2 (int8)"
  snapshot=$("${PY[@]}" python -c 'from huggingface_hub import snapshot_download; import sys; print(snapshot_download(sys.argv[1], revision=sys.argv[2]))' "$1" "$4")
  CONVERT_TMP=$(mktemp -d "$OUT/.convert.XXXXXX")
  CONVERT_DEST="$2"
  "${PY[@]}" ct2-transformers-converter --model "$snapshot" --output_dir "$CONVERT_TMP/model" --quantization int8 \
     --copy_files $3 --force
  old="$CONVERT_TMP/previous"
  [ ! -e "$2" ] || mv "$2" "$old"
  if ! mv "$CONVERT_TMP/model" "$2"; then
    if [ -e "$old" ] && ! mv "$old" "$2"; then
      echo "previous model preserved at $old" >&2
      CONVERT_TMP=""
    fi
    return 1
  fi
  rm -rf "$CONVERT_TMP"
  CONVERT_TMP=""
}
convert Helsinki-NLP/opus-mt-en-zh "$OUT/opus-mt-en-zh" "source.spm target.spm" 408d9bc410a388e1d9aef112a2daba955b945255
if [ "$WITH_M2M" = 1 ]; then
  convert facebook/m2m100_418M "$OUT/m2m100_418m" "sentencepiece.bpe.model" 55c2e61bbf05dfb8d7abccdc3fae6fc8512fd636
fi
if [ "$WITH_LLM" = 1 ]; then
  # Qwen3-1.7B (Apache-2.0), 4-bit GGUF for llama.cpp — the optional high-quality tier
  mkdir -p "$OUT/qwen3-1.7b"
  dl "https://huggingface.co/unsloth/Qwen3-1.7B-GGUF/resolve/d7f544eead698dbd1f15126ef60b45a1e1933222/Qwen3-1.7B-Q4_K_M.gguf" "$OUT/qwen3-1.7b/model.gguf" b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897
fi
du -sh "$OUT"/* "$OUT"/ocr/*
