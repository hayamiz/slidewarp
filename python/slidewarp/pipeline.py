"""1枚の写真に対する処理パイプライン。

read → (ML候補) → detect → 信頼度判定 → warp → enhance → write
低信頼時は誤補正を出さない安全側ポリシー（skip / copy）。
"""

from __future__ import annotations

import shutil
from dataclasses import dataclass, field
from pathlib import Path

import cv2
import numpy as np

from . import detect as detect_mod
from . import enhance as enhance_mod
from . import io_utils
from . import warp as warp_mod
from .ml import MLScreenDetector


@dataclass
class ProcessConfig:
    out_dir: Path
    min_confidence: float = 0.35
    on_low_confidence: str = "skip"  # "skip" | "copy"
    sharpen: float = 1.0
    exposure: bool = False
    color: bool = False
    max_long_side: int = 1600
    margin: float = 0.03
    out_ext: str = ".jpg"
    jpeg_quality: int = 95


@dataclass
class ProcessResult:
    src: Path
    status: str  # "ok" | "low_confidence" | "no_detection" | "error"
    out_path: Path | None = None
    confidence: float = 0.0
    method: str = ""
    message: str = ""
    parts: dict = field(default_factory=dict)


def _imread(path: Path) -> np.ndarray | None:
    # 日本語パス対策で np.fromfile 経由で読む
    data = np.fromfile(str(path), dtype=np.uint8)
    if data.size == 0:
        return None
    return cv2.imdecode(data, cv2.IMREAD_COLOR)


def _imwrite(path: Path, image: np.ndarray, quality: int) -> None:
    ext = path.suffix.lower()
    params = [cv2.IMWRITE_JPEG_QUALITY, quality] if ext in (".jpg", ".jpeg") else []
    ok, buf = cv2.imencode(ext, image, params)
    if not ok:
        raise RuntimeError(f"encode failed: {path}")
    buf.tofile(str(path))


def process_image(
    src: Path,
    cfg: ProcessConfig,
    ml_detector: MLScreenDetector | None = None,
    person_segmenter=None,
) -> ProcessResult:
    try:
        image = _imread(src)
        if image is None:
            return ProcessResult(src, "error", message="読み込み失敗")

        person_mask = person_segmenter.mask(image) if person_segmenter is not None else None
        extra = ml_detector.detect(image) if ml_detector is not None else None
        det = detect_mod.detect_slide(image, extra_quads=extra, ignore_mask=person_mask)

        if det.quad is None:
            return _handle_reject(src, image, cfg, "no_detection", 0.0)
        if det.confidence < cfg.min_confidence:
            return _handle_reject(
                src, image, cfg, "low_confidence", det.confidence, det.method
            )

        warped = warp_mod.warp_to_rect(
            image, det.quad, max_long_side=cfg.max_long_side, person_mask=person_mask,
            margin=cfg.margin
        )
        result = enhance_mod.enhance(
            warped, sharpen=cfg.sharpen, exposure=cfg.exposure, color=cfg.color
        )
        out_path = io_utils.output_path(src, cfg.out_dir, ext=cfg.out_ext)
        _imwrite(out_path, result, cfg.jpeg_quality)
        return ProcessResult(
            src, "ok", out_path=out_path, confidence=det.confidence,
            method=det.method, parts=det.candidates[0].parts if det.candidates else {},
        )
    except Exception as e:  # バッチを止めない
        return ProcessResult(src, "error", message=f"{type(e).__name__}: {e}")


def _handle_reject(
    src: Path, image: np.ndarray, cfg: ProcessConfig, status: str, conf: float, method: str = ""
) -> ProcessResult:
    if cfg.on_low_confidence == "copy":
        review_dir = cfg.out_dir / "_review"
        out_path = io_utils.output_path(src, review_dir, suffix="_orig", ext=src.suffix.lower())
        # 再エンコードせず原本をそのままコピー（EXIF/撮影日時を保持、劣化なし）
        shutil.copy2(src, out_path)
        return ProcessResult(src, status, out_path=out_path, confidence=conf, method=method,
                             message="低信頼のため原本を _review へ退避")
    return ProcessResult(src, status, confidence=conf, method=method, message="低信頼のためスキップ")
