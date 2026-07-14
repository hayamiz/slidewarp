//! スライド矩形の検出（classical 多段フォールバック）。Python 版 detect.py の移植。
//! 候補生成3系統（contour / hough(極線交点) / minrect）を score_quad で統合する。

use image::{GrayImage, Luma, RgbImage};
use imageproc::contours::find_contours;
use imageproc::contrast::otsu_level;
use imageproc::distance_transform::Norm;
use imageproc::edges::canny;
use imageproc::filter::gaussian_blur_f32;
use imageproc::hough::{detect_lines, LineDetectionOptions, PolarLine};
use imageproc::morphology::{close, dilate, open};

use crate::geometry as geo;

const WORK_LONG_SIDE: u32 = 1000;

#[derive(Clone, Default, serde::Serialize)]
pub struct Parts {
    pub area_ratio: f64,
    pub area_score: f64,
    pub rect: f64,
    pub aspect: f64,
    pub aspect_score: f64,
    pub contrast: f64,
    pub fill: f64,
    pub edge: f64,
    pub cut: f64,
}

pub struct Detection {
    pub quad: Option<geo::Quad>,
    pub confidence: f64,
    pub method: String,
    pub parts: Option<Parts>,
}

struct Cand {
    quad: geo::Quad,
    method: &'static str,
    score: f64,
    parts: Parts,
}

fn to_work(img: &RgbImage) -> (GrayImage, f64) {
    let (w, h) = img.dimensions();
    let scale = WORK_LONG_SIDE as f64 / w.max(h) as f64;
    let gray = image::imageops::grayscale(img);
    if scale >= 1.0 {
        return (gray, 1.0);
    }
    let nw = (w as f64 * scale).round() as u32;
    let nh = (h as f64 * scale).round() as u32;
    let resized = image::imageops::resize(&gray, nw, nh, image::imageops::FilterType::Triangle);
    (resized, scale)
}

fn percentile(gray: &GrayImage, p: f64) -> u8 {
    let mut hist = [0u32; 256];
    for px in gray.pixels() {
        hist[px[0] as usize] += 1;
    }
    let total: u32 = hist.iter().sum();
    let target = (total as f64 * p).round() as u32;
    let mut acc = 0u32;
    for (i, &c) in hist.iter().enumerate() {
        acc += c;
        if acc >= target {
            return i as u8;
        }
    }
    255
}

pub fn brightness_mask(gray: &GrayImage) -> GrayImage {
    let blur = gaussian_blur_f32(gray, 2.0);
    let otsu = otsu_level(&blur);
    let thr = (percentile(&blur, 0.75)).max(90);
    let mut mask = GrayImage::new(gray.width(), gray.height());
    for (m, b) in mask.pixels_mut().zip(blur.pixels()) {
        m[0] = if b[0] >= otsu || b[0] >= thr { 255 } else { 0 };
    }
    // close x2 → open x1（楕円9x9 ≒ 半径4）
    let mask = close(&mask, Norm::LInf, 4);
    let mask = close(&mask, Norm::LInf, 4);
    open(&mask, Norm::LInf, 4)
}

/// マスク内部の閉じた暗領域を前景で埋める（外周に開いた暗部は埋めない）。
pub fn fill_holes(mask: &GrayImage) -> GrayImage {
    let (w, h) = mask.dimensions();
    // reachable = 背景(0)で外周から到達可能な領域。holes = 背景 かつ 非到達。
    let mut reachable = vec![false; (w * h) as usize];
    let mut stack: Vec<(u32, u32)> = Vec::new();
    let idx = |x: u32, y: u32| (y * w + x) as usize;
    for x in 0..w {
        for &y in &[0u32, h - 1] {
            if mask.get_pixel(x, y)[0] == 0 && !reachable[idx(x, y)] {
                reachable[idx(x, y)] = true;
                stack.push((x, y));
            }
        }
    }
    for y in 0..h {
        for &x in &[0u32, w - 1] {
            if mask.get_pixel(x, y)[0] == 0 && !reachable[idx(x, y)] {
                reachable[idx(x, y)] = true;
                stack.push((x, y));
            }
        }
    }
    while let Some((x, y)) = stack.pop() {
        let mut push = |nx: u32, ny: u32, st: &mut Vec<(u32, u32)>, rc: &mut Vec<bool>| {
            if mask.get_pixel(nx, ny)[0] == 0 && !rc[idx(nx, ny)] {
                rc[idx(nx, ny)] = true;
                st.push((nx, ny));
            }
        };
        if x > 0 {
            push(x - 1, y, &mut stack, &mut reachable);
        }
        if x + 1 < w {
            push(x + 1, y, &mut stack, &mut reachable);
        }
        if y > 0 {
            push(x, y - 1, &mut stack, &mut reachable);
        }
        if y + 1 < h {
            push(x, y + 1, &mut stack, &mut reachable);
        }
    }
    let mut out = mask.clone();
    for y in 0..h {
        for x in 0..w {
            if mask.get_pixel(x, y)[0] == 0 && !reachable[idx(x, y)] {
                out.put_pixel(x, y, Luma([255]));
            }
        }
    }
    out
}

