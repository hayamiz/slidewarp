"""スライド矩形の検出（classical 多段フォールバック）。

暗い会場の中の「明るい矩形」というスライドの強い事前分布を軸に、複数の手法で
四角形候補を生成し、共通のスコア関数で統合する。ML 検出（slidewarp.ml）が
生成したマスク/候補もここに合流させる設計。

生成手法:
  (A) 明度＋エッジ輪郭      : findContours + approxPolyDP
  (B) Hough 直線交点        : 辺の一部しか見えなくても四隅を外挿（オクルージョン対策の要）
  (C) 明度マスク最小外接矩形: minAreaRect フォールバック
"""

from __future__ import annotations

from dataclasses import dataclass, field

import cv2
import numpy as np

from . import geometry as geo


@dataclass
class Candidate:
    quad: np.ndarray  # (4,2) full-res, TL,TR,BR,BL
    method: str
    score: float = 0.0
    parts: dict = field(default_factory=dict)


@dataclass
class DetectionResult:
    quad: np.ndarray | None
    confidence: float
    method: str
    candidates: list[Candidate]


# 検出は縮小画像で行う（高速化）。この長辺(px)に合わせて縮小する。
_WORK_LONG_SIDE = 1000


def _to_work(image: np.ndarray) -> tuple[np.ndarray, float]:
    h, w = image.shape[:2]
    scale = _WORK_LONG_SIDE / max(h, w)
    if scale >= 1.0:
        return image, 1.0
    work = cv2.resize(image, (int(w * scale), int(h * scale)), interpolation=cv2.INTER_AREA)
    return work, scale


def brightness_mask(gray: np.ndarray) -> np.ndarray:
    """明るいスクリーン領域のおおまかなマスク。"""
    blur = cv2.GaussianBlur(gray, (0, 0), 2.0)
    # 大津 + 相対しきい値の緩い方を採用（暗所で大津が過検出/過小になるのを緩和）
    _, otsu = cv2.threshold(blur, 0, 255, cv2.THRESH_BINARY + cv2.THRESH_OTSU)
    thr = max(int(np.percentile(blur, 75)), 90)
    _, rel = cv2.threshold(blur, thr, 255, cv2.THRESH_BINARY)
    mask = cv2.bitwise_or(otsu, rel)
    k = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (9, 9))
    mask = cv2.morphologyEx(mask, cv2.MORPH_CLOSE, k, iterations=2)
    mask = cv2.morphologyEx(mask, cv2.MORPH_OPEN, k, iterations=1)
    return mask


def fill_holes(mask: np.ndarray) -> np.ndarray:
    """マスク内部の閉じた暗領域（スライド内の暗い図版など）を前景で埋める。

    fill ratio 評価用。スライド内部に暗い領域があっても「スライド外」と誤って減点
    しないようにする。外周に開いた暗部（上端の暗いタイトル帯など）は穴ではないので
    埋まらない点に注意（そちらは edge_support 側で対処する）。
    """
    h, w = mask.shape[:2]
    padded = cv2.copyMakeBorder(mask, 1, 1, 1, 1, cv2.BORDER_CONSTANT, value=0)
    ff = padded.copy()
    cv2.floodFill(ff, np.zeros((h + 4, w + 4), np.uint8), (0, 0), 255)
    holes = cv2.bitwise_not(ff)[1:-1, 1:-1]
    return cv2.bitwise_or(mask, holes)


