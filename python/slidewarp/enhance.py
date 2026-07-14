"""画像強調: シャープ化（既定）と、任意の露出補正・色調補正。"""

from __future__ import annotations

import cv2
import numpy as np


def unsharp(image_bgr: np.ndarray, amount: float = 1.0, radius: float = 2.0) -> np.ndarray:
    """アンシャープマスクによるシャープ化。amount=強さ, radius=ガウシアン半径。"""
    blur = cv2.GaussianBlur(image_bgr, (0, 0), radius)
    sharp = cv2.addWeighted(image_bgr, 1.0 + amount, blur, -amount, 0)
    return sharp


def auto_exposure(image_bgr: np.ndarray, clip_limit: float = 2.0) -> np.ndarray:
    """LAB の L チャンネルに CLAHE を掛けて露出/コントラストを自動補正。"""
    lab = cv2.cvtColor(image_bgr, cv2.COLOR_BGR2LAB)
    l, a, b = cv2.split(lab)
    clahe = cv2.createCLAHE(clipLimit=clip_limit, tileGridSize=(8, 8))
    l = clahe.apply(l)
    return cv2.cvtColor(cv2.merge((l, a, b)), cv2.COLOR_LAB2BGR)


def gray_world_wb(image_bgr: np.ndarray) -> np.ndarray:
    """gray-world 仮説によるホワイトバランス補正。"""
    result = image_bgr.astype(np.float32)
    means = result.reshape(-1, 3).mean(axis=0)
    gray = means.mean()
    scale = gray / np.clip(means, 1e-6, None)
    result *= scale
    return np.clip(result, 0, 255).astype(np.uint8)


def enhance(
    image_bgr: np.ndarray,
    *,
    sharpen: float = 1.0,
    exposure: bool = False,
    color: bool = False,
) -> np.ndarray:
    """強調パイプライン。露出/色はオプション。順序: WB → 露出 → シャープ化。"""
    out = image_bgr
    if color:
        out = gray_world_wb(out)
    if exposure:
        out = auto_exposure(out)
    if sharpen > 0:
        out = unsharp(out, amount=sharpen)
    return out