// ---- 輪郭 → 四角形（Douglas-Peucker） ----

fn perp_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let den = (dx * dx + dy * dy).sqrt();
    if den < 1e-9 {
        return ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt();
    }
    ((dx * (a[1] - p[1]) - (a[0] - p[0]) * dy).abs()) / den
}

fn dp(points: &[[f64; 2]], eps: f64, out: &mut Vec<[f64; 2]>) {
    if points.len() < 2 {
        out.extend_from_slice(points);
        return;
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let mut idx = 0;
    let mut dmax = 0.0;
    for (i, &p) in points.iter().enumerate().take(points.len() - 1).skip(1) {
        let d = perp_dist(p, a, b);
        if d > dmax {
            dmax = d;
            idx = i;
        }
    }
    if dmax > eps {
        dp(&points[..=idx], eps, out);
        out.pop();
        dp(&points[idx..], eps, out);
    } else {
        out.push(a);
        out.push(b);
    }
}

fn approx_quad(contour: &[[f64; 2]]) -> Option<geo::Quad> {
    if contour.len() < 4 {
        return None;
    }
    // 周長
    let mut peri = 0.0;
    for i in 0..contour.len() {
        let a = contour[i];
        let b = contour[(i + 1) % contour.len()];
        peri += ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    }
    let mut closed = contour.to_vec();
    closed.push(contour[0]);
    for frac in [0.02, 0.04, 0.06] {
        let mut out = Vec::new();
        dp(&closed, frac * peri, &mut out);
        if !out.is_empty() && out[0] == *out.last().unwrap() {
            out.pop();
        }
        if out.len() == 4 {
            let q = geo::order_corners(&out);
            if geo::is_convex(&q) {
                return Some(q);
            }
        }
    }
    None
}

fn largest_contours(mask: &GrayImage, top: usize) -> Vec<Vec<[f64; 2]>> {
    let contours = find_contours::<u32>(mask);
    let mut polys: Vec<Vec<[f64; 2]>> = contours
        .into_iter()
        .map(|c| {
            c.points
                .into_iter()
                .map(|p| [p.x as f64, p.y as f64])
                .collect::<Vec<_>>()
        })
        .collect();
    polys.sort_by(|a, b| poly_area(b).partial_cmp(&poly_area(a)).unwrap());
    polys.truncate(top);
    polys
}

fn poly_area(pts: &[[f64; 2]]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        s += a[0] * b[1] - b[0] * a[1];
    }
    s.abs() * 0.5
}

fn contour_candidates(gray: &GrayImage, mask: &GrayImage, edges: &GrayImage) -> Vec<geo::Quad> {
    let _ = gray;
    let mut out = Vec::new();
    for src in [mask, edges] {
        for c in largest_contours(src, 6) {
            if let Some(q) = approx_quad(&c) {
                out.push(q);
            }
        }
    }
    out
}

fn min_area_rect_of(mask: &GrayImage) -> Option<geo::Quad> {
    let cs = largest_contours(mask, 1);
    let c = cs.first()?;
    let pts: Vec<imageproc::point::Point<i32>> = c
        .iter()
        .map(|p| imageproc::point::Point::new(p[0] as i32, p[1] as i32))
        .collect();
    let hull = imageproc::geometry::min_area_rect(&pts);
    let q: Vec<[f64; 2]> = hull.iter().map(|p| [p.x as f64, p.y as f64]).collect();
    Some(geo::order_corners(&q))
}

// ---- Hough（極線）候補 ----

fn bright_bbox(mask: &GrayImage) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = mask.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            if mask.get_pixel(x, y)[0] > 0 {
                any = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if any {
        Some((x0, y0, x1, y1))
    } else {
        None
    }
}

/// 極線 (r,θdeg) の x*cosθ + y*sinθ = r 上の交点。
fn polar_intersection(a: &PolarLine, b: &PolarLine) -> Option<[f64; 2]> {
    let (t1, t2) = (
        (a.angle_in_degrees as f64).to_radians(),
        (b.angle_in_degrees as f64).to_radians(),
    );
    let (c1, s1) = (t1.cos(), t1.sin());
    let (c2, s2) = (t2.cos(), t2.sin());
    let det = c1 * s2 - s1 * c2;
    if det.abs() < 1e-9 {
        return None;
    }
    let x = (a.r as f64 * s2 - b.r as f64 * s1) / det;
    let y = (b.r as f64 * c1 - a.r as f64 * c2) / det;
    Some([x, y])
}

