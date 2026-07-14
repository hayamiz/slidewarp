"""四隅（四角形）の幾何ユーティリティと候補スコアリング。

四角形候補は常に (4, 2) float32 の numpy 配列で表現し、順序は
[左上, 右上, 右下, 左下] (TL, TR, BR, BL) に正規化する。
座標系は入力画像の画素座標（原点=左上）。画角外の負値や幅超過も許容する
（Hough 直線交点によるオクルージョン外挿で四隅が画角外に出るため）。
"""

from __future__ import annotations

import numpy as np

# スライドとして尤もらしいアスペクト比（横/縦）。台形補正後の出力比の推定にも使う。
COMMON_ASPECTS = (4 / 3, 16 / 9, 16 / 10, 3 / 2)


def order_corners(pts: np.ndarray) -> np.ndarray:
    """4点を TL, TR, BR, BL 順に並べ替える。

    重心まわりの偏角でソートして凸多角形の巡回順（画像座標=y下向きでは時計回り）を作り、
    最も左上（x+y 最小）の点を先頭に回す。sum/diff ヒューリスティックは 45 度回転相当の
    四角形で頂点が重複退化するため、偏角ソートで安定させる。
    """
    pts = np.asarray(pts, dtype=np.float32).reshape(4, 2)
    c = pts.mean(axis=0)
    ang = np.arctan2(pts[:, 1] - c[1], pts[:, 0] - c[0])
    pts = pts[np.argsort(ang)]  # -pi..pi 昇順 = TL,TR,BR,BL 相当の時計回り
    start = int(np.argmin(pts.sum(axis=1)))
    pts = np.roll(pts, -start, axis=0)
    return pts.astype(np.float32)


def polygon_area(quad: np.ndarray) -> float:
    """符号なし面積（靴ひも公式）。"""
    q = quad.reshape(4, 2)
    x, y = q[:, 0], q[:, 1]
    return 0.5 * abs(np.dot(x, np.roll(y, -1)) - np.dot(y, np.roll(x, -1)))


def side_lengths(quad: np.ndarray) -> np.ndarray:
    q = quad.reshape(4, 2)
    return np.linalg.norm(q - np.roll(q, -1, axis=0), axis=1)


def _corner_angles(quad: np.ndarray) -> np.ndarray:
    """各頂点の内角（度）。矩形なら全て 90 に近い。"""
    q = quad.reshape(4, 2)
    angles = []
    for i in range(4):
        a = q[(i - 1) % 4] - q[i]
        b = q[(i + 1) % 4] - q[i]
        na, nb = np.linalg.norm(a), np.linalg.norm(b)
        if na < 1e-6 or nb < 1e-6:
            angles.append(0.0)
            continue
        cos = np.clip(np.dot(a, b) / (na * nb), -1.0, 1.0)
        angles.append(np.degrees(np.arccos(cos)))
    return np.array(angles)


def rectangularity(quad: np.ndarray) -> float:
    """内角が 90 度からどれだけずれていないか（1=完全な矩形, 0=退化）。"""
    ang = _corner_angles(quad)
    dev = np.abs(ang - 90.0).mean()
    return float(max(0.0, 1.0 - dev / 45.0))


def estimate_aspect(quad: np.ndarray) -> float:
    """四隅から出力アスペクト比（横/縦）をざっくり推定する。

    対辺の平均長から幅・高さを見積もる簡易版。厳密な消失点補正は行わない。
    """
    s = side_lengths(quad)  # top, right, bottom, left
    width = (s[0] + s[2]) / 2.0
    height = (s[1] + s[3]) / 2.0
    if height < 1e-6:
        return 4 / 3
    return float(width / height)


def snap_aspect(aspect: float, tol: float = 0.12) -> float:
    """推定アスペクト比が定番比に近ければスナップする（スコアリング用）。"""
    for a in COMMON_ASPECTS:
        if abs(aspect - a) / a <= tol:
            return a
    return aspect


