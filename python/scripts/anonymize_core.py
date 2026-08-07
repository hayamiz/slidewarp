"""公開用サンプル匿名化の純粋関数（幾何・画像・パース・検証・review生成）。

torch は import しない（単体テストを軽量に保つため）。
"""

from __future__ import annotations

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