fn hough_candidates(gray: &GrayImage, mask: &GrayImage, ignore: Option<&GrayImage>) -> Vec<geo::Quad> {
    let (w, h) = gray.dimensions();
    let (bx, by, bw, bh) = match bright_bbox(mask) {
        Some((x0, y0, x1, y1)) => (x0, y0, x1 - x0 + 1, y1 - y0 + 1),
        None => return Vec::new(),
    };
    let mx = (0.18 * bw as f64) as i32;
    let my = (0.18 * bh as f64) as i32;
    let x0 = (bx as i32 - mx).max(0) as u32;
    let y0 = (by as i32 - my).max(0) as u32;
    let x1 = ((bx + bw) as i32 + mx).min(w as i32) as u32;
    let y1 = ((by + bh) as i32 + my).min(h as i32) as u32;

    let mut edges = canny(gray, 40.0, 120.0);
    // ROI 外と ignore 領域を消す
    for y in 0..h {
        for x in 0..w {
            let mut keep = x >= x0 && x < x1 && y >= y0 && y < y1;
            if keep {
                if let Some(ig) = ignore {
                    if ig.get_pixel(x, y)[0] > 0 {
                        keep = false;
                    }
                }
            }
            if !keep {
                edges.put_pixel(x, y, Luma([0]));
            }
        }
    }

    let vote = ((0.12 * w.max(h) as f64) as u32).max(40);
    let lines = detect_lines(
        &edges,
        LineDetectionOptions {
            vote_threshold: vote,
            suppression_radius: 12,
        },
    );
    // 角度で H/V に分類（θ=normal angle: 縦線→θ≈0/180, 横線→θ≈90）
    let cx = bx as f64 + bw as f64 / 2.0;
    let cy = by as f64 + bh as f64 / 2.0;
    let mut horiz: Vec<PolarLine> = Vec::new();
    let mut vert: Vec<PolarLine> = Vec::new();
    for l in lines {
        let a = l.angle_in_degrees as f64;
        if a < 35.0 || a > 145.0 {
            vert.push(l);
        } else if a > 55.0 && a < 125.0 {
            horiz.push(l);
        }
    }
    // 位置（水平線→中央xでのy、垂直線→中央yでのx）
    let hy = |l: &PolarLine| {
        let t = (l.angle_in_degrees as f64).to_radians();
        (l.r as f64 - cx * t.cos()) / t.sin()
    };
    let vx = |l: &PolarLine| {
        let t = (l.angle_in_degrees as f64).to_radians();
        (l.r as f64 - cy * t.sin()) / t.cos()
    };
    // detect_lines は投票の強い順。帯（上下・左右）に分けて各 top_k。
    let top_k = 4usize;
    let mut top: Vec<PolarLine> = Vec::new();
    let mut bot: Vec<PolarLine> = Vec::new();
    for l in &horiz {
        if hy(l) < cy {
            if top.len() < top_k {
                top.push(*l);
            }
        } else if bot.len() < top_k {
            bot.push(*l);
        }
    }
    let mut left: Vec<PolarLine> = Vec::new();
    let mut right: Vec<PolarLine> = Vec::new();
    for l in &vert {
        if vx(l) < cx {
            if left.len() < top_k {
                left.push(*l);
            }
        } else if right.len() < top_k {
            right.push(*l);
        }
    }

    let mut cands = Vec::new();
    let build = |t: &PolarLine, b: &PolarLine, le: &PolarLine, ri: &PolarLine| -> Option<geo::Quad> {
        let tl = polar_intersection(t, le)?;
        let tr = polar_intersection(t, ri)?;
        let br = polar_intersection(b, ri)?;
        let bl = polar_intersection(b, le)?;
        let q = geo::order_corners(&[tl, tr, br, bl]);
        // 極端な外挿は棄却
        for p in q.iter() {
            if p[0] < -0.5 * w as f64
                || p[0] > 1.5 * w as f64
                || p[1] < -0.5 * h as f64
                || p[1] > 1.5 * h as f64
            {
                return None;
            }
        }
        Some(q)
    };
    if !top.is_empty() && !bot.is_empty() && !left.is_empty() && !right.is_empty() {
        for t in &top {
            for b in &bot {
                for le in &left {
                    for ri in &right {
                        if let Some(q) = build(t, b, le, ri) {
                            cands.push(q);
                        }
                    }
                }
            }
        }
    }
    cands
}

// ---- スコアリング ----

