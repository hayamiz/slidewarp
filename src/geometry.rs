//! 四隅（四角形）の幾何ユーティリティ。Python 版 geometry.py の移植。
//! Quad は常に [左上, 右上, 右下, 左下] (TL,TR,BR,BL) の順に正規化する。

pub type Point = [f64; 2];
pub type Quad = [Point; 4];

pub const ASPECT_4_3: f64 = 4.0 / 3.0;
pub const ASPECT_16_9: f64 = 16.0 / 9.0;
pub const COMMON_ASPECTS: [f64; 4] = [4.0 / 3.0, 16.0 / 9.0, 16.0 / 10.0, 3.0 / 2.0];

/// 4点を重心まわりの偏角でソートし、最も左上（x+y 最小）を先頭にして TL,TR,BR,BL 順にする。
/// sum/diff ヒューリスティックは 45 度回転で退化するため偏角ソートで安定化。
pub fn order_corners(pts: &[Point]) -> Quad {
    let cx = pts.iter().map(|p| p[0]).sum::<f64>() / pts.len() as f64;
    let cy = pts.iter().map(|p| p[1]).sum::<f64>() / pts.len() as f64;
    let mut v: Vec<Point> = pts.to_vec();
    v.sort_by(|a, b| {
        let aa = (a[1] - cy).atan2(a[0] - cx);
        let ab = (b[1] - cy).atan2(b[0] - cx);
        aa.partial_cmp(&ab).unwrap()
    });
    // x+y 最小の点を先頭へ回す
    let start = (0..v.len())
        .min_by(|&i, &j| (v[i][0] + v[i][1]).partial_cmp(&(v[j][0] + v[j][1])).unwrap())
        .unwrap();
    let mut q = [[0.0; 2]; 4];
    for i in 0..4 {
        q[i] = v[(start + i) % v.len()];
    }
    q
}

pub fn polygon_area(q: &Quad) -> f64 {
    let mut s = 0.0;
    for i in 0..4 {
        let a = q[i];
        let b = q[(i + 1) % 4];
        s += a[0] * b[1] - b[0] * a[1];
    }
    s.abs() * 0.5
}

pub fn side_lengths(q: &Quad) -> [f64; 4] {
    let mut s = [0.0; 4];
    for i in 0..4 {
        let a = q[i];
        let b = q[(i + 1) % 4];
        s[i] = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    }
    s
}

/// 内角が 90 度からどれだけずれていないか（1=矩形, 0=退化）。
pub fn rectangularity(q: &Quad) -> f64 {
    let mut dev = 0.0;
    for i in 0..4 {
        let a = [q[(i + 3) % 4][0] - q[i][0], q[(i + 3) % 4][1] - q[i][1]];
        let b = [q[(i + 1) % 4][0] - q[i][0], q[(i + 1) % 4][1] - q[i][1]];
        let na = (a[0] * a[0] + a[1] * a[1]).sqrt();
        let nb = (b[0] * b[0] + b[1] * b[1]).sqrt();
        if na < 1e-6 || nb < 1e-6 {
            return 0.0;
        }
        let cos = ((a[0] * b[0] + a[1] * b[1]) / (na * nb)).clamp(-1.0, 1.0);
        dev += (cos.acos().to_degrees() - 90.0).abs();
    }
    (1.0 - (dev / 4.0) / 45.0).max(0.0)
}

/// 見かけアスペクト比（横/縦）: 対辺平均長の比。
pub fn estimate_aspect(q: &Quad) -> f64 {
    let s = side_lengths(q);
    let width = (s[0] + s[2]) / 2.0;
    let height = (s[1] + s[3]) / 2.0;
    if height < 1e-6 {
        return ASPECT_4_3;
    }
    width / height
}

pub fn is_convex(q: &Quad) -> bool {
    let mut sign = 0i32;
    for i in 0..4 {
        let a = [q[(i + 1) % 4][0] - q[i][0], q[(i + 1) % 4][1] - q[i][1]];
        let b = [
            q[(i + 2) % 4][0] - q[(i + 1) % 4][0],
            q[(i + 2) % 4][1] - q[(i + 1) % 4][1],
        ];
        let cross = a[0] * b[1] - a[1] * b[0];
        if cross.abs() < 1e-9 {
            continue;
        }
        let s = if cross > 0.0 { 1 } else { -1 };
        if sign == 0 {
            sign = s;
        } else if s != sign {
            return false;
        }
    }
    true
}

/// 2直線（各々2点で定義）の交点。平行なら None。
pub fn line_intersection(p1: Point, p2: Point, p3: Point, p4: Point) -> Option<Point> {
    let d = (p1[0] - p2[0]) * (p3[1] - p4[1]) - (p1[1] - p2[1]) * (p3[0] - p4[0]);
    if d.abs() < 1e-9 {
        return None;
    }
    let a = p1[0] * p2[1] - p1[1] * p2[0];
    let b = p3[0] * p4[1] - p3[1] * p4[0];
    let px = (a * (p3[0] - p4[0]) - (p1[0] - p2[0]) * b) / d;
    let py = (a * (p3[1] - p4[1]) - (p1[1] - p2[1]) * b) / d;
    Some([px, py])
}