def _edge_profile(
    quad: np.ndarray,
    edges_dil: np.ndarray,
    bright: np.ndarray,
    gray_blur: np.ndarray,
    n: int = 48,
) -> tuple[float, float]:
    """各辺を法線方向にサンプリングし、(edge_support, cut) を返す。

    edge_support: 各辺が実画像エッジに乗る割合。ただし「内側も外側も明るい」内部線は
        本物の外周枠と区別するため係数 0.5 に落とす（方向付き edge_support）。真の枠は
        内側が明るく外側が暗いので満点になり、暗タイトル帯でも枠を拾える。辺ごとの
        値を 0.5*mean + 0.5*min でブレンドし「1辺だけ内部に食い込む」候補を減点。
    cut: 辺が明部を素通しで横切っている度合い（内側=明 かつ 外側=明 かつ 外側に
        コンテンツ状のエッジがある）。スライドの一部だけを囲む小矩形の切断辺で 1 に
        なり、天井/明壁のように外側が平坦な辺では 0 のまま（recall を局所評価するので
        全明部マスクを使う大域 coverage の天井・明壁汚染を避けられる）。

    画角外へ完全に出た辺は外挿由来なので評価から除外し、はみ出し対応を阻害しない。
    """
    h, w = edges_dil.shape[:2]
    q = quad.astype(np.float32)
    d = float(np.clip(0.03 * np.sqrt(max(geo.polygon_area(q), 1.0)), 4.0, 14.0))
    sups, cuts = [], []
    for i in range(4):
        p0, p1 = q[i], q[(i + 1) % 4]
        v = p1 - p0
        length = float(np.hypot(v[0], v[1]))
        if length < 1e-6:
            continue
        nrm = np.array([v[1], -v[0]], np.float32) / length  # 時計回り TL,TR,BR,BL の外向き法線
        pts = p0 + np.linspace(0.03, 0.97, n)[:, None] * v
        ok = (pts[:, 0] >= 0) & (pts[:, 0] < w) & (pts[:, 1] >= 0) & (pts[:, 1] < h)
        if not ok.any():  # 完全に画角外の辺は除外
            continue
        pts = pts[ok]

        def samp(img, off):
            p = np.clip(pts + off * nrm, [0, 0], [w - 1, h - 1]).astype(np.int32)
            return img[p[:, 1], p[:, 0]]

        hit = samp(edges_dil, 0) > 0
        g_in = samp(gray_blur, -d).astype(np.float32)
        g_out = samp(gray_blur, d).astype(np.float32)
        oriented = hit * np.where(g_in - g_out > 10, 1.0, 0.5)  # 内部線(両側明)は半減
        sups.append(float(oriented.mean()))

        b_in = samp(bright, -d) > 0
        b_out = samp(bright, d) > 0
        textured_out = (samp(edges_dil, 2.0 * d) > 0) | (samp(edges_dil, 3.5 * d) > 0)
        cuts.append(float((b_in & b_out & textured_out).mean()))

    if not sups:
        return 0.0, 0.0
    s = np.asarray(sups)
    edge = float(0.5 * s.mean() + 0.5 * s.min())
    cut = float(np.mean(cuts))
    return edge, cut


def score_quad(
    quad: np.ndarray,
    gray: np.ndarray,
    img_area: float,
    mask: np.ndarray | None = None,
    edges_dil: np.ndarray | None = None,
    gray_blur: np.ndarray | None = None,
) -> tuple[float, dict]:
    """四角形候補のスコア（0-1）。

    主要4項が補完関係:
    - fill（精度）: 内部が明度マスクで埋まる割合。暗部・天井を含む緩い矩形を減点。
    - edge（方向付き edge_support）: 各辺が本物の外周枠エッジに乗る度合い。内部線は半減。
    - cut_score（recall の局所版）: 辺が明部を素通しで横切る「一部だけ切り出し」を減点。
      大域 coverage と違い辺の外側近傍だけを見るので天井・明壁で汚染されない。
    - contrast: 内部と外部の明度差。スライド全体は暗い会場に対し高く、内部小矩形は低い。
    透視で傾いたスライドの四隅は 90 度から外れるため rectangularity は退化排除に留める。
    """
    h, w = gray.shape[:2]
    if not geo.is_convex(quad):
        return 0.0, {"reason": "non-convex"}
    area = geo.polygon_area(quad)
    area_ratio = area / img_area
    # 面積: 小さすぎ/大きすぎを減点。0.24 付近を良好域とする。
    if area_ratio < 0.04 or area_ratio > 1.6:
        return 0.0, {"reason": "area", "area_ratio": area_ratio}
    area_score = float(np.clip((area_ratio - 0.04) / 0.2, 0, 1)) * float(
        np.clip((1.6 - area_ratio) / 0.7, 0, 1)
    )

    rect = geo.rectangularity(quad)
    aspect = geo.estimate_aspect(quad)
    snapped = geo.snap_aspect(aspect)
    aspect_score = 1.0 if snapped in geo.COMMON_ASPECTS else float(
        np.clip(1.0 - abs(aspect - 4 / 3) / (4 / 3), 0, 1)
    )

    # 内部マスクを一度だけ作り、明度差と fill ratio の両方に使う。
    # fillConvexPoly は画角外の頂点を自動でクリップするため、外挿された四隅もそのまま
    # 渡してよい（手動クランプは quad∩画角と異なる多角形を作り、外挿候補を歪めるので不要）。
    inside = np.zeros(gray.shape, np.uint8)
    cv2.fillConvexPoly(inside, quad.astype(np.int32), 255)
    inside_area = float(np.count_nonzero(inside)) + 1e-6
    outside = cv2.bitwise_not(inside)
    in_mean = cv2.mean(gray, inside)[0]
    out_mean = cv2.mean(gray, outside)[0] + 1e-6
    contrast = float(np.clip((in_mean - out_mean) / 128.0, 0, 1))

    # fill ratio: 内部のうち明度マスク（=明るいスクリーン領域）に含まれる割合。
    if mask is not None:
        bright_inside = float(np.count_nonzero(cv2.bitwise_and(inside, mask)))
        fill = bright_inside / inside_area
    else:
        fill = 1.0

    if edges_dil is not None and gray_blur is not None and mask is not None:
        edge, cut = _edge_profile(quad, edges_dil, mask, gray_blur)
    else:
        edge, cut = 0.0, 0.0
    cut_score = 1.0 - min(1.0, 1.5 * cut)  # わずかな横断でも強めに効かせる

    score = (
        0.12 * area_score
        + 0.05 * rect
        + 0.06 * aspect_score
        + 0.12 * contrast
        + 0.20 * fill
        + 0.25 * edge
        + 0.20 * cut_score
    )
    parts = {
        "area_ratio": round(area_ratio, 3),
        "area_score": round(area_score, 3),
        "rect": round(rect, 3),
        "aspect": round(aspect, 3),
        "aspect_score": round(aspect_score, 3),
        "contrast": round(contrast, 3),
        "fill": round(fill, 3),
        "edge": round(edge, 3),
        "cut": round(cut, 3),
    }
    return float(score), parts


