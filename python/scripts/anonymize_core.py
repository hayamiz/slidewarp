"""公開用サンプル匿名化の純粋関数（幾何・画像・パース・検証・review生成）。

torch は import しない（単体テストを軽量に保つため）。
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

import cv2
import numpy as np


def inset_quad(quad: np.ndarray, inset: float) -> np.ndarray:
    """quad の各頂点を重心方向に inset 割合だけ寄せた (4,2) を返す。

    inset は各辺を内側へ縮める割合（辺ごとに inset 分縮むため、対辺間の
    間隔は全体で 2*inset 縮む）。計画書のテスト期待値
    （100x100 の正方形で inset=0.1 -> 角が (10,10) 等）に整合する係数。
    """
    q = np.asarray(quad, dtype=np.float32)
    c = q.mean(axis=0)
    return (c + (q - c) * (1.0 - 2.0 * float(inset))).astype(np.float32)


def pixelate(img: np.ndarray, block: int) -> np.ndarray:
    """画像を 1/block に縮小し最近傍で元サイズへ戻したブロック化画像を返す。"""
    h, w = img.shape[:2]
    block = max(1, int(block))
    sw, sh = max(1, w // block), max(1, h // block)
    small = cv2.resize(img, (sw, sh), interpolation=cv2.INTER_AREA)
    return cv2.resize(small, (w, h), interpolation=cv2.INTER_NEAREST)


def build_mosaic_mask(shape, inner_quad, face_rects) -> np.ndarray:
    """(h,w) uint8 マスク。モザイク対象=255。"""
    h, w = shape
    mask = np.zeros((h, w), np.uint8)
    if inner_quad is not None:
        cv2.fillConvexPoly(mask, np.asarray(inner_quad, np.int32), 255)
    for (x, y, rw, rh) in face_rects:
        x0, y0 = max(0, x), max(0, y)
        x1, y1 = min(w, x + rw), min(h, y + rh)
        if x1 > x0 and y1 > y0:
            mask[y0:y1, x0:x1] = 255
    return mask


def apply_mosaic(img: np.ndarray, mask: np.ndarray, block: int) -> np.ndarray:
    """mask=255 の画素だけ pixelate 画像に差し替える。"""
    pix = pixelate(img, block)
    out = img.copy()
    sel = mask.astype(bool)
    out[sel] = pix[sel]
    return out


def strip_and_write(img_bgr: np.ndarray, out_path, quality: int = 95) -> None:
    """EXIF を持たない画像として書き出す（cv2.imwrite は EXIF を書かない）。"""
    from pathlib import Path

    p = Path(out_path)
    p.parent.mkdir(parents=True, exist_ok=True)
    params = [cv2.IMWRITE_JPEG_QUALITY, int(quality)] if p.suffix.lower() in (".jpg", ".jpeg") else []
    ok = cv2.imwrite(str(p), img_bgr, params)
    if not ok:
        raise RuntimeError(f"書き出し失敗: {p}")


def face_bands_from_person_mask(person_mask, band, min_area=400):
    """人物マスクの連結成分ごとに bbox 上部 band 割合を顔矩形として返す。"""
    n, _, stats, _ = cv2.connectedComponentsWithStats((person_mask > 0).astype(np.uint8), 8)
    rects = []
    for i in range(1, n):  # 0 は背景
        x, y, w, h, area = stats[i]
        if area < min_area:
            continue
        rects.append((int(x), int(y), int(w), max(1, int(h * float(band)))))
    return rects


@dataclass
class DumpGeom:
    method: str
    aspect: str | None
    quad: np.ndarray | None


def parse_dump_geom(stdout: str) -> dict[str, DumpGeom]:
    result: dict[str, DumpGeom] = {}
    corner_re = re.compile(r"\(-?\d+,-?\d+\)")
    for line in stdout.splitlines():
        line = line.rstrip()
        if not line:
            continue
        if line.startswith("none "):
            name = line[len("none "):].strip()
            result[name] = DumpGeom("none", None, None)
            continue
        m = re.search(r"\[(4:3|16:9)\]\s+quad=(.+?)\s{2,}(.+)$", line)
        if not m:
            continue
        aspect, quad_str, name = m.group(1), m.group(2), m.group(3).strip()
        method = line.split(None, 1)[0]
        pts = [tuple(int(v) for v in c.strip("()").split(",")) for c in corner_re.findall(quad_str)]
        quad = np.array(pts, dtype=np.float32) if len(pts) == 4 else None
        result[name] = DumpGeom(method, aspect, quad)
    return result


def parse_confidence(stdout: str) -> dict[str, float]:
    result: dict[str, float] = {}
    for line in stdout.splitlines():
        m = re.search(r"^\[.{1,6}\]\s+(.+?)\s+conf=([\d.]+)", line)
        if m:
            result[m.group(1).strip()] = float(m.group(2))
    return result