fn edge_profile(quad: &geo::Quad, edges_dil: &GrayImage, bright: &GrayImage, gray_blur: &GrayImage) -> (f64, f64) {
    let (w, h) = edges_dil.dimensions();
    let n = 48;
    let d = (0.03 * geo::polygon_area(quad).max(1.0).sqrt()).clamp(4.0, 14.0);
    let mut sups: Vec<f64> = Vec::new();
    let mut cuts: Vec<f64> = Vec::new();
    let sample = |img: &GrayImage, x: f64, y: f64| -> u8 {
        let xi = x.round().clamp(0.0, (w - 1) as f64) as u32;
        let yi = y.round().clamp(0.0, (h - 1) as f64) as u32;
        img.get_pixel(xi, yi)[0]
    };
    for i in 0..4 {
        let p0 = quad[i];
        let p1 = quad[(i + 1) % 4];
        let v = [p1[0] - p0[0], p1[1] - p0[1]];
        let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
        if len < 1e-6 {
            continue;
        }
        let nrm = [v[1] / len, -v[0] / len]; // 時計回りの外向き法線
        let mut hit_sum = 0.0;
        let mut cut_sum = 0.0;
        let mut cnt = 0.0;
        for j in 0..n {
            let t = 0.03 + (0.94) * j as f64 / (n - 1) as f64;
            let px = p0[0] + t * v[0];
            let py = p0[1] + t * v[1];
            if px < 0.0 || px >= w as f64 || py < 0.0 || py >= h as f64 {
                continue;
            }
            cnt += 1.0;
            let hit = sample(edges_dil, px, py) > 0;
            let g_in = sample(gray_blur, px - d * nrm[0], py - d * nrm[1]) as f64;
            let g_out = sample(gray_blur, px + d * nrm[0], py + d * nrm[1]) as f64;
            let oriented = if hit {
                if g_in - g_out > 10.0 {
                    1.0
                } else {
                    0.5
                }
            } else {
                0.0
            };
            hit_sum += oriented;
            let b_in = sample(bright, px - d * nrm[0], py - d * nrm[1]) > 0;
            let b_out = sample(bright, px + d * nrm[0], py + d * nrm[1]) > 0;
            let tex = sample(edges_dil, px + 2.0 * d * nrm[0], py + 2.0 * d * nrm[1]) > 0
                || sample(edges_dil, px + 3.5 * d * nrm[0], py + 3.5 * d * nrm[1]) > 0;
            if b_in && b_out && tex {
                cut_sum += 1.0;
            }
        }
        if cnt > 0.0 {
            sups.push(hit_sum / cnt);
            cuts.push(cut_sum / cnt);
        }
    }
    if sups.is_empty() {
        return (0.0, 0.0);
    }
    let mean = sups.iter().sum::<f64>() / sups.len() as f64;
    let min = sups.iter().cloned().fold(f64::INFINITY, f64::min);
    let edge = 0.5 * mean + 0.5 * min;
    let cut = cuts.iter().sum::<f64>() / cuts.len() as f64;
    (edge, cut)
}

