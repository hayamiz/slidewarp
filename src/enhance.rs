//! 画像強調: シャープ化（既定）と任意の露出補正・色調補正。Python 版 enhance.py 相当。

use image::RgbImage;
use imageproc::filter::gaussian_blur_f32;

/// アンシャープマスク。amount=強さ, radius=ガウシアンσ。
pub fn unsharp(img: &RgbImage, amount: f32, radius: f32) -> RgbImage {
    let blur = gaussian_blur_f32(img, radius);
    let mut out = img.clone();
    for (o, b) in out.pixels_mut().zip(blur.pixels()) {
        for c in 0..3 {
            let v = (1.0 + amount) * o[c] as f32 - amount * b[c] as f32;
            o[c] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// gray-world 仮説によるホワイトバランス補正。
pub fn gray_world_wb(img: &RgbImage) -> RgbImage {
    let mut sum = [0f64; 3];
    let n = (img.width() * img.height()) as f64;
    for p in img.pixels() {
        for c in 0..3 {
            sum[c] += p[c] as f64;
        }
    }
    let mean = [sum[0] / n, sum[1] / n, sum[2] / n];
    let gray = (mean[0] + mean[1] + mean[2]) / 3.0;
    let scale = [
        gray / mean[0].max(1e-6),
        gray / mean[1].max(1e-6),
        gray / mean[2].max(1e-6),
    ];
    let mut out = img.clone();
    for p in out.pixels_mut() {
        for c in 0..3 {
            p[c] = (p[c] as f64 * scale[c]).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// 露出/コントラストの自動補正。輝度ヒストグラム平坦化のゲインを RGB に適用する
/// （色相を保ちつつ明るさだけ補正。CLAHE の代替として簡易版）。
pub fn auto_exposure(img: &RgbImage) -> RgbImage {
    let n = (img.width() * img.height()) as usize;
    let mut hist = [0u32; 256];
    let luma: Vec<u8> = img
        .pixels()
        .map(|p| {
            let l = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
            l.round().clamp(0.0, 255.0) as u8
        })
        .collect();
    for &l in &luma {
        hist[l as usize] += 1;
    }
    // 累積分布 → 平坦化 LUT
    let mut cdf = [0u32; 256];
    let mut acc = 0u32;
    for i in 0..256 {
        acc += hist[i];
        cdf[i] = acc;
    }
    let cdf_min = cdf.iter().copied().find(|&v| v > 0).unwrap_or(0);
    let denom = (n as u32 - cdf_min).max(1);
    let mut lut = [0u8; 256];
    for i in 0..256 {
        lut[i] = (((cdf[i].saturating_sub(cdf_min)) as f32 / denom as f32) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    let mut out = img.clone();
    for (p, &l) in out.pixels_mut().zip(luma.iter()) {
        let target = lut[l as usize] as f32;
        let gain = if l > 0 { target / l as f32 } else { 1.0 };
        for c in 0..3 {
            p[c] = (p[c] as f32 * gain).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

pub fn enhance(img: &RgbImage, sharpen: f32, exposure: bool, color: bool) -> RgbImage {
    let mut out = if color { gray_world_wb(img) } else { img.clone() };
    if exposure {
        out = auto_exposure(&out);
    }
    if sharpen > 0.0 {
        out = unsharp(&out, sharpen, 2.0);
    }
    out
}