def _contour_candidates(
    gray: np.ndarray, mask: np.ndarray, ignore: np.ndarray | None = None
) -> list[np.ndarray]:
    quads: list[np.ndarray] = []
    edges = cv2.Canny(gray, 50, 150)
    edges = cv2.dilate(edges, np.ones((3, 3), np.uint8), iterations=1)
    if ignore is not None:
        edges = cv2.bitwise_and(edges, cv2.bitwise_not(ignore))
    for src in (mask, edges):
        contours, _ = cv2.findContours(src, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
        contours = sorted(contours, key=cv2.contourArea, reverse=True)[:6]
        for c in contours:
            peri = cv2.arcLength(c, True)
            for eps in (0.02, 0.04, 0.06):
                approx = cv2.approxPolyDP(c, eps * peri, True)
                if len(approx) == 4:
                    quads.append(geo.order_corners(approx.reshape(4, 2)))
                    break
    return quads


def _minrect_candidate(mask: np.ndarray) -> list[np.ndarray]:
    contours, _ = cv2.findContours(mask, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
    if not contours:
        return []
    c = max(contours, key=cv2.contourArea)
    box = cv2.boxPoints(cv2.minAreaRect(c))
    return [geo.order_corners(box)]


def _seg_len(t) -> float:
    return float(np.hypot(t[2] - t[0], t[3] - t[1]))


def _build_quad(top, bottom, left, right, w, h) -> np.ndarray | None:
    """4辺の直線から四隅（交点）を復元。極端な外挿は棄却。"""
    def pts(t):
        return (np.array(t[:2], np.float32), np.array(t[2:4], np.float32))

    tp, bp, lp, rp = pts(top), pts(bottom), pts(left), pts(right)
    tl = geo.line_intersection(*tp, *lp)
    tr = geo.line_intersection(*tp, *rp)
    br = geo.line_intersection(*bp, *rp)
    bl = geo.line_intersection(*bp, *lp)
    if any(p is None for p in (tl, tr, br, bl)):
        return None
    quad = np.array([tl, tr, br, bl], dtype=np.float32)
    if (quad[:, 0].min() < -0.5 * w or quad[:, 0].max() > 1.5 * w
            or quad[:, 1].min() < -0.5 * h or quad[:, 1].max() > 1.5 * h):
        return None
    return geo.order_corners(quad)


def _dedup_lines(lines: list, pos_fn, min_gap: float) -> list:
    """位置(pos_fn)が近い線を1本に集約し、各クラスタで最長の線を残す。"""
    items = sorted(lines, key=pos_fn)
    kept: list = []
    for s in items:
        p = pos_fn(s)
        for i, k in enumerate(kept):
            if abs(pos_fn(k) - p) < min_gap:
                if _seg_len(s) > _seg_len(k):
                    kept[i] = s
                break
        else:
            kept.append(s)
    return kept


def _hough_candidates(
    gray: np.ndarray, mask: np.ndarray, top_k: int = 4, ignore: np.ndarray | None = None
) -> list[np.ndarray]:
    """スクリーンの矩形境界を Hough 直線で捉え、交点で四隅を復元する。

    ・画像そのもののエッジ（Canny(gray)）を使う。スクリーンのベゼル/枠は明るい会場
      でも強い直線として出るため、明度マスクの輪郭より信頼できる。
    ・明部（=スライド）の外周帯に ROI を限定し、内部の文字や会場の雑多な直線を除外。
    ・角度で水平/垂直に分け、位置クラスタで重複除去。水平2本×垂直2本の総当りで
      四角形を作り、スコア（fill 重視）に選ばせる。特定の辺選択ヒューリスティックや
      明部重心に依存しないため、天井が明部と地続きでも破綻しにくい。
    ・辺の一部しか見えなくても直線を外挿して交点を取れるため、観客の頭による
      オクルージョンやスライドの画角はみ出しに強い（本手法が頑健性の要）。
    """
    h, w = gray.shape[:2]
    # ROI は明部のバウンディングボックスを外側へ広げた矩形にする。暗いタイトル帯や
    # 暗背景があっても、その外側にある本物のスクリーン枠を取りこぼさない。
    cnts, _ = cv2.findContours(mask, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
    if not cnts:
        return []
    bx, by, bw, bh = cv2.boundingRect(max(cnts, key=cv2.contourArea))
    mx, my = int(0.18 * bw), int(0.18 * bh)
    x0, y0 = max(0, bx - mx), max(0, by - my)
    x1, y1 = min(w, bx + bw + mx), min(h, by + bh + my)
    roi = np.zeros((h, w), np.uint8)
    roi[y0:y1, x0:x1] = 255
    edges = cv2.bitwise_and(cv2.Canny(gray, 40, 120), roi)
    if ignore is not None:
        edges = cv2.bitwise_and(edges, cv2.bitwise_not(ignore))

    lines = cv2.HoughLinesP(
        edges, 1, np.pi / 180, threshold=40,
        minLineLength=int(0.12 * max(h, w)), maxLineGap=40,
    )
    if lines is None:
        return []

    horiz, vert = [], []
    for x1, y1, x2, y2 in np.asarray(lines).reshape(-1, 4):
        seg = (int(x1), int(y1), int(x2), int(y2))
        ang = abs(np.degrees(np.arctan2(y2 - y1, x2 - x1)))
        if ang < 35 or ang > 145:
            horiz.append(seg)
        elif 55 < ang < 125:
            vert.append(seg)

    horiz = _dedup_lines(horiz, lambda s: (s[1] + s[3]) / 2.0, min_gap=0.05 * h)
    vert = _dedup_lines(vert, lambda s: (s[0] + s[2]) / 2.0, min_gap=0.05 * w)
    if len(horiz) < 2 or len(vert) < 2:
        return []

    # 帯域層化: 明部 bbox の中線で上下・左右に分け、各帯で長い順に top_k 本残す。
    # 表の罫線など長い内部線が本物の枠線を top_k から押し出すのを防ぎ、上帯×下帯・
    # 左帯×右帯を掛け合わせて四角形を作る（本物の外周線の生存率を上げる）。
    cyb, cxb = by + 0.5 * bh, bx + 0.5 * bw

    def _band(lines, pos, ref):
        lo = sorted((s for s in lines if pos(s) < ref), key=_seg_len, reverse=True)[:top_k]
        hi = sorted((s for s in lines if pos(s) >= ref), key=_seg_len, reverse=True)[:top_k]
        return lo, hi

    top_band, bot_band = _band(horiz, lambda s: (s[1] + s[3]) / 2.0, cyb)
    left_band, right_band = _band(vert, lambda s: (s[0] + s[2]) / 2.0, cxb)

    cands: list[np.ndarray] = []
    if top_band and bot_band and left_band and right_band:
        for top in top_band:
            for bottom in bot_band:
                for left in left_band:
                    for right in right_band:
                        q = _build_quad(top, bottom, left, right, w, h)
                        if q is not None:
                            cands.append(q)
    else:
        # どれかの帯が空なら従来どおり長い順 top_k の全ペア総当りにフォールバック
        hh = sorted(horiz, key=_seg_len, reverse=True)[:top_k]
        vv = sorted(vert, key=_seg_len, reverse=True)[:top_k]
        for i in range(len(hh)):
            for j in range(i + 1, len(hh)):
                top, bottom = sorted((hh[i], hh[j]), key=lambda s: (s[1] + s[3]))
                for a in range(len(vv)):
                    for b in range(a + 1, len(vv)):
                        left, right = sorted((vv[a], vv[b]), key=lambda s: (s[0] + s[2]))
                        q = _build_quad(top, bottom, left, right, w, h)
                        if q is not None:
                            cands.append(q)
    return cands


def detect_slide(
    image_bgr: np.ndarray,
    extra_quads: list[tuple[np.ndarray, str]] | None = None,
    ignore_mask: np.ndarray | None = None,
) -> DetectionResult:
    """スライド矩形を検出する。

    extra_quads: ML 検出等が生成した (quad_fullres, method_name) の候補。
    ignore_mask: 人物など、辺の推定から除外したい領域（元画像サイズの2値マスク）。
        指定すると、遮蔽シルエットのエッジを候補生成・辺サポート評価から除く。
    """
    work, scale = _to_work(image_bgr)
    gray = cv2.cvtColor(work, cv2.COLOR_BGR2GRAY)
    img_area = float(gray.shape[0] * gray.shape[1])
    mask = brightness_mask(gray)
    mask_filled = fill_holes(mask)  # fill 評価はスライド内部の暗図版を減点しないよう穴埋め版で
    gray_blur = cv2.GaussianBlur(gray, (0, 0), 1.5)  # 辺の法線サンプリング用に平滑化

    # 除外マスク（人物等）を work 解像度へ。少し太らせて輪郭ぎわのエッジも除く。
    ignore = None
    if ignore_mask is not None:
        ig = cv2.resize(ignore_mask, (gray.shape[1], gray.shape[0]), interpolation=cv2.INTER_NEAREST)
        ignore = cv2.dilate(ig, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (9, 9)))

    # 採点は必ず「実エッジ」（除外なし）で行う。除外は候補生成にのみ使い、遮蔽で隠れて
    # いた候補を追加で掘り起こす用途に留める。こうすることで、人物が真の枠の近くにある
    # 画像で枠エッジまで消して内側領域に誤ロックするのを防ぎつつ、遮蔽で検出不能だった
    # スライドも救える（採点が公平なので良い候補が勝つ）。
    edges_dil = cv2.dilate(cv2.Canny(gray, 40, 120), np.ones((5, 5), np.uint8), iterations=1)

    raw: list[tuple[np.ndarray, str]] = []
    raw += [(q, "contour") for q in _contour_candidates(gray, mask)]
    raw += [(q, "hough") for q in _hough_candidates(gray, mask)]
    raw += [(q, "minrect") for q in _minrect_candidate(mask)]
    if ignore is not None:  # 遮蔽者のエッジを除いた候補も追加（採点は実エッジで公平に）
        raw += [(q, "contour") for q in _contour_candidates(gray, mask, ignore)]
        raw += [(q, "hough") for q in _hough_candidates(gray, mask, ignore=ignore)]

    # minrect は最終フォールバック。定義上 rect が満点になり過信を招くため信頼度に上限。
    method_factor = {"minrect": 0.6}

    candidates: list[Candidate] = []
    for quad, method in raw:
        score, parts = score_quad(quad, gray, img_area, mask_filled, edges_dil, gray_blur)
        score *= method_factor.get(method, 1.0)
        if score <= 0:
            continue
        full = quad / scale  # 元解像度へ戻す
        candidates.append(Candidate(quad=full, method=method, score=score, parts=parts))

    # ML 等の外部候補は元解像度で来る。スコアは work 解像度に落として評価。
    if extra_quads:
        for full_quad, method in extra_quads:
            wq = full_quad * scale
            score, parts = score_quad(wq, gray, img_area, mask_filled, edges_dil, gray_blur)
            if score <= 0:
                continue
            candidates.append(Candidate(quad=full_quad, method=method, score=score, parts=parts))

    if not candidates:
        return DetectionResult(quad=None, confidence=0.0, method="none", candidates=[])

    candidates.sort(key=lambda c: c.score, reverse=True)
    best = candidates[0]
    return DetectionResult(
        quad=best.quad, confidence=best.score, method=best.method, candidates=candidates
    )