/// 見かけアスペクトを定番比に近ければスナップ（スコアリング用）。
pub fn snap_aspect(aspect: f64) -> Option<f64> {
    for a in COMMON_ASPECTS {
        if (aspect - a).abs() / a <= 0.12 {
            return Some(a);
        }
    }
    None
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub struct RectInfo {
    pub persp: Option<f64>,
}

/// Zhang-He whiteboard rectification による真アスペクト比復元。
/// 主点=画像中心・正方画素を仮定し、消失点の直交条件で焦点距離を推定して w/h を復元。
/// 復元不能（一点透視・f虚数など）は None。
pub fn rectified_aspect(q: &Quad, image_size: (u32, u32)) -> (Option<f64>, RectInfo) {
    let (w_img, h_img) = (image_size.0 as f64, image_size.1 as f64);
    let cx = w_img / 2.0;
    let cy = h_img / 2.0;
    let d = (w_img * w_img + h_img * h_img).sqrt();
    let hom = |p: Point| [(p[0] - cx) / d, (p[1] - cy) / d, 1.0];
    // m1=TL, m2=TR, m3=BL, m4=BR
    let m1 = hom(q[0]);
    let m2 = hom(q[1]);
    let m3 = hom(q[3]);
    let m4 = hom(q[2]);

    let c14 = cross3(m1, m4);
    let denom_k2 = dot3(cross3(m2, m4), m3);
    let denom_k3 = dot3(cross3(m3, m4), m2);
    if denom_k2.abs() < 1e-12 || denom_k3.abs() < 1e-12 {
        return (None, RectInfo { persp: None });
    }
    let k2 = dot3(c14, m3) / denom_k2;
    let k3 = dot3(c14, m2) / denom_k3;
    let n2 = [k2 * m2[0] - m1[0], k2 * m2[1] - m1[1], k2 * m2[2] - m1[2]];
    let n3 = [k3 * m3[0] - m1[0], k3 * m3[1] - m1[1], k3 * m3[2] - m1[2]];

    let persp = n2[2].abs().max(n3[2].abs());
    let info = RectInfo { persp: Some(persp) };
    let eps = 1e-3;

    if persp < eps {
        // ほぼ平行四辺形 → f 不要でアスペクト算出
        let num = (n2[0] * n2[0] + n2[1] * n2[1]).sqrt();
        let den = (n3[0] * n3[0] + n3[1] * n3[1]).sqrt();
        if den < 1e-12 {
            return (None, info);
        }
        return (Some(num / den), info);
    }
    if n2[2].abs() < eps || n3[2].abs() < eps {
        return (None, info); // 一点透視
    }
    let f2 = -(n2[0] * n3[0] + n2[1] * n3[1]) / (n2[2] * n3[2]);
    if !f2.is_finite() || f2 <= 0.0 {
        return (None, info);
    }
    let f = f2.sqrt();
    if !(0.2..=3.5).contains(&f) {
        return (None, info);
    }
    let w2 = (n2[0] * n2[0] + n2[1] * n2[1]) / f2 + n2[2] * n2[2];
    let h2 = (n3[0] * n3[0] + n3[1] * n3[1]) / f2 + n3[2] * n3[2];
    if h2 < 1e-12 {
        return (None, info);
    }
    (Some((w2 / h2).sqrt()), info)
}

const PERSP_RECTIFIED_MAX: f64 = 0.12;
const PERSP_APPARENT_MAX: f64 = 0.05;
const AGREE_LOG_TOL: f64 = 0.10;
const ASPECT_43_MAX: f64 = 1.45;

/// 出力アスペクト比を 4:3 / 16:9 の2択で決める（方針: 確度が高くない限り 16:9）。
pub fn decide_output_aspect(q: &Quad, image_size: (u32, u32)) -> f64 {
    let apparent = estimate_aspect(q);
    let (rec, info) = rectified_aspect(q, image_size);
    let persp = info.persp;

    let mut r: Option<f64> = None;
    if let (Some(rv), Some(pv)) = (rec, persp) {
        if pv < PERSP_RECTIFIED_MAX
            && (apparent.max(1e-6) / rv).ln().abs() < AGREE_LOG_TOL
        {
            r = Some(rv);
        }
    }
    if r.is_none() {
        if let Some(pv) = persp {
            if pv < PERSP_APPARENT_MAX {
                r = Some(apparent);
            }
        }
    }
    match r {
        Some(rv) if rv > 1.05 && rv < ASPECT_43_MAX => ASPECT_4_3,
        _ => ASPECT_16_9,
    }
}