# 最終出力で許可するアスペクト比（横/縦）。この2択のいずれかに必ず揃える。
ASPECT_4_3 = 4 / 3
ASPECT_16_9 = 16 / 9


def snap_output_aspect(aspect: float) -> float:
    """見かけアスペクト比を 4:3 か 16:9 の近い方にスナップする（フォールバック用）。

    ・推定値が不自然（クアッドの退化・強い透視で信頼できない範囲）なら判断困難と
      みなしデフォルト 16:9。
    ・妥当な範囲なら対数距離で 4:3 / 16:9 の近い方を選ぶ。
    現在の出力決定は `decide_output_aspect`（透視補正＋確度ゲート）を使う。
    """
    if not np.isfinite(aspect) or not (1.05 < aspect < 2.2):
        return ASPECT_16_9
    d43 = abs(np.log(aspect / ASPECT_4_3))
    d169 = abs(np.log(aspect / ASPECT_16_9))
    return ASPECT_4_3 if d43 < d169 else ASPECT_16_9


def rectified_aspect(
    quad: np.ndarray,
    image_size: tuple[int, int],
) -> tuple[float | None, dict]:
    """Zhang-He whiteboard rectification による真のアスペクト比 (横/縦) の復元。

    主点=画像中心・正方画素を仮定し、対辺の消失点の直交条件から焦点距離を推定して
    透視を補正した w/h を求める。見かけ縦横比（辺長比）は斜め撮影で大きく縮むため、
    出力比の決定にはこの復元値を（確度ゲート付きで）使う。

    quad: (4,2) TL,TR,BR,BL（フルレス画素座標）
    image_size: (width, height)
    戻り値: (aspect | None, info)。None は数値的に復元不可（一点透視・f虚数など）。
        info には persp / f_norm / mode などの診断値が入る。
    """
    w_img, h_img = float(image_size[0]), float(image_size[1])
    cx, cy = w_img / 2.0, h_img / 2.0
    d = float(np.hypot(w_img, h_img))  # 対角長で正規化（数値安定化）
    q = np.asarray(quad, dtype=np.float64).reshape(4, 2)
    tl, tr, br, bl = q

    # Zhang の記法: m1=TL, m2=TR, m3=BL, m4=BR（M1=(0,0),M2=(w,0),M3=(0,h),M4=(w,h)）
    def hom(p):
        return np.array([(p[0] - cx) / d, (p[1] - cy) / d, 1.0])

    m1, m2, m3, m4 = hom(tl), hom(tr), hom(bl), hom(br)

    info: dict = {}
    c14 = np.cross(m1, m4)
    denom_k2 = float(np.dot(np.cross(m2, m4), m3))
    denom_k3 = float(np.dot(np.cross(m3, m4), m2))
    if abs(denom_k2) < 1e-12 or abs(denom_k3) < 1e-12:
        info["mode"] = "degenerate"
        return None, info
    k2 = float(np.dot(c14, m3)) / denom_k2
    k3 = float(np.dot(c14, m2)) / denom_k3
    n2 = k2 * m2 - m1  # 幅方向ベクトル
    n3 = k3 * m3 - m1  # 高さ方向ベクトル

    # 透視の強さ。n2[2]=k2-1, n3[2]=k3-1 が両方 0 なら対辺が平行（正面視）。
    persp = float(max(abs(n2[2]), abs(n3[2])))
    info["persp"] = persp
    eps = 1e-3

    if persp < eps:
        # ほぼ平行四辺形 → f は不定だがアスペクトは f に依存しない
        num = float(np.hypot(n2[0], n2[1]))
        den = float(np.hypot(n3[0], n3[1]))
        if den < 1e-12:
            info["mode"] = "degenerate"
            return None, info
        info["mode"] = "affine"
        return num / den, info

    if abs(n2[2]) < eps or abs(n3[2]) < eps:
        # 片方の消失点だけ無限遠（一点透視）→ f が推定できない
        info["mode"] = "one_point_perspective"
        return None, info

    f2 = -(n2[0] * n3[0] + n2[1] * n3[1]) / (n2[2] * n3[2])
    if not np.isfinite(f2) or f2 <= 0:
        info["mode"] = "f2_nonpositive"
        return None, info
    f = float(np.sqrt(f2))
    info["f_norm"] = f  # f_pixels = f * 対角長
    # スマホの現実範囲（35mm換算 ~13-150mm 相当）のゆるい妥当性チェック
    if not (0.2 <= f <= 3.5):
        info["mode"] = "f_out_of_range"
        return None, info

    w2 = (n2[0] ** 2 + n2[1] ** 2) / f2 + n2[2] ** 2
    h2 = (n3[0] ** 2 + n3[1] ** 2) / f2 + n3[2] ** 2
    if h2 < 1e-12:
        info["mode"] = "degenerate"
        return None, info
    info["mode"] = "full"
    return float(np.sqrt(w2 / h2)), info


