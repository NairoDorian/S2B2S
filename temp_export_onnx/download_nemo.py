from huggingface_hub import hf_hub_download
import os

repo_id = "nvidia/parakeet-unified-en-0.6b"
filename = "parakeet-unified-en-0.6b.nemo"

print(f"Downloading {repo_id}/{filename} (with resume)...")
path = hf_hub_download(
    repo_id=repo_id,
    filename=filename,
    local_dir=".",
    local_dir_use_symlinks=False,
    resume_download=True,
)

size_gb = os.path.getsize(path) / 1024**3
print(f"Done: {size_gb:.2f} GB at {path}")
