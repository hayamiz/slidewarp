"""入力収集と出力パス決定。"""

from __future__ import annotations

from pathlib import Path

# OpenCV のビルドが HEIF/HEIC をデコードできないことが多いため既定では含めない
# （含めると読み込み失敗でエラー扱いになる）。必要なら別途変換してから渡す。
IMAGE_EXTS = {".jpg", ".jpeg", ".png", ".bmp", ".tif", ".tiff", ".webp"}


def collect_inputs(paths: list[str]) -> list[Path]:
    """ファイル/フォルダ混在の入力から画像ファイル一覧を収集（フォルダは再帰）。"""
    result: list[Path] = []
    seen: set[Path] = set()
    for p in paths:
        path = Path(p)
        if path.is_dir():
            for f in sorted(path.rglob("*")):
                if f.is_file() and f.suffix.lower() in IMAGE_EXTS:
                    rp = f.resolve()
                    if rp not in seen:
                        seen.add(rp)
                        result.append(f)
        elif path.is_file() and path.suffix.lower() in IMAGE_EXTS:
            rp = path.resolve()
            if rp not in seen:
                seen.add(rp)
                result.append(path)
    return result


def output_path(src: Path, out_dir: Path, suffix: str = "", ext: str = ".jpg") -> Path:
    """出力パスを決める。名前衝突時は連番を付与。"""
    out_dir.mkdir(parents=True, exist_ok=True)
    stem = src.stem + suffix
    candidate = out_dir / f"{stem}{ext}"
    i = 1
    while candidate.exists():
        candidate = out_dir / f"{stem}_{i}{ext}"
        i += 1
    return candidate