# 4:3 を選ぶための確度ゲート（実サンプル22枚＋合成で調整）。
_PERSP_RECTIFIED_MAX = 0.12  # これ以下なら Zhang 復元を信頼
_PERSP_APPARENT_MAX = 0.05   # これ以下なら見かけ比の透視歪み<1%（復元不能時の救済）
_AGREE_LOG_TOL = 0.10        # 復元値と見かけ比の整合チェック
_ASPECT_43_MAX = 1.45        # 4:3 と判定する上限（対数中点1.54より 16:9 側へバイアス）


def decide_output_aspect(quad: np.ndarray, image_size: tuple[int, int]) -> float:
    """出力アスペクト比を 4:3 / 16:9 の2択で決める（方針: 確度が高くない限り 16:9）。

    4:3 を選ぶのは、透視が弱く（見かけ・復元とも信頼できる領域）かつ比が明確に
    4:3 寄りのときだけ。斜め撮影や復元不能時は 16:9 に倒す。
    """
    apparent = estimate_aspect(quad)
    rec, info = rectified_aspect(quad, image_size)
    persp = info.get("persp")

    r = None
    if (
        rec is not None
        and persp is not None
        and persp < _PERSP_RECTIFIED_MAX
        and abs(np.log(max(apparent, 1e-6) / rec)) < _AGREE_LOG_TOL
    ):
        r = rec  # 復元が安定・低透視・見かけ比とも整合 → 高確度
    elif persp is not None and persp < _PERSP_APPARENT_MAX:
        r = apparent  # ほぼ正面: 見かけ比≈真値（復元がノイズで落ちた場合の救済）

    if r is not None and 1.05 < r < _ASPECT_43_MAX:
        return ASPECT_4_3
    return ASPECT_16_9


def is_convex(quad: np.ndarray) -> bool:
    q = quad.reshape(4, 2)
    sign = 0
    for i in range(4):
        a = q[(i + 1) % 4] - q[i]
        b = q[(i + 2) % 4] - q[(i + 1) % 4]
        cross = a[0] * b[1] - a[1] * b[0]
        if abs(cross) < 1e-6:
            continue
        s = 1 if cross > 0 else -1
        if sign == 0:
            sign = s
        elif s != sign:
            return False
    return True


def line_intersection(p1, p2, p3, p4) -> np.ndarray | None:
    """直線 (p1,p2) と (p3,p4) の交点。平行なら None（画角外も許容）。"""
    x1, y1 = p1
    x2, y2 = p2
    x3, y3 = p3
    x4, y4 = p4
    denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4)
    if abs(denom) < 1e-9:
        return None
    px = ((x1 * y2 - y1 * x2) * (x3 - x4) - (x1 - x2) * (x3 * y4 - y3 * x4)) / denom
    py = ((x1 * y2 - y1 * x2) * (y3 - y4) - (y1 - y2) * (x3 * y4 - y3 * x4)) / denom
    return np.array([px, py], dtype=np.float32)
