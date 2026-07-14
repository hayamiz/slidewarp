"""台形補正（透視変換）。検出した四隅を矩形へ写像する。"""

from __future__ import annotations

import cv2
import numpy as np

from . import geometry as geo


def warp_to_rect(
    image_bgr: np.ndarray,
    quad: np.ndarray,
    max_long_side: int = 1600,
    person_mask: np.ndarray | None = None,
    margin: float = 0.03,
) -> np.ndarray:
    """四隅 quad(TL,TR,BR,BL) を、4:3 または 16:9 の矩形へ透視変換する。

    出力アスペクト比はスライドの見かけの縦横比から推定し、必ず 4:3 か 16:9 の
    いずれかにスナップする（推定が難しい場合はデフォルト 16:9）。

    margin: 検出した矩形を各辺 margin(比率) だけ外側へ広げてから切り出す。トリミング後の
    画像だけでスライド全体が収まっているか判断できるよう、周辺マージンを少し含める。

    person_mask を渡すと、同じ変換でマスクも写像し、切り出し内に残った人物領域を
    inpaint で除去する（遮蔽対策）。
    """
    quad = geo.order_corners(quad).astype(np.float32)

    # 出力比・サイズは元 quad 基準で決める（マージン拡大の影響を受けないように）。
    h_img, w_img = image_bgr.shape[:2]
    aspect = geo.decide_output_aspect(quad, (w_img, h_img))
    s = geo.side_lengths(quad)
    width_px = max(s[0], s[2])
    height_px = max(s[1], s[3])
    base = float(np.sqrt(max(width_px * height_px, 1.0)))
    out_w = base * np.sqrt(aspect)
    out_h = base / np.sqrt(aspect)

    # 長辺を max_long_side に収める
    scale = max_long_side / max(out_w, out_h)
    if scale < 1.0:
        out_w *= scale
        out_h *= scale
    out_w = int(round(out_w))
    out_h = int(round(out_h))

    # マージン: 重心から等倍拡大して各辺を margin だけ外へ広げた quad を src に使う
    # （出力矩形は据え置きなので、スライドが内側に縮み周辺マージンが入る）。
    src = quad
    if margin > 0:
        c = quad.mean(axis=0)
        src = (c + (quad - c) * (1.0 + 2.0 * margin)).astype(np.float32)

    dst = np.array(
        [[0, 0], [out_w - 1, 0], [out_w - 1, out_h - 1], [0, out_h - 1]], dtype=np.float32
    )
    M = cv2.getPerspectiveTransform(src, dst)
    # 四隅が画角外へ外挿された場合、その領域は黒(0)で埋める（縁の引き伸ばしを避ける）。
    warped = cv2.warpPerspective(
        image_bgr, M, (out_w, out_h), flags=cv2.INTER_CUBIC,
        borderMode=cv2.BORDER_CONSTANT, borderValue=(0, 0, 0),
    )
    if person_mask is not None:
        wm = cv2.warpPerspective(
            person_mask, M, (out_w, out_h), flags=cv2.INTER_NEAREST,
            borderMode=cv2.BORDER_CONSTANT, borderValue=0,
        )
        wm = cv2.dilate(wm, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (7, 7)))
        if cv2.countNonZero(wm) > 0:
            warped = cv2.inpaint(warped, wm, 5, cv2.INPAINT_TELEA)
    return warped
