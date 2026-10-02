# GUFO

```sh
nix build .#gufo

MODELS_DIR=/home/emre/.storage/persistent/Desktop/projects/gufo/models
nix shell --inputs-from . nixpkgs#python3Packages.huggingface-hub -c \
  hf download unsloth/Qwen3.8-27B-GGUF Qwen3.8-27B-UD-Q4_K_XL.gguf \
  --revision 4ca720788d1e01f1bff70c033e0d0028fd02e502 \
  --local-dir "$MODELS_DIR/qwen3.8-27b"
nix shell --inputs-from . nixpkgs#python3Packages.huggingface-hub -c \
  hf download z-lab/Qwen3.8-27B-DFlash2-GGUF Qwen3.8-27B-DFlash2-Q4_K_M.gguf \
  --revision 2d9571f8ce46e151f61c6499c99dee6079e1d610 \
  --local-dir "$MODELS_DIR/qwen3.8-27b"

./result/bin/gufo-server serve \
  --host 0.0.0.0 \
  --port 8080 \
  --sessions 1 \
  llm \
  --model "$MODELS_DIR/qwen3.8-27b/Qwen3.8-27B-UD-Q4_K_XL.gguf" \
  --speculative dflash2 \
  --dflash-model "$MODELS_DIR/qwen3.8-27b/Qwen3.8-27B-DFlash2-Q4_K_M.gguf" \
  --served-model-name coding \
  --context 262144 \
  --max-tokens 8192 \
  --think on \
  --reasoning-effort medium
```

```sh
MODELS_DIR=/home/emre/.storage/persistent/Desktop/projects/gufo/models
nix shell --inputs-from . nixpkgs#python3Packages.huggingface-hub -c \
  hf download Qwen/Qwen3-ASR-1.7B \
  --revision 7278e1e70fe206f11671096ffdd38061171dd6e5 \
  --local-dir "$MODELS_DIR/Qwen3-ASR-1.7B"

./result/bin/gufo-server serve \
  --host 0.0.0.0 \
  --port 8084 \
  --max-request-bytes 67108864 \
  asr \
  --model "$MODELS_DIR/Qwen3-ASR-1.7B" \
  --served-model-name qwen3-asr-1.7b \
  --context 2048
```