fn count_inside(quad: &geo::Quad, w: u32, h: u32, mask: Option<&GrayImage>) -> (f64, f64, f64, f64) {
    // 返り値: inside_area, bright_inside, in_sum, out_sum(輝度は呼び出し側で別途) — ここでは面積系のみ
    // 走査線で内部画素を数える
    let ys: Vec<f64> = quad.iter().map(|p| p[1]).collect();
    let ymin = ys.iter().cloned().fold(f64::INFINITY, f64::min).floor().max(0.0) as i32;
    let ymax = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil().min((h - 1) as f64) as i32;
    let mut inside_area = 0.0;
    let mut bright_inside = 0.0;
    for y in ymin..=ymax {
        // 多角形の走査線交点
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..4 {
            let a = quad[i];
            let b = quad[(i + 1) % 4];
            let (y0, y1) = (a[1], b[1]);
            if (y0 <= y as f64 && (y as f64) < y1) || (y1 <= y as f64 && (y as f64) < y0) {
                let t = (y as f64 - y0) / (y1 - y0);
                xs.push(a[0] + t * (b[0] - a[0]));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut k = 0;
        while k + 1 < xs.len() {
            let x0 = xs[k].ceil().max(0.0) as i32;
            let x1 = xs[k + 1].floor().min((w - 1) as f64) as i32;
            for x in x0..=x1 {
                inside_area += 1.0;
                if let Some(m) = mask {
                    if m.get_pixel(x as u32, y as u32)[0] > 0 {
                        bright_inside += 1.0;
                    }
                }
            }
            k += 2;
        }
    }
    (inside_area, bright_inside, 0.0, 0.0)
}

fn contrast_of(quad: &geo::Quad, gray: &GrayImage) -> f64 {
    let (w, h) = gray.dimensions();
    // 内部平均と外部平均。内部は走査線、外部は全体から内部を引く。
    let mut in_sum = 0f64;
    let mut in_cnt = 0f64;
    let ys: Vec<f64> = quad.iter().map(|p| p[1]).collect();
    let ymin = ys.iter().cloned().fold(f64::INFINITY, f64::min).floor().max(0.0) as i32;
    let ymax = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil().min((h - 1) as f64) as i32;
    for y in ymin..=ymax {
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..4 {
            let a = quad[i];
            let b = quad[(i + 1) % 4];
            let (y0, y1) = (a[1], b[1]);
            if (y0 <= y as f64 && (y as f64) < y1) || (y1 <= y as f64 && (y as f64) < y0) {
                let t = (y as f64 - y0) / (y1 - y0);
                xs.push(a[0] + t * (b[0] - a[0]));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut k = 0;
        while k + 1 < xs.len() {
            let x0 = xs[k].ceil().max(0.0) as i32;
            let x1 = xs[k + 1].floor().min((w - 1) as f64) as i32;
            for x in x0..=x1 {
                in_sum += gray.get_pixel(x as u32, y as u32)[0] as f64;
                in_cnt += 1.0;
            }
            k += 2;
        }
    }
    let total: f64 = gray.pixels().map(|p| p[0] as f64).sum();
    let total_cnt = (w * h) as f64;
    let out_cnt = (total_cnt - in_cnt).max(1.0);
    let out_sum = total - in_sum;
    let in_mean = if in_cnt > 0.0 { in_sum / in_cnt } else { 0.0 };
    let out_mean = out_sum / out_cnt + 1e-6;
    ((in_mean - out_mean) / 128.0).clamp(0.0, 1.0)
}

pub fn score_quad(
    quad: &geo::Quad,
    w: u32,
    h: u32,
    gray: &GrayImage,
    mask_filled: &GrayImage,
    edges_dil: &GrayImage,
    gray_blur: &GrayImage,
) -> Option<(f64, Parts)> {
    if !geo::is_convex(quad) {
        return None;
    }
    let img_area = (w * h) as f64;
    let area = geo::polygon_area(quad);
    let area_ratio = area / img_area;
    if area_ratio < 0.04 || area_ratio > 1.6 {
        return None;
    }
    let area_score = ((area_ratio - 0.04) / 0.2).clamp(0.0, 1.0) * ((1.6 - area_ratio) / 0.7).clamp(0.0, 1.0);
    let rect = geo::rectangularity(quad);
    let aspect = geo::estimate_aspect(quad);
    let aspect_score = if geo::snap_aspect(aspect).is_some() {
        1.0
    } else {
        (1.0 - (aspect - 4.0 / 3.0).abs() / (4.0 / 3.0)).clamp(0.0, 1.0)
    };
    let contrast = contrast_of(quad, gray);
    let (inside_area, bright_inside, _, _) = count_inside(quad, w, h, Some(mask_filled));
    let fill = if inside_area > 0.0 {
        bright_inside / inside_area
    } else {
        0.0
    };
    let (edge, cut) = edge_profile(quad, edges_dil, mask_filled, gray_blur);
    let cut_score = 1.0 - (1.5 * cut).min(1.0);

    let score = 0.12 * area_score
        + 0.05 * rect
        + 0.06 * aspect_score
        + 0.12 * contrast
        + 0.20 * fill
        + 0.25 * edge
        + 0.20 * cut_score;
    let parts = Parts {
        area_ratio,
        area_score,
        rect,
        aspect,
        aspect_score,
        contrast,
        fill,
        edge,
        cut,
    };
    Some((score, parts))
}

/// 検出確定後の上辺リファイン: 上辺が「内側=本文（明部）・外側=タイトル帯（本文より暗い）」の
/// 本文上境界に張り付いているとき、法線外側へ帯を走査し、帯を区切る枠エッジまで上辺をスナップする。
/// 帯判定は gray ベース（行平均輝度が本体基準より暗い間は帯）。close で帯がマスク上「明」に
/// 化けるケース（薄暗い青帯・黒帯+白文字）でも発火する。
/// 帯に文字等のコンテンツ（Canny エッジ密度が中間の行）がある場合のみ発火し、空の非投影
/// マージンやレターボックス（内容損失ゼロ）は触らない＝損失非対称の設計で回帰を防ぐ。
fn refine_top_edge(
    quad: &geo::Quad,
    mask_raw: &GrayImage,
    edges_dil: &GrayImage,
    edges_raw: &GrayImage,
    gray_blur: &GrayImage,
) -> Option<(geo::Quad, f64)> {
    let (w, h) = mask_raw.dimensions();
    let (tl, tr, br, bl) = (quad[0], quad[1], quad[2], quad[3]);
    let v = [tr[0] - tl[0], tr[1] - tl[1]];
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len < 1e-6 {
        return None;
    }
    let nrm = [v[1] / len, -v[0] / len]; // 上向き（外向き）法線
    let height = (((bl[0] - tl[0]).powi(2) + (bl[1] - tl[1]).powi(2)).sqrt()
        + ((br[0] - tr[0]).powi(2) + (br[1] - tr[1]).powi(2)).sqrt())
        / 2.0;
    let n = 48;
    // 上辺を off だけ法線方向へ平行移動した線分上で img>0 の画素割合。画角内 30% 未満は -1。
    let frac_at = |off: f64, img: &GrayImage| -> f64 {
        let (mut hits, mut cnt) = (0.0, 0.0);
        for j in 0..n {
            let t = 0.05 + 0.90 * j as f64 / (n - 1) as f64;
            let px = tl[0] + t * v[0] + off * nrm[0];
            let py = tl[1] + t * v[1] + off * nrm[1];
            if px < 0.0 || px >= w as f64 || py < 0.0 || py >= h as f64 {
                continue;
            }
            cnt += 1.0;
            if img.get_pixel(px as u32, py as u32)[0] > 0 {
                hits += 1.0;
            }
        }
        if cnt < n as f64 * 0.3 {
            return -1.0;
        }
        hits / cnt
    };
    // 同じ線分上の gray 平均輝度。画角内 30% 未満は -1。
    let mean_at = |off: f64| -> f64 {
        let (mut sum, mut cnt) = (0.0, 0.0);
        for j in 0..n {
            let t = 0.05 + 0.90 * j as f64 / (n - 1) as f64;
            let px = tl[0] + t * v[0] + off * nrm[0];
            let py = tl[1] + t * v[1] + off * nrm[1];
            if px < 0.0 || px >= w as f64 || py < 0.0 || py >= h as f64 {
                continue;
            }
            cnt += 1.0;
            sum += gray_blur.get_pixel(px as u32, py as u32)[0] as f64;
        }
        if cnt < n as f64 * 0.3 {
            return -1.0;
        }
        sum / cnt
    };
    // 本体輝度基準: 上辺内側 -6/-12px の明るい方（直下に暗い罫線があっても拾えるように）。
    let g_body = mean_at(-6.0).max(mean_at(-12.0));
    if g_body < 0.0 {
        return None;
    }
    const BAND_DELTA: f64 = 12.0; // 「本体より暗い」とみなす輝度差
    // 前提条件: 内側(6px)が明部、外側(6px)が「マスク非明部」または「gray で本体より暗い」
    // →「本文上境界」に座っている兆候。白スクリーンの外側余白（外側も明・同輝度）はここで除外。
    if frac_at(-6.0, mask_raw) < 0.6 {
        return None;
    }
    let out_mask = frac_at(6.0, mask_raw);
    let g_out = mean_at(6.0);
    let outer_is_band = out_mask <= 0.45 || (g_out >= 0.0 && g_out <= g_body - BAND_DELTA);
    if !outer_is_band {
        return None;
    }
    let o_max = 0.24 * height;
    let mut p1: Vec<f64> = Vec::new(); // 帯内の強直線
    let mut p2: Option<f64> = None; // 明るさ再出現の境界（帯→天井/明壁/非投影マージン）
    let mut contents: Vec<f64> = Vec::new(); // 帯内の「コンテンツ行」= タイトル文字・境界遷移
    let mut seen_dark = false; // マスク非明の行を通過したか
    let mut run_start: Option<f64> = None; // 暗部後に再出現した mask明 run の開始オフセット
    let mut o = 4.0;
    while o <= o_max {
        let gm = mean_at(o);
        if gm < 0.0 {
            break;
        }
        let mf = frac_at(o, mask_raw);
        let gray_dark = gm <= g_body - BAND_DELTA;
        let ed_raw = frac_at(o, edges_raw);
        let ed_dil = frac_at(o, edges_dil);
        if mf > 0.75 {
            // 明るさ完全再出現（gray も本体並み）→ 帯終端。境界に枠エッジがあれば P2。
            if !gray_dark {
                let pb = run_start.unwrap_or(o);
                if frac_at(pb, edges_dil) >= 0.5 {
                    p2 = Some(pb);
                }
                break;
            }
            // 暗部を挟んで mask明が再出現（gray はまだ暗い）: 文字テクスチャ（中間の raw
            // Canny 密度）がある間はタイトル帯として継続、無ければ非投影マージン/天井 → 終端。
            if seen_dark {
                if run_start.is_none() {
                    run_start = Some(o);
                }
                if !(0.10..=0.90).contains(&ed_raw) {
                    let pb = run_start.unwrap_or(o);
                    if frac_at(pb, edges_dil) >= 0.5 {
                        p2 = Some(pb);
                    }
                    break;
                }
            }
            // seen_dark 無し（close で帯自体がマスク明: 薄暗青帯・黒帯+白文字）は
            // gray が暗い間、帯として継続する。
        } else {
            run_start = None;
            if mf >= 0.0 && mf <= 0.45 {
                seen_dark = true;
            }
        }
        // コンテンツ行: エッジ活動を伴う半明行（文字・境界遷移）、または
        // 「マスク明だが gray 暗」の帯内に中間の raw Canny 密度（=文字テクスチャ）がある行。
        // エッジ皆無の半明行（投影光のスピル・ぼけたレターボックス）は数えない。
        let is_content = ((0.10..=0.75).contains(&mf) && ed_dil >= 0.30)
            || (mf > 0.75 && gray_dark && (0.10..=0.90).contains(&ed_raw));
        if is_content {
            contents.push(o);
        }
        if ed_dil >= 0.55 {
            p1.push(o);
        }
        o += 2.0;
    }
    let last_content = contents.last().copied();
    if contents.len() < 2 {
        return None; // 空帯（非投影マージン・レターボックス・ベゼルのみ）は触らない（回帰防止の要）
    }
    // P1 検証: 線のさらに外側(+6/+12px)がマスク非明かつ静か（エッジ無し）なら最外周の枠。
    let quiet_dark_at = |oc: f64| -> bool {
        [6.0, 12.0].iter().all(|&e| {
            let b = frac_at(oc + e, mask_raw);
            let ed = frac_at(oc + e, edges_dil);
            b < 0.0 || (b < 0.30 && ed >= 0.0 && ed < 0.25)
        })
    };
    let p1_valid = p1
        .iter()
        .copied()
        .filter(|&oc| quiet_dark_at(oc))
        .fold(None::<f64>, |acc, oc| Some(acc.map_or(oc, |a| a.max(oc))));
    // P3（最終フォールバック）: 枠線が弱く P1/P2 が取れない帯は「最外コンテンツ行 + 6px」。
    // 外側が元からマスク非明（暗帯）で、コンテンツが十分厚く、スナップ先のさらに外側が
    // マスクでも暗く静かな場合に限る（マスク明の天井・非投影マージンへは絶対に伸ばさない）。
    let p3 = last_content.and_then(|lc| {
        let oc = lc + 6.0;
        if out_mask > 0.45 || contents.len() < 4 || oc > o_max {
            return None;
        }
        if quiet_dark_at(oc) {
            Some(oc)
        } else {
            None
        }
    });
    let o = p2.or(p1_valid).or(p3)?;
    if o < 6.0 {
        return None;
    }
    // コンテンツはスナップ線より内側（帯の中）に 2 行以上あること。境界の遷移行だけで
    // 発火しない（損失非対称: 帯に実コンテンツがある時だけ切り出しを広げる）。
    if contents.iter().filter(|&&c| c < o).count() < 2 {
        return None;
    }
    let t0 = [tl[0] + o * nrm[0], tl[1] + o * nrm[1]];
    let t1 = [tr[0] + o * nrm[0], tr[1] + o * nrm[1]];
    let ntl = geo::line_intersection(t0, t1, bl, tl)?;
    let ntr = geo::line_intersection(t0, t1, br, tr)?;
    let q = geo::order_corners(&[ntl, ntr, br, bl]);
    if !geo::is_convex(&q) {
        return None;
    }
    Some((q, o))
}

/// 検出確定後の下辺リファイン（明部継続版・refine_top_edge と対の損失非対称設計）:
/// 投影輝度の落ち込み（プロジェクタ下部の減光）で明度マスクがスライド下部を早期に
/// 切り落とすと、下辺は「内側=明・外側も本体並みに明るい（スライドが続いている）」に
/// 張り付く。その場合のみ、gray 輝度が本体基準から暗転する位置まで下辺を延ばす。
/// 下辺が正しい画像は外側（会場・壁・机・観客）が即座に大きく暗転するため発火しない。
/// 左右半分を独立に走査して終端を推定し、辺の傾き残り（回転ずれ）にも追従する。
fn refine_bottom_edge(quad: &geo::Quad, gray_blur: &GrayImage) -> Option<geo::Quad> {
    let (w, h) = gray_blur.dimensions();
    let (tl, tr, br, bl) = (quad[0], quad[1], quad[2], quad[3]);
    let v = [bl[0] - br[0], bl[1] - br[1]]; // 時計回り: br→bl（t=0 が右下、t=1 が左下）
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len < 1e-6 {
        return None;
    }
    let nrm = [v[1] / len, -v[0] / len]; // 外向き（下）法線
    let height = (((bl[0] - tl[0]).powi(2) + (bl[1] - tl[1]).powi(2)).sqrt()
        + ((br[0] - tr[0]).powi(2) + (br[1] - tr[1]).powi(2)).sqrt())
        / 2.0;
    let n = 24;
    // 下辺を off だけ外へ平行移動した線分の t∈[t0,t1] 区間の gray 平均輝度。画角内 60% 未満は -1。
    let mean_at = |off: f64, t0: f64, t1: f64| -> f64 {
        let (mut sum, mut cnt) = (0.0, 0.0);
        for j in 0..n {
            let t = t0 + (t1 - t0) * j as f64 / (n - 1) as f64;
            let px = br[0] + t * v[0] + off * nrm[0];
            let py = br[1] + t * v[1] + off * nrm[1];
            if px < 0.0 || px >= w as f64 || py < 0.0 || py >= h as f64 {
                continue;
            }
            cnt += 1.0;
            sum += gray_blur.get_pixel(px as u32, py as u32)[0] as f64;
        }
        if cnt < n as f64 * 0.6 {
            return -1.0;
        }
        sum / cnt
    };
    // 本体輝度基準（下辺内側）。本体が明るいスライドのみ対象。
    let g_body = mean_at(-6.0, 0.05, 0.95).max(mean_at(-12.0, 0.05, 0.95));
    if g_body < 120.0 {
        return None;
    }
    // 発火条件: 外側が本体並みに明るい（=マスク落ちでスライドを切っている兆候）。
    // 正しい下辺の外側（会場・壁・机）は大きく暗い（実測: 他画像は -60 以上の暗転）。
    const CONT_DELTA: f64 = 22.0; // 「本体並み」とみなす許容差
    const DARK_DELTA: f64 = 45.0; // 「暗転した＝スライド終端」とみなす輝度差
    let g_out = mean_at(6.0, 0.05, 0.95).max(mean_at(14.0, 0.05, 0.95));
    if g_out < 0.0 || g_out < g_body - CONT_DELTA {
        return None;
    }
    // 左右半分ごとに「暗転する位置」まで走査（終端が見つからなければ発火しない）。
    let o_max = 0.4 * height;
    let walk = |t0: f64, t1: f64| -> Option<f64> {
        let mut o = 4.0;
        while o <= o_max {
            let gm = mean_at(o, t0, t1);
            if gm >= 0.0 && gm <= g_body - DARK_DELTA {
                return Some((o - 2.0).max(0.0)); // 最後の明行へスナップ
            }
            if gm < 0.0 {
                // 画角外へ出た: スライドが画角下端まで続いている → 画角端相当で打ち切り
                return Some((o - 2.0).max(0.0));
            }
            o += 2.0;
        }
        None // 明部が続いたまま終端不明 → 触らない（明壁など不確実ケースの保護）
    };
    let o_r = walk(0.05, 0.50)?; // 右半分（br 側）
    let o_l = walk(0.50, 0.95)?; // 左半分（bl 側）
    if o_r.max(o_l) < 12.0 || (o_r - o_l).abs() > 60.0 {
        return None; // 延長が僅か / 左右の食い違いが大きすぎる（誤検知保護）
    }
    // 半区間の重心 t=0.275 / 0.725 の測定値から辺両端 (t=0/1) へ線形外挿。
    let slope = (o_l - o_r) / 0.45;
    let o_at = |t: f64| (o_r + slope * (t - 0.275)).clamp(0.0, o_max);
    let nb0 = [br[0] + o_at(0.0) * nrm[0], br[1] + o_at(0.0) * nrm[1]];
    let nb1 = [bl[0] + o_at(1.0) * nrm[0], bl[1] + o_at(1.0) * nrm[1]];
    let nbr = geo::line_intersection(nb0, nb1, tr, br)?;
    let nbl = geo::line_intersection(nb0, nb1, tl, bl)?;
    let q = geo::order_corners(&[tl, tr, nbr, nbl]);
    if !geo::is_convex(&q) {
        return None;
    }
    Some(q)
}

pub fn detect_slide(img: &RgbImage, ignore_mask: Option<&GrayImage>) -> Detection {
    let (gray, scale) = to_work(img);
    let (w, h) = gray.dimensions();
    let mask = brightness_mask(&gray);
    let mask_filled = fill_holes(&mask);
    let gray_blur = gaussian_blur_f32(&gray, 1.5);
    let edges_base = canny(&gray, 50.0, 150.0);
    let edges_lo = canny(&gray, 40.0, 120.0);
    let edges_dil = dilate(&edges_lo, Norm::LInf, 2);

    // ignore を work 解像度へ
    let ignore_work: Option<GrayImage> = ignore_mask.map(|ig| {
        let r = image::imageops::resize(ig, w, h, image::imageops::FilterType::Nearest);
        dilate(&r, Norm::LInf, 4)
    });
    let ignore_ref = ignore_work.as_ref();

    // 候補生成
    let mut raw: Vec<(geo::Quad, &'static str)> = Vec::new();
    for q in contour_candidates(&gray, &mask, &edges_base) {
        raw.push((q, "contour"));
    }
    for q in hough_candidates(&gray, &mask, None) {
        raw.push((q, "hough"));
    }
    if let Some(q) = min_area_rect_of(&mask) {
        raw.push((q, "minrect"));
    }
    if let Some(ig) = ignore_ref {
        for q in contour_candidates(&gray, &mask, &{
            let mut e = edges_base.clone();
            for y in 0..h {
                for x in 0..w {
                    if ig.get_pixel(x, y)[0] > 0 {
                        e.put_pixel(x, y, Luma([0]));
                    }
                }
            }
            e
        }) {
            raw.push((q, "contour"));
        }
        for q in hough_candidates(&gray, &mask, Some(ig)) {
            raw.push((q, "hough"));
        }
    }

    // 候補は work 座標で保持し、選択後に上辺リファインを掛けてから元解像度へ戻す。
    let mut best: Option<Cand> = None;
    for (q, method) in raw {
        if let Some((mut score, parts)) = score_quad(&q, w, h, &gray, &mask_filled, &edges_dil, &gray_blur) {
            if method == "minrect" {
                score *= 0.6;
            }
            if score <= 0.0 {
                continue;
            }
            if best.as_ref().map(|c| score > c.score).unwrap_or(true) {
                best = Some(Cand {
                    quad: q,
                    method,
                    score,
                    parts,
                });
            }
        }
    }

    match best {
        Some(mut c) => {
            // 上辺が本文上境界に張り付き暗いタイトル帯を切っている場合、真の上端へ延ばす。
            if let Some((rq, _)) = refine_top_edge(&c.quad, &mask, &edges_dil, &edges_lo, &gray_blur) {
                c.quad = rq;
            }
            // 下辺の外側が本体並みに明るいまま続く場合（マスクの早期切り）、暗転位置まで延ばす。
            if let Some(rq) = refine_bottom_edge(&c.quad, &gray_blur) {
                c.quad = rq;
            }
            let full: geo::Quad = [
                [c.quad[0][0] / scale, c.quad[0][1] / scale],
                [c.quad[1][0] / scale, c.quad[1][1] / scale],
                [c.quad[2][0] / scale, c.quad[2][1] / scale],
                [c.quad[3][0] / scale, c.quad[3][1] / scale],
            ];
            Detection {
                quad: Some(full),
                confidence: c.score,
                method: c.method.to_string(),
                parts: Some(c.parts),
            }
        }
        None => Detection {
            quad: None,
            confidence: 0.0,
            method: "none".to_string(),
            parts: None,
        },
    }
}
